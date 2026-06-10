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

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, Notify};

use quiver_core::git::GitGuard;
use quiver_core::merge::{merge_and_reverify, MergeDecision, MergeLock};
use quiver_core::verify::VerifyCommand;
use quiver_orchestrator::{
    Decision, Effect, ManagerContext, Orchestrator, PendingReview, Step,
};
use quiver_store::{Settings, Store, TaskRecord};

use crate::run::{run_one_task, RunMode};
use crate::scheduler::TASK_EVENT_CHANNEL;

/// 前端「经理工作台」订阅的决策流 channel:经理每拍 emit 一条
/// [`ManagerDecisionEvent`](决策 + 理由 + 调动的真任务),让人**亲眼看到经理在自治**。
pub const MANAGER_DECISION_CHANNEL: &str = "manager-decision";

/// 没设夜预算时视为充裕(不因预算挡经理),与 `manager_preview` 同口径。
const BUDGET_UNCAPPED: f64 = 1_000_000.0;
/// 经理满载/在途未空时一拍的兜底等待。唤醒主要靠 worker 完工的 `notify`,这只是防丢
/// 唤醒的保险 —— 放宽到 5s:别让超时拍刷屏决策流,真大脑下每一拍都是决策调用(钱)。
const TICK_IDLE_TIMEOUT: Duration = Duration::from_secs(5);
/// 一次经理循环生命周期的决策拍数硬上限(DESIGN §5.8「保证停下来」的简版决策税):
/// 不管 AI 怎么绕圈,拍数见底强制停 —— 杜绝任何形式的空转/升级风暴把额度烧穿。
/// 队列重新有活时 enqueue 会重启新循环,所以这只挡风暴、不挡正常工作。
const MAX_TICKS_PER_LOOP: u64 = 200;
const DAY_MS: i64 = 86_400_000;

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
        };

        // 2. 经理拍决策:**每拍现场按人事部配置选脑**(rule 免费/claude 真想,§14)——
        //    人事部改了「经理大脑」下一拍立即生效,没有缓存失效问题。decide 不持 orch 锁
        //    (ClaudeBrain 要等 claude 想几秒,期间不能堵 worker 回流);拿到 decision 后
        //    才持锁瞬间 apply(分配 node/fence/seq)。
        let brain = crate::manager_brain(
            &store,
            &settings.clone().unwrap_or_default(),
            pm.project.clone(),
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
            let parent_goal = parent.as_ref().map(|t| t.prompt.clone());
            let mode = parent
                .as_ref()
                .map(|t| t.mode.clone())
                .unwrap_or_else(|| "simulate".to_string());
            let now = crate::now_ms();
            for (i, sub) in subtasks.iter().enumerate() {
                let id = format!("task-sub-{i}-{now}");
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
                    if r.status.contains("fail") && t.attempt < MAX_AUTO_RETRY {
                        auto_retry(store, pm, project_key, t).await;
                    }
                }
                let _ = app.emit(TASK_EVENT_CHANNEL, project_key);
            }
        }
        // Continue P0 先"记录 + 让前端看见",真 resume 留后续刀(见 plan「不做」)。
        Effect::Continue { node_id, fence, ref_node, prompt } => {
            // §5 双向协作:经理评审完上一轮产出、给了具体指导 → 让**同一任务**的 worker `--resume`
            // 带着指导再跑一轮(迭代改进),而不是闷头一锤子买卖。ref_node 是上一轮完工的节点 →
            // 从待复核队找回它是哪个 task,resume 它的 session_id(worker 记得自己上一轮干了啥)。
            let found = {
                let reviews = pm.reviews.lock().await;
                reviews
                    .iter()
                    .find(|r| &r.node_id == ref_node)
                    .map(|r| (r.task_id.clone(), r.round))
            };
            if let Some((tid, prev_round)) = found {
                // 从待复核队摘掉(正在续跑,不再等裁);建新 node→task 映射;标 running。
                pm.reviews.lock().await.retain(|r| r.task_id != tid);
                pm.node_tasks
                    .lock()
                    .await
                    .insert(node_id.clone(), tid.clone());
                let mode = store
                    .get_task(&tid)
                    .ok()
                    .flatten()
                    .map(|t| t.mode)
                    .unwrap_or_else(|| "simulate".to_string());
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
        Effect::Escalate { .. } | Effect::Refresh | Effect::Nothing => {}
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

/// 把一拍真落地的决策写成记忆 episode(§6 C1)。best-effort:记忆是加性依赖,写失败
/// 绝不影响编排。summary 全中文自含语义(简报/时间线直接可读)。
fn record_decision_episode(
    app: &AppHandle,
    project_key: &str,
    decision: &Decision,
    node_id: Option<&str>,
    task_id: Option<&str>,
    task_prompt: Option<&str>,
) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let Some(mem) = state.memory.get() else {
        return;
    };
    let clip = |s: &str| {
        let t: String = s.chars().take(40).collect();
        if t.len() < s.len() { format!("{t}…") } else { t }
    };
    let task = task_prompt.map(|p| format!("「{}」", clip(p))).unwrap_or_default();
    let summary = match decision {
        Decision::Spawn { reason, .. } => format!(
            "经理·派活{task}{}",
            reason.as_deref().map(|r| format!("({r})")).unwrap_or_default()
        ),
        Decision::Plan { subtasks, reason } => format!(
            "经理·拆活成 {} 个子任务分工{}",
            subtasks.len(),
            reason.as_deref().map(|r| format!("({r})")).unwrap_or_default()
        ),
        Decision::Deliver { .. } => format!("经理·交付{task}"),
        Decision::Block { reason, .. } => format!("经理·拦下{task}({reason})"),
        Decision::Escalate { reason } => format!("经理·升级给人({reason})"),
        Decision::Continue { .. } => format!("经理·续跑{task}"),
        Decision::RefreshMemory | Decision::Noop => return, // 无组织动作,不留痕
    };
    let _ = mem.record_episode(&quiver_memory::NewEpisode {
        project: project_key.to_string(),
        node_id: node_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        summary: Some(summary),
        created_at: crate::now_ms(),
        ..Default::default()
    });
}

/// §5 协作汇总:一个子任务完成后调。若它属于某协作目标(parent_goal),且该目标下**所有**
/// 子任务都到完成态(verified/merged/done) → 把那个停在 `planned` 的父目标标 `done`,表示
/// "公司协作把这个复杂目标搞定了"。非子任务 / 还有兄弟没完成 → 不动。best-effort。
fn maybe_complete_parent_goal(store: &Arc<Store>, project: &str, task_id: &str) {
    let Some(goal) = store
        .get_task(task_id)
        .ok()
        .flatten()
        .and_then(|t| t.parent_goal)
    else {
        return; // 不是子任务
    };
    let Ok(all) = store.list_tasks(Some(project), None) else {
        return;
    };
    let siblings: Vec<_> = all
        .iter()
        .filter(|t| t.parent_goal.as_deref() == Some(goal.as_str()))
        .collect();
    // 终态 = 不再进行中(成功或失败都算"尘埃落定");成功 = 验收过/合并/完成。
    let terminal = |s: &str| {
        matches!(
            s,
            "verified" | "merged" | "done" | "failed" | "verify_failed" | "needs_rebase" | "cancelled"
        )
    };
    let ok = |s: &str| matches!(s, "verified" | "merged" | "done");
    if siblings.is_empty() || !siblings.iter().all(|t| terminal(&t.status)) {
        return; // 还有子任务在进行,先不收尾
    }
    // 子任务都尘埃落定 → 父目标终结,免得卡 planned:全成功→done(协作完成);有失败→
    // needs_rebase(协作部分失败,需要你看)。
    let new_status = if siblings.iter().all(|t| ok(&t.status)) {
        "done"
    } else {
        "needs_rebase"
    };
    if let Some(parent) = all.iter().find(|t| t.prompt == goal && t.status == "planned") {
        let _ = store.update_task_status(&parent.id, new_status, crate::now_ms());
    }
}

/// kill 一个上个会话遗留的孤儿 worker 进程。**先验证该 PID 当前确是 claude 进程**(`ps`
/// 命令名含 claude)再杀 —— PID 会被系统复用,崩溃后那个号可能早归了别的无关进程,绝不能盲杀。
/// best-effort:验证不过 / 杀不掉都静默跳过(顶多孤儿多跑一会到自然结束)。
fn kill_orphan_worker(pid: i64) {
    if pid <= 0 {
        return;
    }
    let Ok(out) = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    else {
        return;
    };
    let cmd = String::from_utf8_lossy(&out.stdout);
    // 进程已不在(空输出)→ 无需杀;在、且确是 claude → kill。
    if cmd.to_lowercase().contains("claude") {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// 起一个 worker 异步跑 `run_one_task`,完成后回流:释放在途名额(`on_complete` 凭栅栏对账)、
/// 清 node↔task 映射、刷新看板、`notify` 唤醒经理再 tick(§5.6)。
#[allow(clippy::too_many_arguments)]
fn spawn_worker(
    app: &AppHandle,
    store: &Arc<Store>,
    pm: &Arc<ProjectManager>,
    node_id: String,
    fence: u64,
    task_id: String,
    prompt: String,
    mode: RunMode,
    round: u32,
    project_key: String,
) {
    let app = app.clone();
    let store = store.clone();
    let pm = pm.clone();
    let guard = pm.guard.clone();
    tokio::spawn(async move {
        run_one_task(&app, &store, &guard, task_id.clone(), prompt, mode).await;
        // 完工入待复核队(§5 裁决):带终态进队,**先入队再释放名额** —— 这样经理任何时刻
        // 看到 inflight==0 时复核队列必然已就绪,不会漏裁。读不到终态按 failed 保守裁。
        let status = store
            .get_task(&task_id)
            .ok()
            .flatten()
            .map(|t| t.status)
            .unwrap_or_else(|| "failed".to_string());
        // §5 协作汇总:子任务完成后,若它所属协作目标的所有子任务都完成了 → 标父目标「协作完成」。
        maybe_complete_parent_goal(&store, &project_key, &task_id);
        pm.reviews.lock().await.push(PendingReview {
            node_id: node_id.clone(),
            task_id,
            status,
            round, // §5 双向协作:这是第几轮(首跑 0,经理每 continue 一次 +1)
        });
        // 回流(§5.5 栅栏对账):栅栏匹配才释放名额,挡掉过期/重复。
        pm.orch.lock().await.on_complete(&node_id, fence);
        pm.node_tasks.lock().await.remove(&node_id);
        let _ = app.emit(TASK_EVENT_CHANNEL, &project_key);
        pm.wake.notify_one();
    });
}

/// §5.7 合并列车:把交付任务的成果分支合进 `main` 并合并后重验,按结果改任务状态。
/// 失败自愈的尝试上限(§5):attempt 到此就停手、保持 block 等 CEO。2 = 首跑 + 最多自动重试 1 次,
/// 防"红任务无限重试"刷屏 / 烧钱。
const MAX_AUTO_RETRY: i64 = 2;

/// §5 失败自愈:把一个验证失败的任务作为**新任务**重新入队(attempt+1、带返工会话续跑),
/// 经理循环下一拍会把它派出去。best-effort:任何一步失败只是不重试,绝不让编排崩。
/// new_id 用 `task-retry-{attempt}-{now}` 避免与原任务及彼此撞键。
async fn auto_retry(store: &Arc<Store>, pm: &Arc<ProjectManager>, project: &str, old: &TaskRecord) {
    let now = crate::now_ms();
    let next_attempt = old.attempt + 1;
    let new_id = format!("task-retry-{next_attempt}-{now}");
    let prompt = format!(
        "(自动重试 第 {} 次)上次运行的成果未通过验收,请修复问题后重新交付。原任务:{}",
        next_attempt - 1,
        old.prompt
    );
    if store
        .enqueue_task(&quiver_store::NewTask {
            id: new_id.clone(),
            project: project.to_string(),
            prompt,
            mode: old.mode.clone(),
            status: "queued".to_string(),
            created_at: now,
        })
        .is_err()
    {
        return;
    }
    let _ = store.set_task_attempt(&new_id, next_attempt, now);
    // 续会话返工:worker 记得上次改过什么(real 有意义;simulate 无 session 也无妨)。
    if let Some(sid) = store.task_session_id(&old.id).ok().flatten() {
        let _ = store.set_task_session_id(&new_id, &sid, now);
    }
    // 唤醒经理循环来 spawn 这个新排队任务(循环 idle 等待时靠这个戳醒)。
    pm.wake.notify_one();
}

/// 全局串行(merge_lock);merge_and_reverify 自带护栏(冲突→needs_rebase、重验红→reset
/// main),所以这里只需翻译结果:`merged`(进了 main)/ `needs_rebase`(冲突或重验红,留人工)。
/// best-effort:合并出错只记日志、不改状态(任务仍在分支上,可人工处理),绝不让编排崩。
async fn deliver_merge(store: &Arc<Store>, pm: &Arc<ProjectManager>, task_id: &str, branch: &str) {
    let verify_cmd = store
        .get_settings()
        .ok()
        .map(|s| s.verify_command)
        .filter(|c| !c.trim().is_empty())
        .map(VerifyCommand::shell)
        .unwrap_or_else(|| VerifyCommand::shell("exit 0"));
    let msg = format!("quiver: merge {branch} (经理交付)");
    match merge_and_reverify(&pm.merge_lock, &pm.guard, branch, "main", &msg, &verify_cmd).await {
        Ok(report) if report.decision == MergeDecision::Merged => {
            let _ = store.update_task_status(task_id, "merged", crate::now_ms());
        }
        Ok(_) => {
            // 冲突 / 合并后重验红 —— main 已被护栏还原,任务留分支等人工(绝不自动解决冲突)。
            let _ = store.update_task_status(task_id, "needs_rebase", crate::now_ms());
        }
        Err(_) => { /* 合并管线本身出错:不改状态,留分支,best-effort */ }
    }
}

/// 一拍决策推给前端工作台的形(camelCase 直喂 React)。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagerDecisionEvent {
    project: String,
    /// 决策序号(§5 去重钥匙,也是工作台时间线的单调序)。
    seq: u64,
    /// 决策动作:spawn/continue/deliver/block/escalate/refresh_memory/noop。
    action: String,
    /// 经理给的理由(有则)。
    reason: Option<String>,
    /// 这拍作用/调动的抽象节点(有则)。
    node_id: Option<String>,
    /// 抽象节点翻译成的真任务(有则)——让人看到"经理因此调动了哪个活"。
    task_id: Option<String>,
    /// effect 是否真落地(非 Nothing:被满载/未知节点等校验拒掉则 false)。
    executed: bool,
    /// spawn 时派的**具体任务文本** —— 让"派活"不再抽象,看得到派的是什么活。
    task_prompt: Option<String>,
    /// 这拍后的实时局面 + 决策依据(预算/并发上限),让人看出经理基于什么判断。
    inflight: usize,
    queued: usize,
    max_inflight: usize,
    budget_remaining_usd: f64,
    ts_ms: i64,
}

/// 把一拍 [`Step`] 拼成 [`ManagerDecisionEvent`] 并 emit 到 [`MANAGER_DECISION_CHANNEL`]。
#[allow(clippy::too_many_arguments)]
async fn emit_decision(
    app: &AppHandle,
    store: &Arc<Store>,
    project_key: &str,
    step: &Step,
    node_id: Option<String>,
    task_id: Option<String>,
    task_prompt: Option<String>,
    budget_remaining_usd: f64,
    pm: &Arc<ProjectManager>,
) {
    let (action, reason) = describe(&step.decision);
    let ev = ManagerDecisionEvent {
        project: project_key.to_string(),
        seq: step.seq,
        action,
        reason,
        node_id,
        task_id,
        executed: !matches!(step.effect, Effect::Nothing),
        task_prompt,
        inflight: pm.orch.lock().await.inflight_len(),
        queued: count_queued(store, project_key),
        max_inflight: pm.max_inflight,
        budget_remaining_usd,
        ts_ms: crate::now_ms(),
    };
    // §10/§20 决策落库(best-effort):决策流持久化,重启不丢、工作台可回看历史。
    let _ = store.record_decision(&quiver_store::DecisionRecord {
        project: ev.project.clone(),
        seq: ev.seq as i64,
        action: ev.action.clone(),
        reason: ev.reason.clone(),
        node_id: ev.node_id.clone(),
        task_id: ev.task_id.clone(),
        task_prompt: ev.task_prompt.clone(),
        executed: ev.executed,
        inflight: ev.inflight as i64,
        queued: ev.queued as i64,
        max_inflight: ev.max_inflight as i64,
        budget_remaining_usd: ev.budget_remaining_usd,
        ts_ms: ev.ts_ms,
    });
    let _ = app.emit(MANAGER_DECISION_CHANNEL, &ev);
}

/// 决策 → (action 标签, 理由)。理由对没有 reason 字段的动作给 `None`。
fn describe(decision: &Decision) -> (String, Option<String>) {
    match decision {
        Decision::Spawn { reason, .. } => ("spawn".into(), reason.clone()),
        Decision::Plan { subtasks, reason } => (
            "plan".into(),
            Some(reason.clone().unwrap_or_else(|| format!("拆成 {} 个子任务分工", subtasks.len()))),
        ),
        Decision::Continue { .. } => ("continue".into(), None),
        Decision::Deliver { .. } => ("deliver".into(), None),
        Decision::Block { reason, .. } => ("block".into(), Some(reason.clone())),
        Decision::Escalate { reason } => ("escalate".into(), Some(reason.clone())),
        Decision::RefreshMemory => ("refresh_memory".into(), None),
        Decision::Noop => ("noop".into(), None),
    }
}

/// 组装项目记忆简报文本(§6):前 8 条当前事实 + 近 6 条 episode。记忆是加性依赖 ——
/// MemoryStore 未装好/读错都返回空串,绝不挡经理决策(操作真值只来自 orchestration store)。
fn memory_brief(app: &AppHandle, project_key: &str) -> String {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return String::new();
    };
    let Some(mem) = state.memory.get() else {
        return String::new();
    };
    mem.brief(project_key, 8, 6)
        .map(|b| b.to_text())
        .unwrap_or_default()
}

/// 某 project 当前排队任务数。
fn count_queued(store: &Arc<Store>, project_key: &str) -> usize {
    store
        .list_tasks(Some(project_key), Some("queued"))
        .map(|v| v.len())
        .unwrap_or(0)
}

/// 团队成员的专长清单(§14):列出所有员工角色 + 专长,供经理派活时按专长指派。
fn team_specialties(store: &Arc<Store>) -> Vec<String> {
    store
        .list_roles()
        .map(|roles| {
            roles
                .into_iter()
                .filter(|r| r.kind == "worker")
                .map(|r| {
                    let sp = if r.specialty.trim().is_empty() { "通用" } else { r.specialty.as_str() };
                    format!("{} · {sp}", r.name)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 队首任务原文(§5 A2):`list_tasks` 与 `claim_next_queued` 同序(position→时间),
/// 所以这里看到的第一条就是 Spawn 时会被认领的那条。
fn peek_next_task(store: &Arc<Store>, project_key: &str) -> Option<String> {
    store
        .list_tasks(Some(project_key), Some("queued"))
        .ok()
        .and_then(|v| v.into_iter().next())
        .map(|t| t.prompt)
}

/// 经理可用预算剩余(美元):有夜预算 → 上限 − 近 24h 花费(夹 0);没设 → 视为充裕。
/// 与 `manager_preview` / scheduler 的 `over_budget` 同口径(§10)。
fn budget_remaining(store: &Arc<Store>, settings: Option<&Settings>) -> f64 {
    let Some(settings) = settings else {
        return BUDGET_UNCAPPED;
    };
    match settings.nightly_budget_usd {
        Some(cap) if cap > 0.0 => {
            let spent = store.cost_since(crate::now_ms() - DAY_MS).unwrap_or(0.0);
            (cap - spent).max(0.0)
        }
        _ => BUDGET_UNCAPPED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_orchestrator::Decision;

    // 执行层(execute_effect/manager_loop)紧耦合 tauri AppHandle + run_one_task,端到端行为靠
    // 桥接器 simulate 冒烟验证(plan 验证总览);Decision→Effect→on_complete 的状态机已在
    // quiver-orchestrator 的 28 个单测里覆盖。这里测 manager.rs 自己的纯逻辑:决策→前端形的
    // 翻译,和预算口径。

    #[test]
    fn describe_maps_action_and_reason() {
        // 带 reason 的动作透传理由;没有 reason 字段的给 None。
        assert_eq!(describe(&Decision::Noop), ("noop".to_string(), None));
        let (action, reason) =
            describe(&Decision::Spawn { prompt: "x".into(), reason: Some("有排队".into()) });
        assert_eq!(action, "spawn");
        assert_eq!(reason, Some("有排队".to_string()));
        let (action, reason) =
            describe(&Decision::Block { node_id: "n".into(), reason: "测试没过".into() });
        assert_eq!(action, "block");
        assert_eq!(reason, Some("测试没过".to_string()));
        assert_eq!(describe(&Decision::Escalate { reason: "拿不准".into() }).0, "escalate");
        assert_eq!(describe(&Decision::Deliver { node_id: "n".into() }), ("deliver".to_string(), None));
        assert_eq!(describe(&Decision::RefreshMemory).0, "refresh_memory");
    }

    #[test]
    fn budget_remaining_uncapped_without_nightly_budget() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        // 默认无夜预算 → 视为充裕(不因预算挡经理);settings 缺失同理。
        let settings = store.get_settings().unwrap();
        assert_eq!(budget_remaining(&store, Some(&settings)), BUDGET_UNCAPPED);
        assert_eq!(budget_remaining(&store, None), BUDGET_UNCAPPED);
    }

    #[test]
    fn budget_remaining_subtracts_spend_under_nightly_cap() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let mut settings = store.get_settings().unwrap();
        settings.nightly_budget_usd = Some(10.0);
        // 空库无花费 → 全额剩余;cap 为 0/负当未设(充裕)。
        assert_eq!(budget_remaining(&store, Some(&settings)), 10.0);
        settings.nightly_budget_usd = Some(0.0);
        assert_eq!(budget_remaining(&store, Some(&settings)), BUDGET_UNCAPPED);
    }
}
