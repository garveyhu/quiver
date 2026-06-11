//! Quiver Tauri app crate (DESIGN §3 three-layer architecture).
//!
//! This is the Rust "core" half of the Tauri app. It owns the Tauri runtime and
//! the **app skeleton**: the picked-project [`AppState`], the durable store +
//! agent memory, the project-state helpers ([`current_project`] /
//! [`select_validated_project`]), and the [`run`] builder.
//!
//! Everything else lives in a focused module:
//! - the IPC surface the React UI talks to is split by business domain under
//!   [`commands`] (project / settings / roles / tasks / observability / memory /
//!   manager preview) and re-exported flat so [`run`]'s `invoke_handler` lists
//!   bare names;
//! - the supervisor logic — worktrees, the verify-gate, merge — lives in
//!   `quiver-core`; per-task run plumbing in [`run`]; the autonomous manager
//!   control loop in [`manager`]; the concurrent queue scheduler in [`scheduler`].

mod claude_brain;
mod commands;
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

use tauri::{AppHandle, Manager};

use quiver_memory::MemoryStore;
use quiver_store::Store;

use crate::environment::check_environment;
use crate::scheduler::{Scheduler, TASK_EVENT_CHANNEL};

// 各业务域的 IPC 命令(`#[tauri::command]`)拉进 crate 根作用域,让下面 `invoke_handler`
// 继续用裸名注册 —— 拆分前后注册列表一字不改。
use commands::*;

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
    /// The durable store, installed in `setup`. `pub(crate)` so the [`commands`]
    /// submodules can read it off the injected `State<AppState>`.
    pub(crate) fn store(&self) -> Result<Arc<Store>, String> {
        self.store
            .get()
            .cloned()
            .ok_or_else(|| "持久化存储尚未初始化".to_string())
    }

    /// The durable agent memory, installed in `setup`.
    pub(crate) fn memory(&self) -> Result<Arc<MemoryStore>, String> {
        self.memory
            .get()
            .cloned()
            .ok_or_else(|| "记忆存储尚未初始化".to_string())
    }
}

/// The currently-picked project, or a clear "pick a project first" error.
///
/// 重启自愈:内存里没有当前项目(进程刚重启、前端还没调 get_initial_state)时,从持久化的
/// `last_project` 恢复并回填内存 —— 否则自治公司一重启,IPC 全报"先选项目",enqueue 瘫痪。
/// 持久化里也没有(全新安装)才真要用户去选。
pub(crate) fn current_project(state: &AppState) -> Result<PathBuf, String> {
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
pub(crate) fn select_validated_project(state: &AppState, path: PathBuf) -> Result<String, String> {
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
/// `list_tasks` on any ping. `pub(crate)` so [`commands::tasks`] can call it.
pub(crate) trait EmitTaskUpdate {
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
                            .reconcile(handle.clone(), store.clone(), max_workers)
                            .await
                    } else {
                        app_state
                            .scheduler
                            .reconcile(handle.clone(), store.clone(), max_workers)
                            .await
                    };
                    if n > 0 {
                        eprintln!("reconcile: requeued {n} interrupted task(s) from a prior session");
                    }
                    // 「给个方向就自治」重启后也恢复:autonomous + 设了目标但无 pending 任务时,
                    // reconcile 只管有 pending 活的项目 → 不会启动任何循环,纯目标自治推不动。补:有
                    // 自治目标 + current project → 主动起经理循环(它会主动 plan 推进目标),过夜关机
                    // 重启也能续上。
                    if autonomous {
                        let goal = settings
                            .as_ref()
                            .map(|s| s.autonomous_goal.trim().to_string())
                            .unwrap_or_default();
                        if !goal.is_empty() {
                            if let Ok(project) = crate::current_project(&app_state) {
                                app_state
                                    .manager
                                    .ensure_running(handle.clone(), store, project, max_workers.max(1))
                                    .await;
                            }
                        }
                    }
                });
            }
            Ok(())
        });

    // Tauri supports only a single `invoke_handler`, and `generate_handler!`
    // needs a literal command list — so the debug-only dev-bridge callback is
    // wired via two cfg'd arms instead of being appended at runtime. The command
    // names resolve to the re-exported `commands::*` handlers (+ `check_environment`).
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
        merge_task_cmd,
        get_settings,
        update_settings,
        list_roles,
        update_role,
        hire_worker,
        fire_worker,
        add_authoritative_fact,
        get_memory_facts,
        retire_fact_cmd,
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
        merge_task_cmd,
        get_settings,
        update_settings,
        list_roles,
        update_role,
        hire_worker,
        fire_worker,
        add_authoritative_fact,
        get_memory_facts,
        retire_fact_cmd,
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
