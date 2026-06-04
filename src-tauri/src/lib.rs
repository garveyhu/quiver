//! Quiver Tauri app crate (DESIGN §3 three-layer architecture).
//!
//! This is the Rust "core" half of the Tauri app. It owns the Tauri runtime,
//! the IPC surface the React/Phaser UI talks to, and the resolution of the agent
//! binary's absolute path (§12). The actual supervisor logic — worktrees, the
//! verify-gate, merge — lives in the pure-Rust `quiver-core` crate; this crate
//! only wires it to the window.
//!
//! Phase 3 wires exactly ONE command, `run_demo_task`, which proves the whole
//! seam end to end against the free, deterministic `fake-claude` binary:
//! temp repo → worktree → run → verify → stream `AgentEvent`s to the UI.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use quiver_core::event::AgentEvent;
use quiver_core::git::GitGuard;
use quiver_core::supervisor::{run_task, FinishStatus, RunOutcome, TaskSpec};
use quiver_core::verify::VerifyCommand;

/// The Tauri event channel the UI subscribes to (see `useSupervisor.ts`).
const AGENT_EVENT_CHANNEL: &str = "agent-event";

/// A terminal event synthesized AFTER `run_task` returns, carrying the §5.2
/// `FinishStatus` and the run's summed cost. `run_task` owns the whole event
/// stream and returns it as `outcome.events`; it does not expose a per-event
/// callback, so Phase 3 emits the collected events (instant & deterministic with
/// `fake-claude`) and then this lifecycle cap. It is NOT a `quiver-core`
/// `AgentEvent` variant — it mirrors the wire envelope so the UI can render it in
/// the same list (matched by `kind: "finished"` in `agentEvent.types.ts`).
///
/// TODO(phase-4): when the supervisor grows a live per-event sink (a coalesced
/// emitter / Tauri Channel per DESIGN §3), emit each event as it arrives instead
/// of after the run, and emit the real terminal `Finished{status}` from the core.
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
}

/// Run one demo task end to end and stream its events to the UI.
///
/// Steps: (1) create a throwaway temp git repo with an initial commit; (2) build
/// a `TaskSpec`; (3) resolve the agent binary to an ABSOLUTE path (§12; defaults
/// to the built `fake-claude`); (4) `run_task` with a trivially-passing verify
/// gate; (5) emit each collected `AgentEvent` over `agent-event`; (6) emit a
/// synthesized `finished` event; (7) drop the temp dir (cleanup).
#[tauri::command]
async fn run_demo_task(app: AppHandle, prompt: String) -> Result<(), String> {
    run_demo_task_inner(app, prompt).await.map_err(|e| {
        // Sanitized surface (DESIGN §9): never leak a stack/SQL detail to the UI.
        // The full chain stays in the (eventual) log; the UI gets the message.
        format!("{e:#}")
    })
}

async fn run_demo_task_inner(app: AppHandle, prompt: String) -> anyhow::Result<()> {
    let agent_bin = resolve_agent_bin(&app)?;

    // (1) Throwaway repo with an initial commit, so worktree ops have a base ref.
    let repo = tempfile::tempdir()?;
    init_git_repo(repo.path())?;

    // (2) + (3): minimal spec + a shared GitGuard whose worktrees live under the
    // temp repo's own .quiver dir (cleaned up with the tempdir).
    let task = TaskSpec {
        id: "demo".to_string(),
        prompt,
    };
    let guard = GitGuard::new(repo.path());

    // (4) Trivially-passing gate — Phase 3 proves the seam, not a real build.
    let verify = VerifyCommand::shell("exit 0");

    let outcome: RunOutcome = run_task(&guard, &task, &agent_bin, &verify).await?;

    // (5) Stream each collected AgentEvent to the UI.
    let mut last_cost: Option<f64> = None;
    for event in &outcome.events {
        if let quiver_core::event::AgentEventPayload::Result { cost_usd, .. } = &event.payload {
            last_cost = *cost_usd;
        }
        emit_agent_event(&app, event)?;
    }

    // (6) Terminal lifecycle cap carrying the FinishStatus + summed cost.
    let finished = FinishedEvent {
        task_id: outcome.task_id,
        seq: outcome.events.len() as u64,
        ts_ms: now_ms(),
        runner: "claude_cli",
        kind: "finished",
        status: finish_status_label(outcome.status).to_string(),
        cost_usd: last_cost,
    };
    app.emit(AGENT_EVENT_CHANNEL, &finished)
        .map_err(|e| anyhow::anyhow!("emit finished failed: {e}"))?;

    // (7) `repo` drops here → temp dir removed.
    Ok(())
}

/// Emit one normalized `AgentEvent` over the `agent-event` channel.
fn emit_agent_event(app: &AppHandle, event: &AgentEvent) -> anyhow::Result<()> {
    app.emit(AGENT_EVENT_CHANNEL, event)
        .map_err(|e| anyhow::anyhow!("emit agent-event failed: {e}"))
}

fn finish_status_label(status: FinishStatus) -> &'static str {
    match status {
        FinishStatus::Verified => "verified",
        FinishStatus::VerifyFailed => "verify_failed",
        FinishStatus::Failed => "failed",
        FinishStatus::NeedsRebase => "needs_rebase",
    }
}

/// Resolve the agent binary to an ABSOLUTE path (DESIGN §12).
///
/// Probe order: (1) `QUIVER_AGENT_BIN` env override (the configurable hook that
/// will later map to `app_config.claude_path_override`); (2) the built
/// `fake-claude` sitting next to this app binary (the dev/CI default); (3) common
/// `fake-claude` locations in the workspace `target/` dir.
///
/// TODO(phase-4): for a real run, switch the default to resolving the official
/// `claude` binary per §12 (override → `~/.claude/local` → `/opt/homebrew/bin` →
/// `/usr/local/bin`), assert the OAuth route (§9.3), and surface a clear
/// "claude not found" error with a one-click override instead of a spawn failure.
fn resolve_agent_bin(_app: &AppHandle) -> anyhow::Result<PathBuf> {
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

    // The app binary's own directory — in `cargo tauri dev` and in a bundle this
    // is the workspace `target/<profile>/`, where `cargo build` also drops
    // `fake-claude`.
    let exe = std::env::current_exe()?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("current_exe has no parent dir"))?;
    let candidates = [
        exe_dir.join(fake_claude_name()),
        exe_dir.join("debug").join(fake_claude_name()),
        exe_dir.join("release").join(fake_claude_name()),
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

fn fake_claude_name() -> &'static str {
    // No `.exe` branch: Quiver is macOS-only (DESIGN §2.2).
    "fake-claude"
}

/// Is `path` a real, executable file?
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Create a git repo with one initial commit at `dir` (a throwaway temp repo for
/// the demo). Uses the `git` CLI to match `quiver-core`'s "no `git2` crate"
/// constraint and isolates user/email config to this repo so it works on a box
/// with no global git identity.
fn init_git_repo(dir: &Path) -> anyhow::Result<()> {
    git(dir, &["init", "-q", "-b", "main"])?;
    git(dir, &["config", "user.email", "demo@quiver.local"])?;
    git(dir, &["config", "user.name", "Quiver Demo"])?;
    std::fs::write(dir.join("README.md"), "# quiver demo repo\n")?;
    git(dir, &["add", "."])?;
    git(dir, &["commit", "-q", "-m", "initial commit"])?;
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
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
        .setup(|app| {
            // Eagerly grab the main window handle so a missing-window config
            // fails loudly here rather than silently at first emit.
            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![run_demo_task])
        .run(tauri::generate_context!())
        .expect("error while running Quiver");
}
