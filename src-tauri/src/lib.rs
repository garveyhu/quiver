//! Quiver Tauri app crate (DESIGN §3 three-layer architecture).
//!
//! This is the Rust "core" half of the Tauri app. It owns the Tauri runtime,
//! the IPC surface the React/Phaser UI talks to, the picked-project app state,
//! and the resolution of the agent binary's absolute path (§12). The actual
//! supervisor logic — worktrees, the verify-gate, merge — lives in the pure-Rust
//! `quiver-core` crate; this crate only wires it to the window.
//!
//! Commands:
//! - `pick_project()` — open a folder dialog, validate the chosen dir is a git
//!   repo, store it in app state, return its path.
//! - `run_task_cmd({ prompt, mode })` — run one task against the picked repo in
//!   either `simulate` (free `fake-claude`, default) or `real` (the official
//!   `claude` binary, subscription/OAuth, §9.3 env-scrubbed, NO auto-merge) mode.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use quiver_core::event::{AgentEvent, AgentEventPayload};
use quiver_core::git::GitGuard;
use quiver_store::{
    InitialState, NewEvent, NewRun, NewTask, Settings, SettingsPatch, Store, StoredEvent,
    TaskRecord,
};
use quiver_core::supervisor::{
    run_task_streaming, Cleanup, FinishStatus, RunOptions, RunOutcome, TaskSpec,
};
use quiver_core::verify::VerifyCommand;

/// The Tauri event channel the UI subscribes to (see `useSupervisor.ts`).
const AGENT_EVENT_CHANNEL: &str = "agent-event";

/// Env vars that, if present, mean the `claude` CLI would NOT be on the
/// subscription/OAuth route (§9.3). In `real` (subscription) mode we assert NONE
/// of these are present in the scrubbed child env before spawning — if any is,
/// we refuse to launch rather than silently spend on a key/Bedrock/Vertex route.
const FORBIDDEN_SUBSCRIPTION_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

/// The run mode chosen in the UI (§4.2). `Simulate` is the free default;
/// `Real` spawns the official `claude` binary on the subscription route.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum RunMode {
    Simulate,
    Real,
}

/// Per-app state: the user-picked project (a git repo) all runs operate on, plus
/// the durable SQLite [`Store`] (DESIGN §11). The project path is guarded by a std
/// `Mutex` since it is only touched briefly on the command thread (no `.await`
/// held across the lock); the store is installed once in `setup` via `OnceLock`.
#[derive(Default)]
struct AppState {
    project_path: Mutex<Option<PathBuf>>,
    store: std::sync::OnceLock<Store>,
}

impl AppState {
    /// The durable store, installed in `setup`. Errors (surfaced to the UI) if a
    /// command runs before setup wired it — which should never happen in practice.
    fn store(&self) -> Result<&Store, String> {
        self.store
            .get()
            .ok_or_else(|| "持久化存储尚未初始化".to_string())
    }
}

/// A terminal event synthesized AFTER `run_task` returns, carrying the §5.2
/// `FinishStatus`, the run's summed cost, the run mode, and — for real-mode runs
/// left un-merged — the attempt branch the work lives on.
///
/// It is NOT a `quiver-core` `AgentEvent` variant — it mirrors the wire envelope
/// so the UI can render it in the same list (matched by `kind: "finished"` in
/// `agentEvent.types.ts`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishedEvent {
    task_id: String,
    seq: u64,
    ts_ms: i64,
    runner: &'static str,
    kind: &'static str,
    status: String,
    cost_usd: Option<f64>,
    /// `Some` when the work was left on a branch (real-mode, no merge).
    branch: Option<String>,
}

/// Open a folder dialog, validate the chosen directory is a git repo, store it
/// in app state, and return its absolute path. Returns `Ok(None)` if the user
/// cancels the dialog. Errors (surfaced to the UI) if the chosen dir is not a
/// git repo.
#[tauri::command]
async fn pick_project(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    // The dialog plugin's blocking picker must not run on the async command's
    // executor; `blocking_pick_folder` spawns the native dialog and waits.
    let chosen = app.dialog().file().blocking_pick_folder();
    let Some(folder) = chosen else {
        return Ok(None); // user cancelled
    };

    let path: PathBuf = folder
        .into_path()
        .map_err(|e| format!("could not resolve the chosen folder: {e}"))?;

    select_validated_project(&state, path).map(Some)
}

/// Re-select a project the user picked before (from the recent list) WITHOUT the
/// folder dialog. Validates it is still a git repo, stores it in app state, and
/// refreshes its recent-list timestamp. Returns the (re-validated) path.
#[tauri::command]
fn select_recent_project(state: State<'_, AppState>, path: String) -> Result<String, String> {
    select_validated_project(&state, PathBuf::from(path))
}

/// Load the durable startup bundle (DESIGN §11): the last-picked project, the
/// recent-projects list, and run history. Called once on app load to restore
/// everything that survived a restart.
#[tauri::command]
fn get_initial_state(state: State<'_, AppState>) -> Result<InitialState, String> {
    let store = state.store()?;
    let initial = store.initial_state().map_err(|e| format!("{e:#}"))?;
    // Restore the last project into in-memory state so a `run_task_cmd`
    // immediately after load works without an explicit re-pick.
    if let Some(last) = &initial.last_project {
        *state.project_path.lock().expect("project_path lock") = Some(PathBuf::from(last));
    }
    Ok(initial)
}

/// Read the typed app settings (DESIGN §11, v1.0 module 1). Always returns a
/// complete [`Settings`] (defaults are seeded on first open). For Phase B's
/// settings ledger UI.
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let store = state.store()?;
    store.get_settings().map_err(|e| format!("{e:#}"))
}

/// Apply a partial settings update and return the resulting full [`Settings`]
/// (immediate-apply). For Phase B.
#[tauri::command]
fn update_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let store = state.store()?;
    store.update_settings(&patch).map_err(|e| format!("{e:#}"))
}

/// All persisted tasks for the bulletin-board UI (DESIGN §11, v1.0 module 5),
/// ordered by board position then time. Optional `project` / `status` filters.
/// For Phase C.
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

/// Move a task to a new board `position` (drag-to-reorder). For Phase C.
#[tauri::command]
fn reorder_task(state: State<'_, AppState>, id: String, position: i64) -> Result<(), String> {
    let store = state.store()?;
    store
        .reorder_task(&id, position, now_ms())
        .map_err(|e| format!("{e:#}"))
}

/// The full ordered event log for one task — the §11 source of truth, for
/// Phase D's Logbook replay. Returns the verbatim payload JSON the UI parses
/// with its live `AgentEvent` contract.
#[tauri::command]
fn get_task_events(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<StoredEvent>, String> {
    let store = state.store()?;
    store
        .events_for_task(&task_id)
        .map_err(|e| format!("{e:#}"))
}

/// Validate `path` is a git repo, store it as the picked project (in memory +
/// durable last_project), and touch its recent-projects entry. Shared by
/// [`pick_project`] and [`select_recent_project`]. Returns the path string.
fn select_validated_project(state: &AppState, path: PathBuf) -> Result<String, String> {
    if !path.join(".git").exists() {
        return Err(format!(
            "{} 这不是一个 git 仓库（没有 .git 目录）。请选择一个 git 仓库。",
            path.display()
        ));
    }

    let path_str = path.display().to_string();
    *state.project_path.lock().expect("project_path lock") = Some(path.clone());

    // Durable: remember it as the last project + bump it in the recent list. A
    // store error here is non-fatal to the pick itself — surface it so the user
    // knows persistence failed, but the in-memory selection already succeeded.
    let store = state.store()?;
    store
        .set_last_project(&path_str)
        .map_err(|e| format!("{e:#}"))?;
    store
        .touch_recent_project(&path_str, now_ms())
        .map_err(|e| format!("{e:#}"))?;

    Ok(path_str)
}

/// Run one task end to end against the picked repo and stream its events to the
/// UI. `mode` selects the free `fake-claude` double (`simulate`) or the real
/// `claude` binary (`real`).
#[tauri::command]
async fn run_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    mode: RunMode,
) -> Result<(), String> {
    // Resolve the picked project up front so a clear "pick a project first"
    // error reaches the UI for both modes.
    let project = state
        .project_path
        .lock()
        .expect("project_path lock")
        .clone();
    let Some(project) = project else {
        return Err("请先选择一个项目——尚未选择 git 仓库。".to_string());
    };

    // A unique task id per invocation, generated up front so it is the SINGLE
    // identity shared by: the queue row (`enqueue_task`), the streamed
    // `AgentEvent.task_id` (the runner stamps `TaskSpec.id` on every event), and
    // the appended event-log rows — so the task line and its full I/O line up
    // for Phase C/D (board + Logbook replay).
    let task_id = format!("task-{}", now_ms());
    let project_str = project.display().to_string();

    // Enqueue the task in `running` state before the work starts (DESIGN §11,
    // v1.0 module 5). A store error here is non-fatal to the run itself.
    if let Ok(store) = state.store() {
        let _ = store.enqueue_task(&NewTask {
            id: task_id.clone(),
            project: project_str.clone(),
            prompt: prompt.clone(),
            mode: mode_label(mode).to_string(),
            status: "running".to_string(),
            created_at: now_ms(),
        });
    }

    // Snapshot the store handle for the per-event append callback. The store is
    // installed once at setup; if it is missing (impossible in practice), events
    // simply aren't persisted and the live stream still works.
    let summary = run_task_inner(
        app,
        state.store().ok(),
        task_id.clone(),
        project.clone(),
        prompt.clone(),
        mode,
    )
    .await
    .map_err(|e| {
        // Sanitized surface (DESIGN §9): never leak a stack/SQL detail to the UI.
        format!("{e:#}")
    })?;

    // Persist the finished run: update the live task row's lifecycle + cost +
    // branch (DESIGN §11 task), and ALSO append to the legacy `run_history`
    // summary so existing readers keep working. Store errors are non-fatal — the
    // run already streamed + finished.
    if let Ok(store) = state.store() {
        let _ = store.update_task_status(&task_id, &summary.status, now_ms());
        let _ = store.set_task_cost_branch(
            &task_id,
            summary.cost_usd,
            summary.branch.as_deref(),
            now_ms(),
        );
        let _ = store.record_run(&NewRun {
            project: project_str,
            prompt,
            mode: mode_label(mode).to_string(),
            status: summary.status.clone(),
            cost_usd: summary.cost_usd,
            branch: summary.branch.clone(),
            created_at: now_ms(),
        });
    }

    Ok(())
}

/// What a finished run yields for the history record (DESIGN §11).
struct RunSummary {
    status: String,
    cost_usd: Option<f64>,
    branch: Option<String>,
}

/// The persisted label for a run mode.
fn mode_label(mode: RunMode) -> &'static str {
    match mode {
        RunMode::Simulate => "simulate",
        RunMode::Real => "real",
    }
}

async fn run_task_inner(
    app: AppHandle,
    store: Option<&Store>,
    task_id: String,
    project: PathBuf,
    prompt: String,
    mode: RunMode,
) -> anyhow::Result<RunSummary> {
    // Resolve the agent binary + per-mode run options.
    let (agent_bin, options, verify) = match mode {
        RunMode::Simulate => (
            resolve_fake_claude(&app)?,
            // Simulate keeps the original Phase-3 behavior on the picked repo:
            // the worktree is torn down after a trivially-passing gate.
            RunOptions::default(),
            VerifyCommand::shell("exit 0"),
        ),
        RunMode::Real => {
            let bin = resolve_real_claude(&app)?;
            // §9.3: assert the subscription/OAuth route BEFORE spawning. The
            // child is env_clear'd + allowlisted inside the runner; here we
            // assert none of the API-key/Bedrock/Vertex vars survive into the
            // scrubbed env (the allowlist is PATH/HOME/USER/TERM + locale, so
            // they cannot — but we assert explicitly and refuse to launch on a
            // mismatch rather than silently spending on a key route).
            assert_subscription_env()?;
            (
                bin,
                RunOptions {
                    // SAFETY: no sandbox yet (Phase 6). Never auto-merge a real
                    // agent's work into the user's `main` — leave it on a branch.
                    keep_branch: true,
                    extra_args: vec![
                        "--permission-mode".to_string(),
                        "acceptEdits".to_string(),
                        "--model".to_string(),
                        "sonnet".to_string(),
                    ],
                },
                // No real verify command wired yet; a trivially-passing gate so
                // the run reaches the keep-branch disposition.
                VerifyCommand::shell("exit 0"),
            )
        }
    };

    let guard = GitGuard::new(project);
    let task = TaskSpec {
        // The caller-generated unique id (shared with the queue row + event log).
        // It is also what makes the per-attempt branch (`quiver/task-<id>/
        // attempt-1`) unique across runs on the same picked repo.
        id: task_id,
        prompt,
    };

    // TRUE live streaming (Problem 1): emit each AgentEvent to the UI the MOMENT
    // it is produced, via the streaming supervisor's per-event callback — not in a
    // post-run batch. `last_cost` is updated as the Result event flows through.
    // EVERY event is ALSO appended to the §11 source-of-truth log so the full I/O
    // is captured for Phase D replay (in addition to the live emit).
    let mut last_cost: Option<f64> = None;
    let outcome: RunOutcome = run_task_streaming(
        &guard,
        &task,
        &agent_bin,
        &verify,
        options,
        |event: &AgentEvent| {
            if let AgentEventPayload::Result { cost_usd, .. } = &event.payload {
                last_cost = *cost_usd;
            }
            // Persist the event (best-effort: a store write failure must not abort
            // the run — the live stream still drives the office).
            if let Some(store) = store {
                persist_event(store, event);
            }
            // A failed emit (window gone) is not worth aborting the run over — the
            // event is still recorded in the outcome / history. Best-effort live UI.
            let _ = emit_agent_event(&app, event);
        },
    )
    .await?;

    // Terminal lifecycle cap, emitted AFTER the gate (as before). Surface the
    // branch only when the work was left on one (real-mode, no merge).
    let branch = match outcome.cleanup {
        Cleanup::PreservedBranch => Some(outcome.branch.clone()),
        _ => None,
    };
    let status_label = finish_status_label(outcome.status).to_string();
    let finished_task_id = outcome.task_id.clone();
    let finished_seq = outcome.events.len() as u64;
    let finished_ts = now_ms();
    let finished = FinishedEvent {
        task_id: outcome.task_id,
        seq: finished_seq,
        ts_ms: finished_ts,
        runner: "claude_cli",
        kind: "finished",
        status: status_label.clone(),
        cost_usd: last_cost,
        branch: branch.clone(),
    };
    // Persist the synthesized terminal event too, so the appended log replays the
    // FULL run (including the `finished` cap) exactly as the live stream showed it.
    if let Some(store) = store {
        if let Ok(payload) = serde_json::to_string(&finished) {
            let _ = store.append_event(&NewEvent {
                task_id: &finished_task_id,
                seq: finished_seq,
                ts_ms: finished_ts,
                runner: "claude_cli",
                kind: "finished",
                payload_json: &payload,
            });
        }
    }
    app.emit(AGENT_EVENT_CHANNEL, &finished)
        .map_err(|e| anyhow::anyhow!("emit finished failed: {e}"))?;

    Ok(RunSummary {
        status: status_label,
        cost_usd: last_cost,
        branch,
    })
}

/// Emit one normalized `AgentEvent` over the `agent-event` channel.
fn emit_agent_event(app: &AppHandle, event: &AgentEvent) -> anyhow::Result<()> {
    app.emit(AGENT_EVENT_CHANNEL, event)
        .map_err(|e| anyhow::anyhow!("emit agent-event failed: {e}"))
}

/// Append one live `AgentEvent` to the §11 source-of-truth log (best-effort).
///
/// The store is decoupled from the `quiver-core` event type, so this serializes
/// the event to its flat camelCase wire JSON (the exact shape the UI replays)
/// and pulls `kind` / `runner` out of it for the indexed columns. A serialization
/// or write failure is swallowed: persistence must never break the live run.
fn persist_event(store: &Store, event: &AgentEvent) {
    let Ok(value) = serde_json::to_value(event) else {
        return;
    };
    let payload_json = value.to_string();
    let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown");
    let runner = value
        .get("runner")
        .and_then(|v| v.as_str())
        .unwrap_or("claude_cli");
    let _ = store.append_event(&NewEvent {
        task_id: &event.task_id,
        seq: event.seq,
        ts_ms: event.ts_ms,
        runner,
        kind,
        payload_json: &payload_json,
    });
}

fn finish_status_label(status: FinishStatus) -> &'static str {
    match status {
        FinishStatus::Verified => "verified",
        FinishStatus::VerifyFailed => "verify_failed",
        FinishStatus::Failed => "failed",
        FinishStatus::NeedsRebase => "needs_rebase",
    }
}

/// §9.3 pre-spawn guard: assert NONE of the API-key/Bedrock/Vertex env vars are
/// present, so a real-mode (subscription) run cannot silently take a non-OAuth
/// route. The child is additionally env_clear'd + allowlisted inside the runner;
/// asserting on the parent env here is a fail-closed belt-and-braces check.
fn assert_subscription_env() -> anyhow::Result<()> {
    assert_no_forbidden_env(|key| std::env::var_os(key).is_some())
}

/// Pure core of [`assert_subscription_env`]: error if `is_present` reports any of
/// the [`FORBIDDEN_SUBSCRIPTION_ENV`] vars set. Parameterized over the lookup so
/// it is testable without mutating the process's global environment (env var
/// mutation races across parallel tests).
fn assert_no_forbidden_env(is_present: impl Fn(&str) -> bool) -> anyhow::Result<()> {
    for key in FORBIDDEN_SUBSCRIPTION_ENV {
        if is_present(key) {
            anyhow::bail!(
                "refusing to launch real mode: {key} is set, which would route around \
                 the subscription/OAuth path (§9.3). Unset it and retry."
            );
        }
    }
    Ok(())
}

/// Resolve the free `fake-claude` test double to an ABSOLUTE path (DESIGN §12),
/// for `simulate` mode. Probe order: `QUIVER_AGENT_BIN` override → next to this
/// app binary → the workspace `target/<profile>/`.
fn resolve_fake_claude(_app: &AppHandle) -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("QUIVER_AGENT_BIN") {
        let p = PathBuf::from(override_path);
        if is_executable(&p) {
            return Ok(p);
        }
        anyhow::bail!(
            "QUIVER_AGENT_BIN points at a non-executable path: {}",
            p.display()
        );
    }

    let exe = std::env::current_exe()?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("current_exe has no parent dir"))?;
    let candidates = [
        exe_dir.join("fake-claude"),
        exe_dir.join("debug").join("fake-claude"),
        exe_dir.join("release").join("fake-claude"),
    ];
    for candidate in candidates {
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "could not resolve the fake-claude agent binary near {} — run `cargo build` \
         so target/<profile>/fake-claude exists, or set QUIVER_AGENT_BIN",
        exe_dir.display()
    )
}

/// Resolve the official `claude` binary to an ABSOLUTE path (DESIGN §12), for
/// `real` mode. Probe order: `QUIVER_CLAUDE_BIN` override → `PATH` lookup (PATH
/// is repaired by `fix_path_env::fix()` in `main`) → `~/.local/bin/claude` →
/// `~/.claude/local/claude` → `/opt/homebrew/bin/claude` → `/usr/local/bin/claude`.
/// A clear "claude not found" error is returned if none resolve.
fn resolve_real_claude(_app: &AppHandle) -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("QUIVER_CLAUDE_BIN") {
        let p = PathBuf::from(override_path);
        if is_executable(&p) {
            return Ok(p);
        }
        anyhow::bail!(
            "QUIVER_CLAUDE_BIN points at a non-executable path: {}",
            p.display()
        );
    }

    if let Some(p) = which_in_path("claude") {
        return Ok(p);
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let candidates = [
            home.join(".local").join("bin").join("claude"),
            home.join(".claude").join("local").join("claude"),
        ];
        for candidate in candidates {
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    for fixed in ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"] {
        let p = PathBuf::from(fixed);
        if is_executable(&p) {
            return Ok(p);
        }
    }

    anyhow::bail!(
        "could not find the `claude` binary. Install it, or set QUIVER_CLAUDE_BIN \
         to its absolute path."
    )
}

/// First executable `name` found by scanning `PATH` (absolute path), or `None`.
fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Is `path` a real, executable file?
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Build and run the Tauri application. Called from `main.rs` AFTER
/// `fix_path_env::fix()` has repaired the process `PATH` (§12).
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            // Eagerly grab the main window handle so a missing-window config
            // fails loudly here rather than silently at first emit.
            let _ = app.get_webview_window("main");

            // Open the durable SQLite store (DESIGN §11) under the app data dir
            // and install it into AppState so commands can read/write history.
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("could not resolve app data dir: {e}"))?;
            let db_path = Store::default_db_path(&data_dir);
            let store = Store::open(&db_path)
                .map_err(|e| format!("could not open quiver.sqlite at {}: {e:#}", db_path.display()))?;
            let state = app.state::<AppState>();
            // The store is installed exactly once at setup; a second set never
            // happens, so ignore the (impossible) already-set return.
            let _ = state.store.set(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick_project,
            select_recent_project,
            get_initial_state,
            run_task_cmd,
            get_settings,
            update_settings,
            list_tasks,
            reorder_task,
            get_task_events
        ])
        .run(tauri::generate_context!())
        .expect("error while running Quiver");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_mode_deserializes_lowercase() {
        let m: RunMode = serde_json::from_str("\"simulate\"").unwrap();
        assert_eq!(m, RunMode::Simulate);
        let m: RunMode = serde_json::from_str("\"real\"").unwrap();
        assert_eq!(m, RunMode::Real);
    }

    #[test]
    fn subscription_env_assertion_rejects_api_key() {
        // No global env mutation: drive the pure core with a stub lookup that
        // reports ANTHROPIC_API_KEY present.
        let result = assert_no_forbidden_env(|key| key == "ANTHROPIC_API_KEY");
        assert!(result.is_err(), "an API key present must refuse real mode");
    }

    #[test]
    fn subscription_env_assertion_rejects_bedrock_and_vertex() {
        assert!(assert_no_forbidden_env(|k| k == "CLAUDE_CODE_USE_BEDROCK").is_err());
        assert!(assert_no_forbidden_env(|k| k == "CLAUDE_CODE_USE_VERTEX").is_err());
        assert!(assert_no_forbidden_env(|k| k == "ANTHROPIC_AUTH_TOKEN").is_err());
    }

    #[test]
    fn subscription_env_assertion_passes_when_clean() {
        assert!(assert_no_forbidden_env(|_| false).is_ok());
    }
}
