//! Quiver Tauri app crate (DESIGN §3 three-layer architecture).
//!
//! This is the Rust "core" half of the Tauri app. It owns the Tauri runtime,
//! the IPC surface the React UI talks to, the picked-project app state,
//! and the durable store. The actual supervisor logic — worktrees, the
//! verify-gate, merge — lives in `quiver-core`; the per-task run plumbing
//! (binary resolution, live streaming, persistence) lives in [`run`]; and the
//! Phase-C concurrent queue scheduler lives in [`scheduler`].
//!
//! Commands:
//! - `pick_project` / `select_recent_project` — choose + remember a git repo.
//! - `get_initial_state` / `get_settings` / `update_settings` — durable §11 state.
//! - `run_task_cmd` — run ONE task immediately (legacy single-shot path).
//! - `enqueue_task_cmd` — pin a task to the bulletin board (queued) and kick the
//!   scheduler, which runs up to `maxWorkers` tasks CONCURRENTLY (Phase C).
//! - `list_tasks` / `reorder_task` / `cancel_task_cmd` — board management.
//! - `get_task_events` — the §11 ordered event log for Logbook replay (Phase D).

mod claude_brain;
mod environment;
mod librarian;
mod manager;
mod run;
mod scheduler;

// Dev-only inspection bridge for `tauri-agent-tools`. Compiled out of release
// builds entirely so neither the module nor its localhost HTTP server ship.
#[cfg(debug_assertions)]
mod dev_bridge;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use quiver_memory::{Brief, EpisodeRecord, MemoryStore, NewFact};
use quiver_store::{
    InitialState, MetricSample, NewTask, Settings, SettingsPatch, Store, StoredEvent, TaskRecord,
};

use crate::environment::check_environment;
use crate::run::{run_one_task, RunMode};
use crate::scheduler::{Scheduler, TASK_EVENT_CHANNEL};

/// Per-app state: the user-picked project (a git repo) all runs operate on, the
/// durable SQLite [`Store`] (shared as an `Arc` so spawned queue workers can hold
/// it), and the concurrent queue [`Scheduler`] (DESIGN §11, v1.0 module 5).
#[derive(Default)]
struct AppState {
    project_path: Mutex<Option<PathBuf>>,
    store: std::sync::OnceLock<Arc<Store>>,
    /// Durable agent memory (episodes + facts, DESIGN §6). Shared as an `Arc` so
    /// spawned queue workers can record episodes on completion. Installed in `setup`.
    memory: std::sync::OnceLock<Arc<MemoryStore>>,
    scheduler: Scheduler,
    /// 自治经理编排运行时(DESIGN §5)。`settings.autonomous` 为真时,enqueue 走它而非
    /// `scheduler`:经理控制循环基于决策驱动调动(spawn/deliver/…),而不是无脑流水线。
    manager: manager::ManagerLoop,
    /// task_id → child PID of currently-running tasks. Populated when a task's
    /// agent spawns, removed when it ends. Read by `cancel_task_cmd` to stop a
    /// running task (kill the PID → stdout EOF → normal cleanup path).
    running_pids: Mutex<std::collections::HashMap<String, u32>>,
}

impl AppState {
    /// The durable store, installed in `setup`.
    fn store(&self) -> Result<Arc<Store>, String> {
        self.store
            .get()
            .cloned()
            .ok_or_else(|| "持久化存储尚未初始化".to_string())
    }

    /// The durable agent memory, installed in `setup`.
    fn memory(&self) -> Result<Arc<MemoryStore>, String> {
        self.memory
            .get()
            .cloned()
            .ok_or_else(|| "记忆存储尚未初始化".to_string())
    }
}

/// Open a folder dialog, validate the chosen directory is a git repo, store it
/// in app state, and return its absolute path. `Ok(None)` if the user cancels.
#[tauri::command]
async fn pick_project(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let chosen = app.dialog().file().blocking_pick_folder();
    let Some(folder) = chosen else {
        return Ok(None);
    };
    let path: PathBuf = folder
        .into_path()
        .map_err(|e| format!("could not resolve the chosen folder: {e}"))?;
    select_validated_project(&state, path).map(Some)
}

/// Re-select a project the user picked before (from the recent list) WITHOUT the
/// folder dialog.
#[tauri::command]
fn select_recent_project(state: State<'_, AppState>, path: String) -> Result<String, String> {
    select_validated_project(&state, PathBuf::from(path))
}

/// Load the durable startup bundle (DESIGN §11): last project + recents + history.
#[tauri::command]
fn get_initial_state(state: State<'_, AppState>) -> Result<InitialState, String> {
    let store = state.store()?;
    let initial = store.initial_state().map_err(|e| format!("{e:#}"))?;
    if let Some(last) = &initial.last_project {
        *state.project_path.lock().expect("project_path lock") = Some(PathBuf::from(last));
    }
    Ok(initial)
}

/// Forget a project from the recent list (durable). Does not touch the currently
/// picked project — only the MRU entry is dropped.
#[tauri::command]
fn remove_recent_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let store = state.store()?;
    store
        .remove_recent_project(&path)
        .map_err(|e| format!("{e:#}"))
}

/// Set (or clear, with `None`) a recent project's display alias.
#[tauri::command]
fn set_project_alias(
    state: State<'_, AppState>,
    path: String,
    alias: Option<String>,
) -> Result<(), String> {
    let store = state.store()?;
    store
        .set_recent_project_alias(&path, alias.as_deref())
        .map_err(|e| format!("{e:#}"))
}

/// Read the typed app settings (DESIGN §11, v1.0 module 1).
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let store = state.store()?;
    store.get_settings().map_err(|e| format!("{e:#}"))
}

/// Apply a partial settings update and return the resulting full [`Settings`].
///
/// After persisting, re-kick the scheduler: a settings change may raise the §10
/// budget cap, which should un-pause any queue the gate stopped — `resume_all`
/// re-checks the gate for each known project (no-op for queues already running).
#[tauri::command]
async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let store = state.store()?;
    let settings = store.update_settings(&patch).map_err(|e| format!("{e:#}"))?;
    // 两条调度路径都重踢:旧 scheduler 总是;自治开着时经理循环也复活(调高/清预算后继续派活)。
    // 经理循环复用各 project 首次注册时存的大脑,故这里无需再传。
    state.scheduler.resume_all(app.clone(), store.clone()).await;
    if settings.autonomous {
        state.manager.resume_all(app, store.clone()).await;
    }
    Ok(settings)
}

/// 当前项目最近的经理决策(decision_log,最新在前;§10 复盘)。工作台打开时回填决策流
/// 历史 —— live 事件之前发生过什么,重启也不丢。
#[tauri::command]
fn get_decisions(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<quiver_store::DecisionRecord>, String> {
    let project = current_project(&state)?.display().to_string();
    let store = state.store()?;
    store
        .decisions_for_project(&project, limit.unwrap_or(40).clamp(1, 200))
        .map_err(|e| format!("{e:#}"))
}

/// 人事部:全部角色配置(经理在前)。前端人事部 UI 的数据源(DESIGN §14)。
#[tauri::command]
fn list_roles(state: State<'_, AppState>) -> Result<Vec<quiver_store::AgentRole>, String> {
    let store = state.store()?;
    store.list_roles().map_err(|e| format!("{e:#}"))
}

/// 人事部:雇一个新员工(§14 角色生命周期)。生成唯一 id,默认免费规则配置,返回新角色行。
#[tauri::command]
fn hire_worker(state: State<'_, AppState>) -> Result<quiver_store::AgentRole, String> {
    let store = state.store()?;
    let now = now_ms();
    let role = quiver_store::AgentRole {
        id: format!("worker-{now}"),
        name: "新员工".to_string(),
        kind: "worker".to_string(),
        brain: "rule".to_string(),
        model: "sonnet".to_string(),
        system_prompt: String::new(),
        budget_usd: None,
        max_turns: None,
        specialty: "通用".to_string(),
        version: 1,
        updated_at: now,
    };
    store.create_role(&role).map_err(|e| format!("{e:#}"))?;
    Ok(role)
}

/// 人事部:裁掉一个员工(§14)。只能裁 kind=worker(经理/图书管理员是单例,删不得)。
#[tauri::command]
fn fire_worker(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let store = state.store()?;
    if !store.delete_role(&id).map_err(|e| format!("{e:#}"))? {
        return Err("只能裁员工(经理/图书管理员是单例岗位,删不得)".to_string());
    }
    Ok(())
}

/// 人事部:增量改一个角色(version+1),返回更新后的完整配置。改「经理」的 `brain` 字段
/// 即切换经理大脑(rule 免费 / claude 真想)——下次经理循环启动时生效。
#[tauri::command]
fn update_role(
    state: State<'_, AppState>,
    id: String,
    patch: quiver_store::RolePatch,
) -> Result<quiver_store::AgentRole, String> {
    let store = state.store()?;
    store
        .update_role(&id, &patch, now_ms())
        .map_err(|e| format!("{e:#}"))
}

/// All persisted tasks for the bulletin-board UI (DESIGN §11, v1.0 module 5),
/// ordered by board position then time. Optional `project` / `status` filters.
#[tauri::command]
fn list_tasks(
    state: State<'_, AppState>,
    project: Option<String>,
    status: Option<String>,
) -> Result<Vec<TaskRecord>, String> {
    let store = state.store()?;
    store
        .list_tasks(project.as_deref(), status.as_deref())
        .map_err(|e| format!("{e:#}"))
}

/// Aggregate progression stats for the XP / level HUD (read-only, all projects).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    total: i64,
    verified: i64,
    failed: i64,
    cost_usd: f64,
    xp: i64,
    level: i64,
    /// Rolling-window spend (all projects), mirroring the §10 budget gate so the
    /// HUD「今夜花费」and the budget banner agree with what actually pauses the queue.
    spent_day: f64,
    spent_month: f64,
    /// Rolling-24h verified/failed counts — the「昨晚」morning report shows these so
    /// its tally reflects the night, not all-time.
    verified_day: i64,
    failed_day: i64,
}

/// Compute XP + level from the task table. Pure read-only aggregate — does not
/// touch the run / merge / verify path. XP = verified·100 + failed·20; level
/// grows on a sqrt curve so early wins level fast, later ones taper.
#[tauri::command]
fn get_stats(state: State<'_, AppState>) -> Result<Stats, String> {
    let store = state.store()?;
    let (total, verified, failed, cost_usd) = store.task_stats().map_err(|e| format!("{e:#}"))?;
    let xp = verified * 100 + failed * 20;
    let level = 1 + ((xp as f64) / 100.0).sqrt().floor() as i64;
    const DAY_MS: i64 = 86_400_000;
    let now = now_ms();
    let spent_day = store.cost_since(now - DAY_MS).map_err(|e| format!("{e:#}"))?;
    let spent_month = store.cost_since(now - 30 * DAY_MS).map_err(|e| format!("{e:#}"))?;
    let (_, verified_day, failed_day, _) = store
        .task_stats_since(now - DAY_MS)
        .map_err(|e| format!("{e:#}"))?;
    Ok(Stats {
        total,
        verified,
        failed,
        cost_usd,
        xp,
        level,
        spent_day,
        spent_month,
        verified_day,
        failed_day,
    })
}

/// 观测指标(DESIGN §10-12)的 wire 形:由 [`quiver_core::metrics::Metrics`] 投影成
/// camelCase,额外带派生的 `verifyRate` / `avgCostUsd`,给验收台 / 自治度面板用。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricsDto {
    runs: u64,
    verified: u64,
    failed: u64,
    total_cost_usd: f64,
    total_tokens: u64,
    p50_duration_ms: u64,
    p95_duration_ms: u64,
    /// 验收率(自治度核心指标)。
    verify_rate: f64,
    avg_cost_usd: f64,
}

/// 聚合所有项目里**已结束**任务(verified/done/failed)的运行指标(§10-12)。
/// 每个任务一条样本:花费取 `cost_usd`,时长用 `updated_at - created_at` 墙钟代理
/// (per-task tokens 暂未单独入库,记 0),验收 = 状态属 verified/done。纯读聚合,不碰调度。
#[tauri::command]
fn get_metrics(state: State<'_, AppState>) -> Result<MetricsDto, String> {
    let store = state.store()?;
    let samples = store.metric_samples().map_err(|e| format!("{e:#}"))?;
    Ok(metrics_from_samples(&samples))
}

/// 把任务样本映射成运行样本并聚合(§10-12)。抽成纯函数便于单测;tauri 命令只读 store 后调它。
/// 只算已结束任务;tokens/时长优先用 agent 报的真值,缺则退回 0 / 墙钟代理(updated-created)。
fn metrics_from_samples(samples: &[MetricSample]) -> MetricsDto {
    use quiver_core::metrics::{aggregate, RunSample};
    let runs: Vec<RunSample> = samples
        .iter()
        .filter(|s| matches!(s.status.as_str(), "verified" | "done" | "failed"))
        .map(|s| RunSample {
            cost_usd: s.cost_usd.unwrap_or(0.0),
            tokens: s.tokens.unwrap_or(0).max(0) as u64,
            duration_ms: s
                .duration_ms
                .unwrap_or((s.updated_at - s.created_at).max(0))
                .max(0) as u64,
            verified: matches!(s.status.as_str(), "verified" | "done"),
        })
        .collect();
    let m = aggregate(&runs);
    MetricsDto {
        runs: m.runs,
        verified: m.verified,
        failed: m.failed,
        total_cost_usd: m.total_cost_usd,
        total_tokens: m.total_tokens,
        p50_duration_ms: m.p50_duration_ms,
        p95_duration_ms: m.p95_duration_ms,
        verify_rate: m.verify_rate(),
        avg_cost_usd: m.avg_cost_usd(),
    }
}

/// AI 经理在「此刻真实局面」下会做的决策预览(DESIGN §5/§21):组装真实 ManagerContext
/// (在途/排队/在途上限/预算剩余),用**免费的 RuleBrain** 跑一拍 —— 只看不动:不 spawn、
/// 不花钱、不碰 run/调度路径。给 UI 展示"经理会怎么想",也为真接调度铺垫。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagerPreviewDto {
    inflight: usize,
    queued: usize,
    max_inflight: usize,
    budget_remaining_usd: f64,
    /// RuleBrain 的决策(§21 tagged action);真接千问大脑是后续刀。
    decision: quiver_orchestrator::Decision,
}

#[tauri::command]
async fn manager_preview(state: State<'_, AppState>) -> Result<ManagerPreviewDto, String> {
    use quiver_orchestrator::{ManagerBrain, ManagerContext, RuleBrain};
    let store = state.store()?;
    let running = store
        .list_tasks(None, Some("running"))
        .map_err(|e| format!("{e:#}"))?
        .len();
    let queued = store
        .list_tasks(None, Some("queued"))
        .map_err(|e| format!("{e:#}"))?
        .len();
    let settings = store.get_settings().map_err(|e| format!("{e:#}"))?;
    const DAY_MS: i64 = 86_400_000;
    let spent = store.cost_since(now_ms() - DAY_MS).map_err(|e| format!("{e:#}"))?;
    // 有夜预算 → 剩余=上限-今夜花费(夹 0);没设 → 视为充裕(不因预算挡)。
    let budget_remaining = settings
        .nightly_budget_usd
        .map(|cap| (cap - spent).max(0.0))
        .unwrap_or(1_000_000.0);
    let ctx = ManagerContext {
        inflight: running,
        queued,
        max_inflight: settings.max_workers.max(0) as usize,
        budget_remaining_usd: budget_remaining,
        brief: String::new(),
        // 预览在经理循环之外,看不到(也无需看)循环私有的复核队列/队首细节。
        pending_reviews: vec![],
        next_task: None,
        team: vec![],
    };
    let decision = RuleBrain.decide(&ctx).await.map_err(|e| format!("{e:#}"))?;
    Ok(ManagerPreviewDto {
        inflight: ctx.inflight,
        queued: ctx.queued,
        max_inflight: ctx.max_inflight,
        budget_remaining_usd: ctx.budget_remaining_usd,
        decision,
    })
}

/// 一次独立审计的结果(DESIGN §8)。`audited=false` 表示没东西可审(无分支 / 没配 verify)。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditResult {
    audited: bool,
    passed: Option<bool>,
    reason: String,
}

/// §8 独立审计:把某任务的成果分支**完整克隆到干净副本**、重跑配置的 verify 命令,看是否
/// 仍通过 —— 在 worker 碰不到的副本里重跑,挫败它在自己 worktree 里改弱测试骗验收。
/// 用户可触发;无独立分支(simulate / 已合并)或没配 verify 命令则短路不审。不改 gate 决策。
#[tauri::command]
fn audit_task(state: State<'_, AppState>, task_id: String) -> Result<AuditResult, String> {
    use quiver_core::audit::clean_clone_verify;
    use quiver_core::verify::VerifyCommand;
    let store = state.store()?;
    let task = store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在".to_string())?;
    let Some(branch) = task.branch else {
        return Ok(AuditResult {
            audited: false,
            passed: None,
            reason: "任务无独立分支(simulate / 已合并),无可审计".to_string(),
        });
    };
    let settings = store.get_settings().map_err(|e| format!("{e:#}"))?;
    if settings.verify_command.trim().is_empty() {
        return Ok(AuditResult {
            audited: false,
            passed: None,
            reason: "未配置 verify 命令,无法独立审计".to_string(),
        });
    }
    let project = current_project(&state)?;
    // 唯一临时目录(git clone 要求目标不存在);用纳秒时间戳防撞。
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dest = std::env::temp_dir().join(format!("quiver-audit-{task_id}-{nonce}"));
    let verify = VerifyCommand::shell(settings.verify_command);
    let result = clean_clone_verify(&project, &branch, &verify, &dest);
    let _ = std::fs::remove_dir_all(&dest); // 审完清理临时副本
    let passed = result.map_err(|e| format!("独立审计执行失败:{e:#}"))?;
    Ok(AuditResult {
        audited: true,
        passed: Some(passed),
        reason: if passed {
            "干净克隆重跑 verify 通过".to_string()
        } else {
            "干净克隆重跑 verify 未通过(疑似 worktree 内作弊 / 真红)".to_string()
        },
    })
}

/// How many facts / episodes a brief carries (DESIGN §6 简报). Bounded so the
/// brief stays a compact context snapshot, not a memory dump.
const BRIEF_FACT_LIMIT: usize = 20;
const BRIEF_EPISODE_LIMIT: usize = 10;

/// The current project's memory brief (DESIGN §6): current-truth facts + recent
/// CEO 给公司注入一条**权威事实**(§6.2 权威档):项目约束 / 已定方案 / 领域知识。存进
/// 记忆,注入经理简报 —— claude 经理决策时读得到(记忆 → 决策)。空文本拒绝。
#[tauri::command]
fn add_authoritative_fact(
    state: State<'_, AppState>,
    text: String,
    topic: Option<String>,
    importance: Option<i64>,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("事实内容不能为空".to_string());
    }
    // 主题(entity):同主题的旧低档事实会被这条权威事实机械作废(§6.4)。空 → 不框定主题、不作废。
    let entity = topic.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let project = current_project(&state)?.display().to_string();
    let mem = state.memory()?;
    let now = now_ms();
    let new_id = mem
        .insert_fact(&NewFact {
            project,
            scope: None,
            kind: "知识".to_string(),
            text,
            entities: None,
            entity: entity.clone(),
            importance: Some(importance.unwrap_or(9).clamp(1, 9)),
            valid_at: None,
            recorded_at: now,
            provenance: Some("CEO".to_string()),
            trust: "权威".to_string(),
            source_commit: None,
            source_episode_id: None,
        })
        .map_err(|e| format!("{e:#}"))?;
    // §6.4 机械矛盾消解:带主题时,作废同主题更低可信度的旧事实(如旧的员工汇报"用 RocksDB")。
    if entity.is_some() {
        let _ = mem.supersede_lower_same_entity(new_id, now);
    }
    Ok(())
}

/// episodes, for the manager "简报书" panel. Empty (not an error) before any
/// memory is recorded — a fresh project simply has nothing yet.
#[tauri::command]
fn get_brief(state: State<'_, AppState>) -> Result<Brief, String> {
    let project = current_project(&state)?.display().to_string();
    let memory = state.memory()?;
    memory
        .brief(&project, BRIEF_FACT_LIMIT, BRIEF_EPISODE_LIMIT)
        .map_err(|e| format!("{e:#}"))
}

/// 当前项目近期 episode(DESIGN §6.2,最新在前,默认上限 50),供时间轴 / 档案 UI 回放
/// "发生过什么"。只读;记忆未初始化或项目未选时报错(由 UI 兜成空)。
#[tauri::command]
fn get_episodes(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<EpisodeRecord>, String> {
    let project = current_project(&state)?.display().to_string();
    let memory = state.memory()?;
    memory
        .episodes_for_project(&project, limit.unwrap_or(50))
        .map_err(|e| format!("{e:#}"))
}

/// Suggest a verify-gate command (DESIGN §7) by sniffing the current project for
/// well-known build/test markers. Read-only; returns "" when nothing is
/// recognized or no project is selected, so the UI just shows no hint. Purely a
/// convenience nudge to help the user enable the gate.
#[tauri::command]
fn suggest_verify_command(state: State<'_, AppState>) -> Result<String, String> {
    let project = match current_project(&state) {
        Ok(p) => p,
        Err(_) => return Ok(String::new()),
    };
    let has = |f: &str| project.join(f).exists();
    let cmd = if has("Cargo.toml") {
        "cargo test"
    } else if has("package.json") {
        "npm test"
    } else if has("pyproject.toml") || has("setup.py") {
        "pytest"
    } else if has("go.mod") {
        "go test ./..."
    } else if has("Makefile") || has("makefile") {
        "make test"
    } else {
        ""
    };
    Ok(cmd.to_string())
}

/// Move a task to a new board `position` (drag-to-reorder).
#[tauri::command]
fn reorder_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    position: i64,
) -> Result<(), String> {
    let store = state.store()?;
    store
        .reorder_task(&id, position, now_ms())
        .map_err(|e| format!("{e:#}"))?;
    // Tell the board to re-read so the new order shows immediately.
    let _ = app.emit_task_update();
    Ok(())
}

/// Send SIGTERM to a child PID. The agent exits → its stdout closes → the run's
/// drain loop ends through the normal path, which GCs the worktree (§8.4).
/// Cancel a running task by killing its agent's whole PROCESS GROUP, not just the
/// agent pid (DESIGN §23 P3 "真急停 killpg"). The runner spawns each agent as its
/// own process-group leader (pgid == pid via `process_group(0)`), so signalling
/// the negative pid reaps the agent AND any children it spawned — never leaving an
/// orphan. SIGKILL is decisive (cancel must stop work now); the dead agent's
/// stdout EOF still routes the run through the normal §8.4 cleanup path.
fn kill_pid(pid: u32) {
    #[cfg(unix)]
    {
        // SAFETY: a plain kill(2) syscall. A negative pid targets the process
        // group whose id is the absolute value — here the agent's own group.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("kill")
            .arg("-KILL")
            .arg(pid.to_string())
            .status();
    }
}

/// Cancel a task. QUEUED → remove its commission from the board. RUNNING /
/// VERIFYING → kill its agent process (it stops + cleans up via the normal path).
/// Already-finished tasks can't be cancelled.
#[tauri::command]
fn cancel_task_cmd(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let store = state.store()?;
    let task = store
        .get_task(&id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在或已被移除".to_string())?;
    match task.status.as_str() {
        "queued" => {
            store.delete_task(&id).map_err(|e| format!("{e:#}"))?;
        }
        "running" | "verifying" => {
            let pid = state
                .running_pids
                .lock()
                .ok()
                .and_then(|m| m.get(&id).copied());
            match pid {
                Some(pid) => {
                    kill_pid(pid);
                    if let Ok(mut m) = state.running_pids.lock() {
                        m.remove(&id);
                    }
                }
                None => return Err("找不到运行中的进程（可能刚结束）".to_string()),
            }
        }
        _ => return Err("该委托已结束，无法取消".to_string()),
    }
    let _ = app.emit_task_update();
    Ok(())
}

/// The full ordered event log for one task — the §11 source of truth, for Phase
/// D's Logbook replay.
#[tauri::command]
fn get_task_events(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<StoredEvent>, String> {
    let store = state.store()?;
    store.events_for_task(&task_id).map_err(|e| format!("{e:#}"))
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
    store: &Arc<Store>,
    settings: &Settings,
    cwd: PathBuf,
) -> Arc<dyn quiver_orchestrator::ManagerBrain> {
    let role = store.get_role("manager").ok().flatten();
    if let Some(role) = role.filter(|r| r.brain == "claude") {
        if let Ok(bin) = crate::run::resolve_agent_bin(settings, RunMode::Real) {
            return Arc::new(claude_brain::ClaudeBrain::new(bin, cwd, role.model));
        }
    }
    Arc::new(quiver_orchestrator::RuleBrain)
}

/// Pin a task to the bulletin board (status `queued`) and kick the concurrent
/// scheduler, which will run it (and any other queued tasks) up to `maxWorkers`
/// at a time. Returns the created [`TaskRecord`] so the UI can flash the new
/// commission card immediately. This is the Phase-C "加入公告板" enqueue path —
/// distinct from the single-shot `run_task_cmd`.
#[tauri::command]
async fn enqueue_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    mode: RunMode,
) -> Result<TaskRecord, String> {
    let project = current_project(&state)?;
    let project_str = project.display().to_string();
    let store = state.store()?;

    let task_id = format!("task-{}", now_ms());
    store
        .enqueue_task(&NewTask {
            id: task_id.clone(),
            project: project_str.clone(),
            prompt,
            mode: mode.label().to_string(),
            status: "queued".to_string(),
            created_at: now_ms(),
        })
        .map_err(|e| format!("{e:#}"))?;

    // Read the live settings: max-workers cap + the §5 autonomous switch.
    let settings = store.get_settings().ok();
    let max_workers = settings
        .as_ref()
        .map(|s| s.max_workers as usize)
        .unwrap_or(1)
        .max(1);
    let autonomous = settings.as_ref().map(|s| s.autonomous).unwrap_or(false);
    if autonomous {
        // 自治模式:经理控制循环驱动调度 —— 经理拍决策、决策真的调动 worker(DESIGN §5)。
        // 经理大脑由循环每拍按人事部配置现场选(rule 免费/claude 真想),这里不注入。
        state
            .manager
            .ensure_running(app.clone(), store.clone(), project, max_workers)
            .await;
    } else {
        // 旧模式:scheduler 无脑流水线(有名额+有排队就跑),作为可回退的默认。
        state
            .scheduler
            .ensure_running(app.clone(), store.clone(), project, max_workers)
            .await;
    }

    // Flash the board + return the new card.
    let _ = app.emit_task_update();
    store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务刚入队却读不到，请重试".to_string())
}

/// §12 打回重做(组织回流):把一个已终结(失败/被经理拦下)的任务作为**新任务**重新入队。
/// 新 task_id —— 事件日志主键是 (task_id,seq),复用老行重跑会撞 seq;老行保留作档案。
/// **带上下文返工(§5.3/A5)**:老任务的会话句柄复制给新任务 → worker 经 `--resume` 在
/// 原会话里续跑(记得自己改过什么、为什么没过),配上返工指引,不是裸重跑;经理简报里
/// 还带着「经理·拦下(原因)」的 episode,记忆双保险。
#[tauri::command]
async fn requeue_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<TaskRecord, String> {
    let store = state.store()?;
    let old = store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在".to_string())?;
    if matches!(old.status.as_str(), "queued" | "running" | "verifying") {
        return Err("任务还在进行中,不能打回重做".to_string());
    }
    let old_session = store.task_session_id(&old.id).ok().flatten();
    let prompt = if old_session.is_some() {
        // 续会话返工:worker 记得上次的上下文,指引点明这次要干嘛。
        format!("(返工)上次运行的成果未通过验收,请修复问题后重新交付。原任务:{}", old.prompt)
    } else {
        old.prompt
    };
    let new = enqueue_task_cmd(app, state, prompt, RunMode::from_label(&old.mode)).await?;
    if let Some(sid) = old_session {
        // 把老会话句柄挂到新任务行 → run_one_task 读到即走 runner.resume(已有链路)。
        let _ = store.set_task_session_id(&new.id, &sid, now_ms());
    }
    Ok(new)
}

/// Run ONE task immediately against the picked repo and stream its events to the
/// UI (legacy single-shot path, kept for the original quick-run control). The
/// task row is enqueued `running` up front, shares a fresh per-call git guard,
/// and its lifecycle is persisted by [`run_one_task`].
#[tauri::command]
async fn run_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    mode: RunMode,
) -> Result<(), String> {
    let project = current_project(&state)?;
    let project_str = project.display().to_string();
    let store = state.store()?;

    let task_id = format!("task-{}", now_ms());
    let _ = store.enqueue_task(&NewTask {
        id: task_id.clone(),
        project: project_str,
        prompt: prompt.clone(),
        mode: mode.label().to_string(),
        status: "running".to_string(),
        created_at: now_ms(),
    });
    let _ = app.emit_task_update();

    let guard = quiver_core::git::GitGuard::new(project);
    run_one_task(&app, &store, &guard, task_id, prompt, mode).await;
    let _ = app.emit_task_update();
    Ok(())
}

/// The currently-picked project, or a clear "pick a project first" error.
///
/// 重启自愈:内存里没有当前项目(进程刚重启、前端还没调 get_initial_state)时,从持久化的
/// `last_project` 恢复并回填内存 —— 否则自治公司一重启,IPC 全报"先选项目",enqueue 瘫痪。
/// 持久化里也没有(全新安装)才真要用户去选。
fn current_project(state: &AppState) -> Result<PathBuf, String> {
    if let Some(p) = state.project_path.lock().expect("project_path lock").clone() {
        return Ok(p);
    }
    if let Ok(store) = state.store() {
        if let Ok(Some(last)) = store.last_project() {
            let p = PathBuf::from(&last);
            *state.project_path.lock().expect("project_path lock") = Some(p.clone());
            return Ok(p);
        }
    }
    Err("请先选择一个项目——尚未选择 git 仓库。".to_string())
}

/// Validate `path` is a git repo, store it as the picked project (in memory +
/// durable last_project), and touch its recent-projects entry.
fn select_validated_project(state: &AppState, path: PathBuf) -> Result<String, String> {
    if !path.join(".git").exists() {
        return Err(format!(
            "{} 这不是一个 git 仓库（没有 .git 目录）。请选择一个 git 仓库。",
            path.display()
        ));
    }

    let path_str = path.display().to_string();
    *state.project_path.lock().expect("project_path lock") = Some(path.clone());

    let store = state.store()?;
    store
        .set_last_project(&path_str)
        .map_err(|e| format!("{e:#}"))?;
    store
        .touch_recent_project(&path_str, now_ms())
        .map_err(|e| format!("{e:#}"))?;

    Ok(path_str)
}

/// Wall-clock millis since the epoch.
pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A tiny extension so command handlers emit a board-refresh ping without
/// repeating the channel name. The payload is unit — the board just re-reads
/// `list_tasks` on any ping.
trait EmitTaskUpdate {
    fn emit_task_update(&self) -> tauri::Result<()>;
}

impl EmitTaskUpdate for AppHandle {
    fn emit_task_update(&self) -> tauri::Result<()> {
        use tauri::Emitter;
        self.emit(TASK_EVENT_CHANNEL, "")
    }
}

/// Build and run the Tauri application. Called from `main.rs` AFTER
/// `fix_path_env::fix()` has repaired the process `PATH` (§12).
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            let _ = app.get_webview_window("main");

            // Start the tauri-agent-tools dev bridge (debug builds only). A
            // failure here must not block app startup, so we only warn.
            #[cfg(debug_assertions)]
            if let Err(e) = dev_bridge::start_bridge(app.handle()) {
                eprintln!("Warning: failed to start dev bridge: {e}");
            }

            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("could not resolve app data dir: {e}"))?;
            let db_path = Store::default_db_path(&data_dir);
            let store = Store::open(&db_path).map_err(|e| {
                format!("could not open quiver.sqlite at {}: {e:#}", db_path.display())
            })?;
            let state = app.state::<AppState>();
            let _ = state.store.set(Arc::new(store));

            // Open the durable agent memory (DESIGN §6/§20) alongside the operational
            // store — a separate memory.sqlite. A failure here must not block startup
            // (memory is additive to the run loop), so we only warn and carry on.
            let mem_path = MemoryStore::default_db_path(&data_dir);
            match MemoryStore::open(&mem_path) {
                Ok(mem) => {
                    let _ = state.memory.set(Arc::new(mem));
                }
                Err(e) => eprintln!(
                    "Warning: could not open memory.sqlite at {}: {e:#}",
                    mem_path.display()
                ),
            }

            // Crash recovery (DESIGN §23 P0): a prior session may have died mid-run,
            // leaving tasks stuck `running` and orphan worktrees behind. Requeue
            // them, sweep the orphans, and restart their projects' dispatchers —
            // 按 §5 自治开关分流:自治开着由**经理循环**接管恢复(组织重启后仍由经理
            // 驱动,不降级回无脑流水线),关着走旧 scheduler。
            // Spawned async so startup never blocks on git/db work.
            if let Some(store) = state.store.get().cloned() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let settings = store.get_settings().ok();
                    let max_workers = settings
                        .as_ref()
                        .map(|s| s.max_workers as usize)
                        .unwrap_or(1);
                    let autonomous = settings.as_ref().map(|s| s.autonomous).unwrap_or(false);
                    let app_state = handle.state::<AppState>();
                    let n = if autonomous {
                        app_state
                            .manager
                            .reconcile(handle.clone(), store, max_workers)
                            .await
                    } else {
                        app_state
                            .scheduler
                            .reconcile(handle.clone(), store, max_workers)
                            .await
                    };
                    if n > 0 {
                        eprintln!("reconcile: requeued {n} interrupted task(s) from a prior session");
                    }
                });
            }
            Ok(())
        });

    // Tauri supports only a single `invoke_handler`, and `generate_handler!`
    // needs a literal command list — so the debug-only dev-bridge callback is
    // wired via two cfg'd arms instead of being appended at runtime.
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        pick_project,
        select_recent_project,
        remove_recent_project,
        set_project_alias,
        get_initial_state,
        run_task_cmd,
        enqueue_task_cmd,
        requeue_task_cmd,
        get_settings,
        update_settings,
        list_roles,
        update_role,
        hire_worker,
        fire_worker,
        add_authoritative_fact,
        get_decisions,
        list_tasks,
        get_stats,
        get_metrics,
        manager_preview,
        audit_task,
        get_brief,
        get_episodes,
        suggest_verify_command,
        reorder_task,
        cancel_task_cmd,
        get_task_events,
        check_environment,
        dev_bridge::__dev_bridge_result
    ]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        pick_project,
        select_recent_project,
        remove_recent_project,
        set_project_alias,
        get_initial_state,
        run_task_cmd,
        enqueue_task_cmd,
        requeue_task_cmd,
        get_settings,
        update_settings,
        list_roles,
        update_role,
        hire_worker,
        fire_worker,
        add_authoritative_fact,
        get_decisions,
        list_tasks,
        get_stats,
        get_metrics,
        manager_preview,
        audit_task,
        get_brief,
        get_episodes,
        suggest_verify_command,
        reorder_task,
        cancel_task_cmd,
        get_task_events,
        check_environment
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running Quiver");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        status: &str,
        cost: Option<f64>,
        tokens: Option<i64>,
        duration: Option<i64>,
        created: i64,
        updated: i64,
    ) -> MetricSample {
        MetricSample {
            cost_usd: cost,
            tokens,
            duration_ms: duration,
            created_at: created,
            updated_at: updated,
            status: status.into(),
        }
    }

    #[test]
    fn metrics_counts_only_terminal_samples() {
        let samples = vec![
            sample("verified", Some(0.10), Some(100), Some(500), 0, 1000),
            sample("done", Some(0.20), Some(200), None, 0, 3000), // 无真时长 → 墙钟代理 3000
            sample("failed", Some(0.30), Some(300), Some(2000), 0, 2000),
            sample("running", Some(0.99), Some(999), Some(9000), 0, 9000), // 在跑,不计
            sample("queued", None, None, None, 0, 0),                      // 排队,不计
        ];
        let m = metrics_from_samples(&samples);
        assert_eq!(m.runs, 3, "只算 verified/done/failed");
        assert_eq!(m.verified, 2);
        assert_eq!(m.failed, 1);
        assert!((m.total_cost_usd - 0.60).abs() < 1e-9);
        assert_eq!(m.total_tokens, 600, "tokens 用 agent 报的真值之和");
        assert!((m.verify_rate - 2.0 / 3.0).abs() < 1e-9);
        // 时长 [500, 2000, 3000(代理)] 排序后 p50 = 2000。
        assert_eq!(m.p50_duration_ms, 2000);
    }

    #[test]
    fn empty_samples_zero_metrics() {
        let m = metrics_from_samples(&[]);
        assert_eq!(m.runs, 0);
        assert_eq!(m.verify_rate, 0.0);
    }
}
