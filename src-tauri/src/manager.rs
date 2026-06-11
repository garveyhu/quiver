//! 经理控制循环 + Effect 执行层(DESIGN §5)。
//!
//! 这是把**自治引擎接到真实运行路径**的那一层 —— 此前 `quiver-orchestrator` 只是纯逻辑
//! 状态机(`Decision`/`Effect`/`tick`),算出"该干啥"却没人执行;`scheduler.rs` 那条无脑
//! 流水线(有名额+有排队就跑)又绕过了经理。本模块让经理**真的拍决策、决策真的驱动调动**:
//!
//! ```text
//! 经理控制循环(每 project 一个 tokio task,§5.6 异步事件循环):
//!   loop {
//!     1. 组装 ManagerContext  ← store 读 在途/排队/预算
//!     2. decide(不持 orch 锁,等 claude 想)  →  apply(持锁瞬间分配 node/fence/seq)
//!     3. 执行 step.effect(Spawn 真起 worker / Deliver / Block / …) + emit 决策给前端工作台
//!     4. effect==Nothing 且无在途 → 退出(enqueue/settings 变更重启);否则等 worker 唤醒
//!   }
//!   worker 完成 → on_complete 释放名额 + 唤醒经理再 tick
//! ```
//!
//! **并发由 orchestrator 的 `InFlight`(`max_inflight`)把关**,不再用 scheduler 的 `Semaphore`
//! (避免双重记账,见 plan 关键设计点 1)。`GitGuard`(共享 `.git` 串行)、`run_one_task`
//! (跑 claude→verify→merge 的执行原语)照旧复用。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Notify};

use quiver_core::git::GitGuard;
use quiver_core::merge::MergeLock;
use quiver_orchestrator::{
    Decision, Effect, ManagerContext, Orchestrator, PendingReview, Step,
};
use quiver_store::{Settings, Store};

use crate::run::{resolve_agent_bin, RunMode};
use crate::scheduler::TASK_EVENT_CHANNEL;

// 按职责拆出的子模块(单一职责):ctx 组装只读 helper、决策→前端工作台投影、worker 生命周期 +
// 自愈、交付合并 + 记忆沉淀。核心循环 + execute_effect 留在本根模块;子模块用 `super::` 拿根的
// 类型/常量(Rust 后代可见父私有项)。
mod context;
mod delivery;
mod events;
mod worker;
use context::{
    budget_remaining, count_queued, goal_progress, memory_brief, peek_next_task, team_specialties,
};
use delivery::{deliver_merge, record_decision_episode, record_failure_lesson};
use events::emit_decision;
use worker::{auto_retry, kill_orphan_worker, spawn_worker, MAX_AUTO_RETRY};

/// 前端「经理工作台」订阅的决策流 channel:经理每拍 emit 一条
/// 决策(动作 + 理由 + 调动的真任务),让人**亲眼看到经理在自治**。
pub const MANAGER_DECISION_CHANNEL: &str = "manager-decision";

/// 经理**思考流** channel(可见性):经理用 claude 决策时,每吐一段思考就 emit 一条 —— 让
/// CEO 点开经理就能实时看到它在想什么(不再是黑箱)。与最终决策(上面那条)分开。
pub const MANAGER_THINKING_CHANNEL: &str = "manager-thinking";

/// 经理满载/在途未空时一拍的兜底等待。唤醒主要靠 worker 完工的 `notify`,这只是防丢
/// 唤醒的保险 —— 放宽到 5s:别让超时拍刷屏决策流,真大脑下每一拍都是决策调用(钱)。
const TICK_IDLE_TIMEOUT: Duration = Duration::from_secs(5);
/// 一次经理循环生命周期的决策拍数硬上限(DESIGN §5.8「保证停下来」的简版决策税):
/// 不管 AI 怎么绕圈,拍数见底强制停 —— 杜绝任何形式的空转/升级风暴把额度烧穿。
/// 队列重新有活时 enqueue 会重启新循环,所以这只挡风暴、不挡正常工作。
const MAX_TICKS_PER_LOOP: u64 = 200;
/// 同一任务经理「continue 续跑改进」的轮次硬上限(§5.8 防失控):救到这一轮还没达标 → 不再烧钱
/// 续跑,摘出复核队、标 needs_rebase 上报 CEO。claude 经理再想 continue 也挡得住 —— 自治不靠
/// 经理自觉收手,系统兜底保证「救不动就停、不无限烧」(预算闸只挡总额,这道挡单任务空耗)。
const MAX_CONTINUE_ROUNDS: u32 = 3;
/// 纯目标自治启动时,经理 noop(没主动 plan)的重试上限 + 间隔:real claude 第一拍可能保守观望就
/// noop,别一 noop 就退出、让「给方向就自治」看 claude 心情失灵。给几拍机会(prompt 已强令第一批
/// 别 noop),限次防真要收工时无限空转。
const MAX_GOAL_START_RETRY: u32 = 3;
const GOAL_START_RETRY_GAP: Duration = Duration::from_secs(2);

/// 经理编排运行时,挂在 `AppState`(类比 [`Scheduler`](crate::scheduler::Scheduler))。
/// 每个 project 一个 [`ProjectManager`](懒创建、单循环)。
#[derive(Default)]
pub struct ManagerLoop {
    projects: Mutex<HashMap<String, Arc<ProjectManager>>>,
}

/// 单 project 的经理运行时:它的 orchestrator(在途/栅栏/saga seq)、共享 git guard、
/// `node_id→task_id` 映射、worker 完成的唤醒信号、"循环在跑"标志(防重入)。
struct ProjectManager {
    project: PathBuf,
    guard: Arc<GitGuard>,
    /// 纯内存编排状态机。放 `Mutex` 因 tick(apply)与 worker 回流(on_complete)都要 `&mut`。
    orch: Mutex<Orchestrator>,
    /// orchestrator 分配的抽象 `node-N` ↔ 实际跑的 `task_id`(§5 执行层映射)。
    node_tasks: Mutex<HashMap<String, String>>,
    /// 完工待经理复核裁决的 worker(§5):完成回流时入队,经理 Deliver/Block 时出队。
    reviews: Mutex<Vec<PendingReview>>,
    /// §5.7 合并列车锁:经理交付时合并到 main 全局串行(一次一个),防并行合并互踩。
    merge_lock: MergeLock,
    /// 建循环时定的在途上限(也喂给 `ManagerContext`,与 `InFlight.max` 保持一致)。
    max_inflight: usize,
    /// worker 完成 → `notify_one` 唤醒经理再 tick(§5.6 不卡等)。
    wake: Notify,
    loop_running: Mutex<bool>,
}

impl ManagerLoop {
    /// 确保 `project` 的经理循环在跑:懒创建运行时,再(重)启循环(幂等,照
    /// [`Scheduler::ensure_running`](crate::scheduler::Scheduler) 模式)。经理大脑**不在这里
    /// 注入**——循环每拍现场按人事部配置选(见 [`manager_loop`]),改配置下一拍即生效。
    pub async fn ensure_running(
        &self,
        app: AppHandle,
        store: Arc<Store>,
        project: PathBuf,
        max_inflight: usize,
    ) {
        let key = project.display().to_string();
        let cap = max_inflight.max(1);
        // §5.2 序号续编:决策序号从持久化日志 max+1 接着编,跨重启单调不回卷。
        let seq_start = store
            .max_decision_seq(&key)
            .ok()
            .flatten()
            .map(|m| (m + 1).max(0) as u64)
            .unwrap_or(0);
        let pm = {
            let mut projects = self.projects.lock().await;
            projects
                .entry(key)
                .or_insert_with(|| {
                    Arc::new(ProjectManager {
                        project: project.clone(),
                        guard: Arc::new(GitGuard::new(project.clone())),
                        orch: Mutex::new(Orchestrator::with_seq_start(cap, seq_start)),
                        node_tasks: Mutex::new(HashMap::new()),
                        reviews: Mutex::new(Vec::new()),
                        merge_lock: MergeLock::new(),
                        max_inflight: cap,
                        wake: Notify::new(),
                        loop_running: Mutex::new(false),
                    })
                })
                .clone()
        };
        Self::spawn_loop_if_idle(app, store, pm).await;
    }

    /// 重启所有已知 project 的经理循环(已在跑的 no-op)。settings 变更后调 —— 比如调高/清
    /// 预算后,被预算挡退出的经理该复活继续派活。幂等 + 便宜。
    pub async fn resume_all(&self, app: AppHandle, store: Arc<Store>) {
        let pms: Vec<Arc<ProjectManager>> = {
            let projects = self.projects.lock().await;
            projects.values().cloned().collect()
        };
        for pm in pms {
            Self::spawn_loop_if_idle(app.clone(), store.clone(), pm.clone()).await;
            // 已在跑但等着的循环也戳醒:配置(预算/角色大脑)变了,这一拍的判断可能不同。
            pm.wake.notify_one();
        }
    }

    /// 启动崩溃恢复(DESIGN §23 P0,自治版):上个会话可能死在半路,留下卡在 `running` 的
    /// 任务和孤儿工作区。requeue 它们、清扫孤儿,然后由**经理循环**接管 pending 项目 ——
    /// 自治模式下重启后组织仍由经理驱动(派活/复核/留痕),不降级回无脑流水线。
    /// best-effort:单个项目出错不挡其他项目恢复。返回 requeue 的任务数。
    pub async fn reconcile(&self, app: AppHandle, store: Arc<Store>, max_workers: usize) -> usize {
        // §23 崩溃恢复卫生:上个会话被强杀时,running 任务的 worker(claude)子进程成了孤儿、
        // 还在跑、还烧额度。它们的 PID 持久化在 task 表里 —— 重启后先把活着的孤儿 kill 掉
        // (验证确是 claude,防 PID 复用误杀),再 requeue 任务重跑。
        for (_tid, pid) in store.running_task_pids().unwrap_or_default() {
            kill_orphan_worker(pid);
        }
        let requeued = store.requeue_running_tasks(crate::now_ms()).unwrap_or(0);
        let projects = store.projects_with_pending_tasks().unwrap_or_default();
        for project in projects {
            let path = PathBuf::from(&project);
            let _ = GitGuard::new(path.clone()).sweep_orphan_worktrees().await;
            self.ensure_running(app.clone(), store.clone(), path, max_workers)
                .await;
        }
        requeued
    }

    /// 启动 `pm` 的经理循环,除非已有一个在跑。标志在 mutex 下 check-and-set,并发 enqueue
    /// 至多起一个循环。
    async fn spawn_loop_if_idle(app: AppHandle, store: Arc<Store>, pm: Arc<ProjectManager>) {
        {
            let mut running = pm.loop_running.lock().await;
            if *running {
                return;
            }
            *running = true;
        }
        let pm_for_loop = pm.clone();
        tokio::spawn(async move {
            manager_loop(app, store, pm_for_loop.clone()).await;
            // 标空闲,下次 enqueue/resume 能重启。
            *pm_for_loop.loop_running.lock().await = false;
        });
    }
}

/// 经理控制循环本体(§5.6):一拍拍地"看局面→拍决策→执行→记录→等唤醒"。
async fn manager_loop(app: AppHandle, store: Arc<Store>, pm: Arc<ProjectManager>) {
    let project_key = pm.project.display().to_string();
    let mut ticks: u64 = 0;
    // 纯目标自治启动时经理连续 noop 的计数(给 real claude 几拍 plan 机会,见退出逻辑)。
    let mut idle_noops: u32 = 0;
    loop {
        // §5.8 风暴熔断:拍数见底强制停(enqueue 重启新循环)。真大脑每拍都可能花钱,
        // 这道闸保证"不管经理怎么绕圈,循环总会停"。
        ticks += 1;
        if ticks > MAX_TICKS_PER_LOOP {
            break;
        }
        // 1. 组装这一拍的局面(操作真值,实时读 orchestration.db,§5.1)。
        let settings = store.get_settings().ok();
        // §7 并发上限实时生效:每拍读当前 max_workers 同步进 orchestrator 的 InFlight ——
        // 人事部/设置调并发立即影响下一拍能 admit 几个,不必重建经理循环(轮3 缓存坑修复)。
        let cur_max = settings
            .as_ref()
            .map(|s| (s.max_workers.max(1)) as usize)
            .unwrap_or(pm.max_inflight);
        let inflight = {
            let mut orch = pm.orch.lock().await;
            orch.set_max_inflight(cur_max);
            orch.inflight_len()
        };
        let queued = count_queued(&store, &project_key);
        let budget_remaining_usd = budget_remaining(&store, settings.as_ref());
        let ctx = ManagerContext {
            inflight,
            queued,
            max_inflight: cur_max,
            budget_remaining_usd,
            // §6 记忆简报:当前事实 + 近期 episode,让经理"带着项目记忆"决策(RuleBrain 不读它,
            // ClaudeBrain 读 —— 这是"记忆 → 决策"的接缝)。读失败给空,不挡决策。
            brief: memory_brief(&app, &project_key),
            // §5 复核裁决:完工等裁的 worker,经理先裁后派。
            pending_reviews: pm.reviews.lock().await.clone(),
            // §5 A2 拆活:队首任务原文给经理看 —— 真经理可结合记忆改写后再派。
            next_task: peek_next_task(&store, &project_key),
            // §14 团队专长清单:让经理知道有哪些员工、各擅长什么,派活时点明专长 → 派给对的人。
            team: team_specialties(&store),
            // §5 主动自治:CEO 的高层方向。非空 + 队列空时,经理主动 plan 推进(而非 noop 退出)。
            autonomous_goal: settings.as_ref().map(|s| s.autonomous_goal.clone()).unwrap_or_default(),
            // §5 目标进展:已为这个自治目标做过的子任务(结局)——让经理 plan 时知道推进到哪了,
            // 不重复、递进、理性收尾,而非每拍从零铺活。
            goal_progress: settings
                .as_ref()
                .map(|s| goal_progress(&store, &project_key, &s.autonomous_goal))
                .unwrap_or_default(),
        };

        // 2. 经理拍决策:**每拍现场按人事部配置选脑**(rule 免费/claude 真想,§14)——
        //    人事部改了「经理大脑」下一拍立即生效,没有缓存失效问题。decide 不持 orch 锁
        //    (ClaudeBrain 要等 claude 想几秒,期间不能堵 worker 回流);拿到 decision 后
        //    才持锁瞬间 apply(分配 node/fence/seq)。
        let brain = manager_brain(
            &app,
            &store,
            &settings.clone().unwrap_or_default(),
            pm.project.clone(),
            &project_key,
        );
        let decision = match brain.decide(&ctx).await {
            Ok(d) => d,
            // 应用层兜底(§21):经理决策失败绝不崩,降级为升级给人。
            Err(e) => Decision::Escalate { reason: format!("经理决策失败:{e:#}") },
        };
        let step = pm.orch.lock().await.apply(decision);

        // 3. 执行 effect(真起 worker / 交付 / 拦下 …)并把这一拍 emit 给前端工作台。
        //    返回**这拍是否真推进了局面** —— spawn 但队列空(claim None)算没推进,避免空转。
        let progressed =
            execute_effect(&app, &store, &pm, &step, &project_key, ctx.budget_remaining_usd).await;

        // 4. 终止/等待。只有**真正改变局面**的 effect(派了活/处置了节点)才立刻下一拍
        //    (可能继续派满名额)。Nothing/Escalate/Refresh、以及"想 spawn 但没活可派"都不算
        //    推进 —— 立即重 tick 必然同样结果,只会空转(真大脑还每拍烧钱,§5.8 升级风暴),所以一律等:
        //    - 无在途 → 局面不会自己变(没有 worker 会完成来翻盘),退出循环;
        //      enqueue / 调预算(resume_all)会重启。
        //    - 有在途 → 等某个 worker 完成唤醒(或超时兜底)再 tick。
        if !progressed {
            if pm.orch.lock().await.inflight_len() == 0 {
                // 竞态兜底:这拍的 ctx 是快照,期间可能有 worker 刚完工入了复核队
                // (先入队再释放名额,见 spawn_worker)——还有待裁的就立即再 tick 去裁,
                // 全裁完才退出。否则 break 会把完工 worker 晾在复核队里没人裁。
                if pm.reviews.lock().await.is_empty() {
                    // 纯目标自治启动:有目标 + 队列空 + 还没为目标做过任何事,但经理这拍 noop(没主动
                    // plan)。real claude 第一拍可能保守观望就 noop —— 别一 noop 就退出、让「给方向就自治」
                    // 看 claude 心情失灵。给几拍重试(prompt 已强令第一批别 noop),限次防真要收工时空转。
                    let pure_goal_start =
                        !ctx.autonomous_goal.trim().is_empty() && ctx.goal_progress.is_empty();
                    if pure_goal_start && idle_noops < MAX_GOAL_START_RETRY {
                        idle_noops += 1;
                        tokio::time::sleep(GOAL_START_RETRY_GAP).await;
                        continue;
                    }
                    break;
                }
                continue;
            }
            tokio::select! {
                _ = pm.wake.notified() => {}
                _ = tokio::time::sleep(TICK_IDLE_TIMEOUT) => {}
            }
        }
    }
}

/// 执行经理这一拍的 [`Effect`],并 emit 决策事件给前端工作台。返回这拍是否真落地了动作。
///
/// **P0 范围**:`Spawn` 真起 worker(经理的 spawn = 认领队列下一个真任务,队列=经理待办池);
/// `Continue/Deliver/Block/Escalate/Refresh` 先记录 + emit(真审计合并/真 resume 留后续刀)。
async fn execute_effect(
    app: &AppHandle,
    store: &Arc<Store>,
    pm: &Arc<ProjectManager>,
    step: &Step,
    project_key: &str,
    budget_remaining_usd: f64,
) -> bool {
    let mut node_id_out: Option<String> = None;
    let mut task_id_out: Option<String> = None;
    let mut task_prompt_out: Option<String> = None;
    // 这一拍是否真推进了局面(派成活 / 处置了节点)。Spawn 认领到任务才算;队列空(claim None)
    // 不算 —— 否则经理立即重 tick、claude 每拍空 spawn 烧钱,直到风暴熔断才停。
    let mut progressed = matches!(
        step.effect,
        Effect::Spawn { .. }
            | Effect::Plan { .. }
            | Effect::Continue { .. }
            | Effect::Deliver { .. }
            | Effect::Block { .. }
    );

    match &step.effect {
        // §9 烧钱红线硬闸:预算耗尽就**绝不派新活**(不只靠 claude prompt 自觉 —— claude 可能
        // 误判)。**实时重算**预算,不用 ctx.budget_remaining 那个快照 —— 快照在这拍开头算、而
        // claude 想了几秒,期间前一个 worker 才跑完记账,用快照会漏判一拍、烧超(实测 $0.3 cap
        // 烧到 $0.51)。回滚 admit 的名额、这拍不推进 → 经理停下等 CEO 加预算(resume_all 重启)。
        Effect::Spawn { node_id, fence, .. }
            if budget_remaining(store, store.get_settings().ok().as_ref()) <= 0.0 =>
        {
            pm.orch.lock().await.on_complete(node_id, *fence);
            progressed = false;
        }
        Effect::Spawn { node_id, fence, prompt } => {
            // 经理"派活"=从队列原子认领下一个待办,交给一个 worker 跑到底。
            match store.claim_next_queued(project_key, crate::now_ms()) {
                Ok(Some(task)) => {
                    // §5 A2 拆活:真经理给了非占位 prompt = 它结合记忆改写过的任务描述,
                    // worker 跑改写后的;占位(RuleBrain)→ 跑任务原文。
                    let work_prompt = if prompt == quiver_orchestrator::QUEUE_NEXT_PLACEHOLDER {
                        task.prompt.clone()
                    } else {
                        prompt.clone()
                    };
                    pm.node_tasks
                        .lock()
                        .await
                        .insert(node_id.clone(), task.id.clone());
                    node_id_out = Some(node_id.clone());
                    task_id_out = Some(task.id.clone());
                    task_prompt_out = Some(work_prompt.clone()); // 派的具体活,给决策流看
                    let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
                    spawn_worker(app, store, pm, node_id.clone(), *fence, task.id, work_prompt, RunMode::from_label(&task.mode), 0, project_key.to_string());
                }
                _ => {
                    // ctx 快照说有排队、认领时却空了(竞态/已被拆) → 回滚刚 admit 的名额,免得泄漏。
                    // 这拍没真派活 → 不算推进,别让经理立即重 tick 空转(真大脑每拍烧钱)。
                    pm.orch.lock().await.on_complete(node_id, *fence);
                    progressed = false;
                }
            }
        }
        // 拆活(§5 协作):经理把一个复杂目标拆成子任务,这里把子任务入队。后续经理逐拍 spawn
        // 它们(按并发/专长),实现"多 agent 分工并行"。入队后认领下一个待办的拆解原任务出队。
        Effect::Plan { subtasks } => {
            // 先把拆解的原任务(队首)认领掉,免得它又被当普通活派出去(它的角色已变成"被拆")。
            // 它的 prompt 就是父目标,标到每个子任务上 → 追溯室画"目标 → 子任务们"协作树。
            let parent = store
                .claim_next_queued(project_key, crate::now_ms())
                .ok()
                .flatten();
            // 父目标被拆了、没人直接干它 → 标「已拆解(planned)」终态,免得卡在 running 僵尸
            // (HUD 运行计数虚高、调度台假装它在跑)。由子任务接力完成。
            if let Some(p) = &parent {
                let _ = store.update_task_status(&p.id, "planned", crate::now_ms());
            }
            // 主动自治(队列空、无队首原任务)时:父目标退回 CEO 的自治目标 → 子任务仍挂在目标下,
            // 追溯室照样画「自治目标 → 子任务」树;mode 也退回设置的默认模式,而非写死 simulate ——
            // 否则 CEO 开了 real 自治、经理主动 plan 的子任务却全 simulate(假干、不真推进目标)。
            let settings = store.get_settings().ok();
            let parent_goal = parent
                .as_ref()
                .map(|t| t.prompt.clone())
                .or_else(|| {
                    settings
                        .as_ref()
                        .map(|s| s.autonomous_goal.clone())
                        .filter(|g| !g.trim().is_empty())
                });
            let mode = parent
                .as_ref()
                .map(|t| t.mode.clone())
                .or_else(|| settings.as_ref().map(|s| s.default_mode.clone()))
                .unwrap_or_else(|| "simulate".to_string());
            let now = crate::now_ms();
            for (i, sub) in subtasks.iter().enumerate() {
                // -{seq} 防同毫秒连拍 plan 撞 id(enqueue 是 ON CONFLICT DO UPDATE,撞了会覆盖丢活)。
                let id = format!("task-sub-{now}-{i}-{}", crate::next_id_seq());
                let _ = store.enqueue_task(&quiver_store::NewTask {
                    id: id.clone(),
                    project: project_key.to_string(),
                    prompt: sub.clone(),
                    mode: mode.clone(),
                    status: "queued".to_string(),
                    created_at: now,
                });
                if let Some(g) = &parent_goal {
                    let _ = store.set_task_parent_goal(&id, g, now);
                }
            }
            task_prompt_out = Some(format!("拆成 {} 个子任务分工", subtasks.len()));
            pm.wake.notify_one(); // 唤醒经理来 spawn 这些子任务
            let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
        }
        // 裁决(§5 复核):经理对完工 worker 拍交付/拦下 → 这单复核出队。
        Effect::Deliver { node_id } | Effect::Block { node_id, .. } => {
            node_id_out = Some(node_id.clone());
            let review = {
                let mut reviews = pm.reviews.lock().await;
                reviews
                    .iter()
                    .position(|r| r.node_id == *node_id)
                    .map(|pos| reviews.remove(pos))
            };
            if let Some(r) = review {
                let task = store.get_task(&r.task_id).ok().flatten();
                task_prompt_out = task.as_ref().map(|t| t.prompt.clone());
                task_id_out = Some(r.task_id.clone());
                // §5.7 真合并列车:**只有交付、且 real 留了分支**时,把成果合进 main(审计就位
                // = 沙箱三片已完工)。simulate 无分支 → 跳过(原样标记交付)。merge_and_reverify
                // 自带"合并后重验红 → reset main"的护栏,守住「main 永不坏」铁律。
                if matches!(step.effect, Effect::Deliver { .. }) {
                    if let Some(branch) = task.as_ref().and_then(|t| t.branch.clone()) {
                        deliver_merge(store, pm, &r.task_id, &branch).await;
                    }
                    // §6.7 里程碑:交付后记忆官(若显式开)从 episode 提炼事实。后台 best-effort。
                    crate::librarian::distill_after_delivery(app, store, project_key.to_string());
                } else if let Some(t) = task.as_ref() {
                    // §5 失败自愈(绝对自治):经理拦下的若是**验证失败**的活,公司自己再试一轮
                    // (带返工会话续跑),到上限才停手等 CEO —— 不用 CEO 每次手动打回。
                    if r.status.contains("fail") {
                        // 失败自愈是**自治行为**:autonomous 开才自己再试。手动 / 急停(autonomous 关)→
                        // 不擅自重跑,留 failed 等 CEO —— 尤其急停后(CEO 关了自治 + 杀了 worker),被杀的
                        // 活绝不能又被自愈拉起来烧钱(否则急停形同虚设)。
                        let autonomous = store.get_settings().ok().map(|s| s.autonomous).unwrap_or(false);
                        if autonomous && t.attempt < MAX_AUTO_RETRY {
                            auto_retry(store, pm, project_key, t).await;
                        } else if t.attempt >= MAX_AUTO_RETRY {
                            // §6 记忆驱动:自愈都救不动的失败 → 沉淀成一条高可信「教训」事实,经理
                            // 下一拍 brief 按 importance 优先召回,下次接类似活别同样硬上(越用越不重犯)。
                            record_failure_lesson(app, project_key, t, &r.status);
                        }
                    } else {
                        // 经理拦下一个**通过了验证**的活 = 觉得方向不对、否决它。别留它 verified —— 否则
                        // 晨报显示「通过·待接受」,CEO 可能误接受经理已否决的产出。标 needs_rebase 进
                        // 「卡住等你处理」,CEO 看到经理拦了、可带补充打回重做(§5 闭环),状态如实。
                        let _ = store.update_task_status(&t.id, "needs_rebase", crate::now_ms());
                    }
                }
                let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
            }
        }
        // §5 双向协作闭环:经理评审上一轮产出、给了指导 → 同一任务的 worker 带 --resume 续跑一轮,
        // 真把经理的回答送回 worker 手里(不是只记录给前端看)。
        Effect::Continue { node_id, fence, ref_node, prompt } => {
            // §5 双向协作:经理评审完上一轮产出、给了具体指导 → 让**同一任务**的 worker `--resume`
            // 带着指导再跑一轮(迭代改进),而不是闷头一锤子买卖。ref_node 是上一轮完工的节点 →
            // 从待复核队找回它是哪个 task,resume 它的 session_id(worker 记得自己上一轮干了啥)。
            // C1 §9 烧钱硬闸:Continue 也派 real worker(--resume),预算耗尽必须挡 —— 否则续跑烧过上限
            // (此前只 gate Spawn,Continue 是漏的)。
            let broke = budget_remaining(store, store.get_settings().ok().as_ref()) <= 0.0;
            let found = if broke {
                None
            } else {
                let reviews = pm.reviews.lock().await;
                reviews
                    .iter()
                    .find(|r| &r.node_id == ref_node)
                    .map(|r| (r.task_id.clone(), r.round))
            };
            // H1:读不到 task.mode 就**不续跑**(绝不把 real 任务降级 simulate 假干一轮) —— 预算/ref/
            // mode 任何一关过不了,都走下面 else 回滚名额、这拍不推进,而非猜一个 mode 硬上。
            let resume = match &found {
                // §5.8 续跑硬上限:这一轮续跑会让轮次到 MAX → 不再烧钱救,摘出复核队 + 标 needs_rebase
                // 上报 CEO(晨报「卡住等你处理」)。挡得住 claude 经理反复 continue 救不动一直救。
                Some((tid, prev_round)) if *prev_round + 1 >= MAX_CONTINUE_ROUNDS => {
                    pm.reviews.lock().await.retain(|r| &r.task_id != tid);
                    let _ = store.update_task_status(tid, "needs_rebase", crate::now_ms());
                    let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
                    None
                }
                Some((tid, prev_round)) => store
                    .get_task(tid)
                    .ok()
                    .flatten()
                    .map(|t| (tid.clone(), *prev_round, t.mode)),
                None => None,
            };
            if let Some((tid, prev_round, mode)) = resume {
                // 从待复核队摘掉(正在续跑,不再等裁);建新 node→task 映射;标 running。
                pm.reviews.lock().await.retain(|r| r.task_id != tid);
                pm.node_tasks
                    .lock()
                    .await
                    .insert(node_id.clone(), tid.clone());
                let _ = store.update_task_status(&tid, "running", crate::now_ms());
                node_id_out = Some(node_id.clone());
                task_id_out = Some(tid.clone());
                task_prompt_out = Some(format!("续跑改进:{prompt}"));
                let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
                // spawn_worker 用经理指导当 prompt;worker 内部凭 task.session_id 自动 --resume,
                // 于是它带着"上一轮的记忆 + 经理这轮的指导"改进。
                spawn_worker(
                    app,
                    store,
                    pm,
                    node_id.clone(),
                    *fence,
                    tid,
                    prompt.clone(),
                    RunMode::from_label(&mode),
                    prev_round + 1, // §5 双向协作:续跑轮次 +1
                    project_key.to_string(),
                );
            } else {
                // ref 找不到(竞态/已被别的拍裁掉) → 回滚刚 admit 的名额,这拍不推进。
                pm.orch.lock().await.on_complete(node_id, *fence);
                progressed = false;
            }
        }
        Effect::Escalate { reason } => {
            // §12 经理升级给 CEO。队首通常卡着一个 escalate 的诱因(缺只有 CEO 有的信息/超出自治
            // 能力)→ claim 它、标 `escalated` 移出队列,免得经理每拍重看同一个、反复 escalate
            // (熔断才停);升级原因存进 question 给 CEO 看。CEO 在晨报看到、补信息后重派。无队首
            // (纯局面级升级)→ 只记决策、不动任务。
            if let Ok(Some(task)) = store.claim_next_queued(project_key, crate::now_ms()) {
                let _ = store.update_task_status(&task.id, "escalated", crate::now_ms());
                let _ = store.set_task_question(&task.id, reason, crate::now_ms());
                task_id_out = Some(task.id.clone());
                let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
                progressed = true; // 挂起了一个任务,局面变了 → 继续看队列下一个
            }
        }
        Effect::Refresh | Effect::Nothing => {}
    }

    // §6 决策留痕回记忆(C1):真落地的组织动作(派活/交付/拦下/升级)写成 episode ——
    // 时间线可见"经理做了哪些决策",且简报的近期 episode 自然带上 → 经理下一拍
    // "记得自己之前的裁决"。Noop/被拒的不写(刷屏无信息量)。
    if !matches!(step.effect, Effect::Nothing) {
        record_decision_episode(
            app,
            project_key,
            &step.decision,
            node_id_out.as_deref(),
            task_id_out.as_deref(),
            task_prompt_out.as_deref(),
        );
    }

    emit_decision(
        app, store, project_key, step, node_id_out, task_id_out, task_prompt_out,
        budget_remaining_usd, pm,
    )
    .await;

    progressed
}

/// 经理**思考流**推给前端的形(camelCase):经理用 claude 决策时,claude 每吐一段思考就发一条,
/// 让 CEO 点开经理实时看到它在想什么。project 用于前端按当前项目过滤。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagerThinkingEvent {
    project: String,
    text: String,
}

/// 给某 project 的经理控制循环选大脑(DESIGN §5/§21/§14)。
///
/// **唯一的选脑依据是人事部「经理」角色的 `brain` 字段**(用户在人事部显式改,seed 为
/// 免费 `rule`):`claude` → 真 claude 经理([`ClaudeBrain`](crate::claude_brain),用该角色
/// 配置的 model,走 headless 额度);其余/读不到/二进制解析失败 → 免费 `RuleBrain`。
///
/// 教训(2026-06-10 实测,绝不回退):曾按 `default_mode=="real"` 自动选 ClaudeBrain,结果
/// simulate 任务被真 claude 经理连环想、烧额度毫无感知。`default_mode` 是"任务跑什么
/// 模式",不是"经理用什么脑"——烧钱的大脑只能由这个显式旋钮打开(蓝图 §7),
/// 不搭任何其他设置的便车。
fn manager_brain(
    app: &AppHandle,
    store: &Arc<Store>,
    settings: &Settings,
    cwd: PathBuf,
    project_key: &str,
) -> Arc<dyn quiver_orchestrator::ManagerBrain> {
    let role = store.get_role("manager").ok().flatten();
    if let Some(role) = role.filter(|r| r.brain == "claude") {
        // claude 经理走 ClaudeBrain 路径(spawn 进程→parse 决策 JSON→执行)。**simulate 模式
        // 用 fake-claude bin 免费跑这条路径**(fake-claude 识别决策 prompt、输出桩决策):让用户
        // 不烧钱就能预演 claude 经理的完整工作流、也验证开真 claude 前管道无断点。real 模式才
        // 用真 claude bin(真智能决策、烧 headless 额度)。
        let mode = if settings.default_mode == "real" {
            RunMode::Real
        } else {
            RunMode::Simulate
        };
        if let Ok(bin) = resolve_agent_bin(settings, mode) {
            // 思考流回调:claude 每吐一段思考就 emit 到前端(可见性)——经理不再是黑箱。
            let app2 = app.clone();
            let proj = project_key.to_string();
            let sink = std::sync::Arc::new(move |text: &str| {
                let _ = app2.emit(
                    MANAGER_THINKING_CHANNEL,
                    ManagerThinkingEvent { project: proj.clone(), text: text.to_string() },
                );
            });
            // CEO 在人事部给经理写的工作准则/性格注入决策(§14 丰富配置)。
            return Arc::new(
                crate::claude_brain::ClaudeBrain::new(bin, cwd, role.model)
                    .with_system_prompt(role.system_prompt)
                    .with_thinking_sink(sink),
            );
        }
    }
    Arc::new(quiver_orchestrator::RuleBrain)
}
