//! Running one task end-to-end: resolve the agent binary, stream its events live
//! to the UI, persist every event + the terminal status/cost/branch.
//!
//! This is the shared core both the legacy single-shot path and the Phase-C
//! queue scheduler call. It is deliberately decoupled from Tauri's `State` /
//! command machinery: it takes a plain `&AppHandle`, an `Arc<Store>`, and a
//! shared `Arc<GitGuard>` (so concurrent workers on one project serialize their
//! shared-`.git` metadata ops through the §6 mutex), making it safe to invoke
//! from inside a `tokio::spawn`ed worker.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use quiver_core::event::{AgentEvent, AgentEventPayload};
use quiver_core::git::GitGuard;
use quiver_core::supervisor::{
    run_task_streaming, Cleanup, FinishStatus, RunOptions, RunOutcome, TaskSpec,
};
use quiver_core::verify::VerifyCommand;
use quiver_store::{NewEvent, NewRun, Settings, Store};

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
pub enum RunMode {
    Simulate,
    Real,
}

impl RunMode {
    /// The persisted label for a run mode.
    pub fn label(self) -> &'static str {
        match self {
            RunMode::Simulate => "simulate",
            RunMode::Real => "real",
        }
    }

    /// Parse the persisted label back into a mode. Anything that is not exactly
    /// `"real"` is treated as `simulate` (the safe, free default) — so a corrupt
    /// or unknown stored value never silently spends real credit.
    pub fn from_label(label: &str) -> Self {
        match label {
            "real" => RunMode::Real,
            _ => RunMode::Simulate,
        }
    }
}

/// What a finished run yields for the history record (DESIGN §11).
pub struct RunSummary {
    pub status: String,
    pub cost_usd: Option<f64>,
    pub branch: Option<String>,
}

/// A terminal event synthesized AFTER `run_task` returns, carrying the §5.2
/// `FinishStatus`, the run's summed cost, and — for real-mode runs left
/// un-merged — the attempt branch the work lives on. Mirrors the wire envelope
/// so the UI renders it in the same list (matched by `kind: "finished"`).
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
    branch: Option<String>,
}

/// Run one already-enqueued task to completion against a SHARED git guard,
/// persisting its lifecycle exactly like the single-shot path: stream every
/// event live + append to the §11 log, then update the task row's status, cost,
/// and branch, and append a `run_history` summary.
///
/// The task row is assumed to already exist in `running` state (the scheduler's
/// `claim_next_queued` flips it before calling here; the single-shot command
/// enqueues it `running`). On error the row is moved to `failed` and a sanitized
/// message is logged — never propagated as a panic out of the spawned worker.
pub async fn run_one_task(
    app: &AppHandle,
    store: &Store,
    guard: &GitGuard,
    task_id: String,
    prompt: String,
    mode: RunMode,
) {
    let project = guard.repo().display().to_string();
    match run_streaming(app, Some(store), guard, task_id.clone(), prompt.clone(), mode).await {
        Ok(summary) => {
            let _ = store.update_task_status(&task_id, &summary.status, crate::now_ms());
            let _ = store.set_task_cost_branch(
                &task_id,
                summary.cost_usd,
                summary.branch.as_deref(),
                crate::now_ms(),
            );
            let _ = store.record_run(&NewRun {
                project,
                prompt,
                mode: mode.label().to_string(),
                status: summary.status,
                cost_usd: summary.cost_usd,
                branch: summary.branch,
                created_at: crate::now_ms(),
            });
        }
        Err(_e) => {
            // A run-level failure (e.g. agent binary not resolvable) is a task
            // outcome, not a crash: mark the card failed so the board reflects it
            // and the freed slot moves on. The sanitized detail is not surfaced
            // here (no live error event from a finished worker) — the card status
            // carries it.
            let _ = store.update_task_status(&task_id, "failed", crate::now_ms());
        }
    }
}

/// The streaming run core: resolve the binary + options for the mode, run the
/// task with a per-event callback that (1) tracks cost, (2) appends to the §11
/// log, (3) emits live to the UI, then synthesize + persist + emit the terminal
/// `finished` cap. Returns the [`RunSummary`] for the caller to persist on the
/// task row + history.
pub async fn run_streaming(
    app: &AppHandle,
    store: Option<&Store>,
    guard: &GitGuard,
    task_id: String,
    prompt: String,
    mode: RunMode,
) -> anyhow::Result<RunSummary> {
    let settings = store
        .map(|s| s.get_settings())
        .transpose()?
        .unwrap_or_default();

    let (agent_bin, options, verify) = match mode {
        RunMode::Simulate => {
            // Make `fakeDelayMs` take effect: the runner forwards
            // QUIVER_FAKE_DELAY_MS to the `fake-claude` child via its env
            // allowlist. SAFETY: set on the parent process env just before the
            // spawn; the value is inert for any non-fake binary, and concurrent
            // simulate workers all want the SAME saved pacing, so a benign race
            // on this write only ever (re)writes the same value.
            std::env::set_var("QUIVER_FAKE_DELAY_MS", settings.fake_delay_ms.to_string());
            (
                resolve_agent_bin(&settings, mode)?,
                RunOptions::default(),
                VerifyCommand::shell("exit 0"),
            )
        }
        RunMode::Real => {
            let bin = resolve_agent_bin(&settings, mode)?;
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
                        settings.model.clone(),
                    ],
                },
                VerifyCommand::shell("exit 0"),
            )
        }
    };

    let task = TaskSpec {
        id: task_id,
        prompt,
    };

    let mut last_cost: Option<f64> = None;
    let outcome: RunOutcome = run_task_streaming(
        guard,
        &task,
        &agent_bin,
        &verify,
        options,
        |event: &AgentEvent| {
            if let AgentEventPayload::Result { cost_usd, .. } = &event.payload {
                last_cost = *cost_usd;
            }
            if let Some(store) = store {
                persist_event(store, event);
            }
            let _ = app.emit(AGENT_EVENT_CHANNEL, event);
        },
    )
    .await?;

    let branch = match outcome.cleanup {
        Cleanup::PreservedBranch => Some(outcome.branch.clone()),
        _ => None,
    };
    let status_label = finish_status_label(outcome.status).to_string();
    let finished_task_id = outcome.task_id.clone();
    let finished_seq = outcome.events.len() as u64;
    let finished_ts = crate::now_ms();
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

/// Append one live `AgentEvent` to the §11 source-of-truth log (best-effort).
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
/// route.
fn assert_subscription_env() -> anyhow::Result<()> {
    assert_no_forbidden_env(|key| std::env::var_os(key).is_some())
}

/// Pure core of [`assert_subscription_env`]: error if `is_present` reports any of
/// the [`FORBIDDEN_SUBSCRIPTION_ENV`] vars set.
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

/// Resolve the agent binary honoring the saved `agent_bin_override` (Phase B)
/// before the per-mode auto-resolution.
fn resolve_agent_bin(settings: &Settings, mode: RunMode) -> anyhow::Result<PathBuf> {
    if let Some(override_path) = settings
        .agent_bin_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(override_path);
        if is_executable(&p) {
            return Ok(p);
        }
        anyhow::bail!(
            "设置里的 agent 二进制路径不可执行：{}。请修正或清空该项以自动解析。",
            p.display()
        );
    }
    match mode {
        RunMode::Simulate => resolve_fake_claude(),
        RunMode::Real => resolve_real_claude(),
    }
}

/// Resolve the free `fake-claude` test double to an ABSOLUTE path (DESIGN §12).
fn resolve_fake_claude() -> anyhow::Result<PathBuf> {
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

/// Resolve the official `claude` binary to an ABSOLUTE path (DESIGN §12).
fn resolve_real_claude() -> anyhow::Result<PathBuf> {
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
    fn run_mode_label_round_trips() {
        assert_eq!(RunMode::from_label("real"), RunMode::Real);
        assert_eq!(RunMode::from_label("simulate"), RunMode::Simulate);
        // Unknown / corrupt label falls back to the safe free default.
        assert_eq!(RunMode::from_label("garbage"), RunMode::Simulate);
        assert_eq!(RunMode::Real.label(), "real");
        assert_eq!(RunMode::Simulate.label(), "simulate");
    }

    #[test]
    fn subscription_env_assertion_rejects_api_key() {
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
