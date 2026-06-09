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

mod environment;
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

use quiver_memory::MemoryStore;
use quiver_store::{
    InitialState, NewTask, Settings, SettingsPatch, Store, StoredEvent, TaskRecord,
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
    state.scheduler.resume_all(app, store.clone()).await;
    Ok(settings)
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
    Ok(Stats {
        total,
        verified,
        failed,
        cost_usd,
        xp,
        level,
        spent_day,
        spent_month,
    })
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

    // Read the live max-workers cap and kick the scheduler for this project.
    let max_workers = store
        .get_settings()
        .map(|s| s.max_workers as usize)
        .unwrap_or(1)
        .max(1);
    state
        .scheduler
        .ensure_running(app.clone(), store.clone(), project, max_workers)
        .await;

    // Flash the board + return the new card.
    let _ = app.emit_task_update();
    store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务刚入队却读不到，请重试".to_string())
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
fn current_project(state: &AppState) -> Result<PathBuf, String> {
    state
        .project_path
        .lock()
        .expect("project_path lock")
        .clone()
        .ok_or_else(|| "请先选择一个项目——尚未选择 git 仓库。".to_string())
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
            // them, sweep the orphans, and restart their projects' dispatchers.
            // Spawned async so startup never blocks on git/db work.
            if let Some(store) = state.store.get().cloned() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let max_workers = store
                        .get_settings()
                        .map(|s| s.max_workers as usize)
                        .unwrap_or(1);
                    let n = handle
                        .state::<AppState>()
                        .scheduler
                        .reconcile(handle.clone(), store, max_workers)
                        .await;
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
        get_settings,
        update_settings,
        list_tasks,
        get_stats,
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
        get_settings,
        update_settings,
        list_tasks,
        get_stats,
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
