//! Per-task supervision: run one agent in an isolated worktree and clean up
//! (DESIGN §8, Phase 1 subset).
//!
//! `run_task` ties together the Phase-0 runner and the §6 worktree subsystem:
//! create a worktree → spawn the agent in it → drain its [`AgentEvent`] stream →
//! emit a terminal [`FinishStatus`] → tear the worktree down. A successful run
//! is removed under the §6.3 dirty-tree guard; a permanently-failed run is
//! deterministically GC'd (§8.4: `worktree prune` + `remove --force`) and a
//! classified [`FailureReason`] is recorded, so a crash never leaks an orphan
//! worktree or a stale lock.
//!
//! The verify-gate that distinguishes `Verified` from `VerifyFailed` is Phase 2;
//! for now `FinishStatus` is a stub derived from whether the run produced a
//! successful `Result`.

use std::path::Path;

use crate::event::{AgentEvent, AgentEventPayload};
use crate::git::{GitGuard, RemoveOutcome};
use crate::runner::AgentRunner;
use crate::runner::claude::ClaudeRunner;

/// The minimal task description Phase 1 needs.
#[derive(Clone, Debug)]
pub struct TaskSpec {
    pub id: String,
    pub prompt: String,
}

/// Terminal lifecycle status of a run (DESIGN §5.2 `FinishStatus`).
///
/// Phase 1 stub: the real verify-gate (§7) lands in Phase 2 and will introduce
/// `Verified` vs `VerifyFailed`. For now a run that produced `Result{ok:true}`
/// is `Verified`, anything else is `Failed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishStatus {
    Verified,
    Failed,
}

/// Why a run was classified a permanent failure (DESIGN §8.4 record).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureReason {
    /// The agent binary could not be spawned (e.g. a bogus path).
    SpawnFailed,
    /// The agent emitted a terminal `Error` event mid-run.
    AgentCrashed,
    /// The stream ended with no successful `Result` and no explicit error
    /// (e.g. a non-zero exit with a truncated stream).
    NoResult,
}

/// How the worktree teardown actually went.
///
/// A successful run is removed under the §6.3 dirty-tree guard (so a dirty tree
/// is preserved). A failed run is the deliberate §8.4 abandon — force-removed
/// regardless of dirtiness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cleanup {
    /// Clean worktree removed (§6.3).
    Removed,
    /// Dirty worktree preserved, not removed (§6.3); carries the porcelain status.
    PreservedDirty { status: String },
    /// Failed attempt deterministically force-removed (§8.4).
    ForcedRemoved,
}

/// What `run_task` produced: the ordered events, the terminal status, the
/// teardown outcome, and (on failure) the classified reason.
#[derive(Debug)]
pub struct RunOutcome {
    pub task_id: String,
    pub status: FinishStatus,
    pub events: Vec<AgentEvent>,
    pub cleanup: Cleanup,
    /// `Some` iff `status == Failed`.
    pub failure: Option<FailureReason>,
}

/// Run one task to completion in an isolated worktree (DESIGN Phase 1, Tasks
/// 1.4 + 1.5).
///
/// Steps: (1) disable auto-gc + create the attempt-1 worktree behind the §6
/// metadata lock; (2) spawn the agent (`ClaudeRunner` pointed at `runner_bin`)
/// with cwd = the worktree; (3) drain the normalized event stream; (4) classify
/// the outcome; (5a) success → remove the worktree under the §6.3 dirty guard;
/// (5b) permanent failure → deterministic §8.4 GC (`prune` + `remove --force`)
/// and a recorded [`FailureReason`].
///
/// A spawn failure after the worktree exists still routes through the §8.4 GC,
/// so the worktree is never leaked.
///
/// Takes a shared [`GitGuard`] (not a bare repo path) because §6 mandates ONE
/// metadata mutex shared across all workers of a run — constructing a fresh
/// guard per task would defeat the serialization. The guard already carries the
/// repo root.
pub async fn run_task(
    guard: &GitGuard,
    task: &TaskSpec,
    runner_bin: &Path,
) -> anyhow::Result<RunOutcome> {
    const ATTEMPT: u32 = 1;

    // Harden the shared object store for the run (§6.3).
    guard.disable_auto_gc().await?;

    // (1) Isolated worktree on a unique per-attempt branch.
    let worktree = guard.create(&task.id, ATTEMPT).await?;

    // (2) Spawn the agent in the worktree. A spawn failure here must still GC
    // the worktree we just created (§8.4) — never leak it.
    let runner = ClaudeRunner::new(task.id.clone());
    let mut rx = match runner
        .spawn(&task.prompt, worktree.as_path(), runner_bin)
        .await
    {
        Ok(rx) => rx,
        Err(_spawn_err) => {
            guard.force_remove(&worktree).await?;
            return Ok(RunOutcome {
                task_id: task.id.clone(),
                status: FinishStatus::Failed,
                events: Vec::new(),
                cleanup: Cleanup::ForcedRemoved,
                failure: Some(FailureReason::SpawnFailed),
            });
        }
    };

    // (3) Drain the normalized event stream.
    let mut events = Vec::new();
    let mut saw_result_ok = false;
    let mut saw_error = false;
    while let Some(event) = rx.recv().await {
        match event.payload {
            AgentEventPayload::Result { ok: true, .. } => saw_result_ok = true,
            AgentEventPayload::Error { .. } => saw_error = true,
            _ => {}
        }
        events.push(event);
    }

    // (4) Classify (stub verify-gate; real gate is Phase 2).
    if saw_result_ok {
        // (5a) Success → gentle, dirty-guarded teardown (§6.3).
        let cleanup = match guard.remove(&worktree).await? {
            RemoveOutcome::Removed => Cleanup::Removed,
            RemoveOutcome::PreservedDirty { status } => Cleanup::PreservedDirty { status },
        };
        return Ok(RunOutcome {
            task_id: task.id.clone(),
            status: FinishStatus::Verified,
            events,
            cleanup,
            failure: None,
        });
    }

    // (5b) Permanent failure → deterministic §8.4 GC + classified reason.
    let failure = if saw_error {
        FailureReason::AgentCrashed
    } else {
        FailureReason::NoResult
    };
    guard.force_remove(&worktree).await?;
    Ok(RunOutcome {
        task_id: task.id.clone(),
        status: FinishStatus::Failed,
        events,
        cleanup: Cleanup::ForcedRemoved,
        failure: Some(failure),
    })
}
