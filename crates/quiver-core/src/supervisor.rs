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
use crate::git::{attempt_branch, GitGuard, RemoveOutcome};
use crate::runner::AgentRunner;
use crate::runner::claude::ClaudeRunner;
use crate::verify::{VerifyCommand, VerifyResult};

/// The minimal task description Phase 1 needs.
#[derive(Clone, Debug)]
pub struct TaskSpec {
    pub id: String,
    pub prompt: String,
}

/// Knobs that change what `run_task` does with a *successful* run's worktree.
///
/// The default (`keep_branch: false`) is the original Phase-1/2 behavior: a clean
/// worktree is removed after the verify-gate (and, in the full pipeline, the
/// branch would be merged into `main`). `keep_branch: true` is the SAFETY mode
/// for running a real agent on the user's own repo with NO sandbox yet (Phase 6):
/// the agent's work is left on its attempt branch + worktree, untouched, and is
/// NEVER merged into the user's `main`. The branch name is reported so the UI can
/// tell the user where to find the work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Keep the worktree + attempt branch on a successful run instead of removing
    /// it. Set for real-agent runs so the user's `main` is never auto-touched.
    pub keep_branch: bool,
    /// Extra CLI args forwarded verbatim to the agent binary (e.g.
    /// `--permission-mode acceptEdits --model sonnet` for a real run). Ignored by
    /// the `fake-claude` test double.
    pub extra_args: Vec<String>,
}

/// Terminal lifecycle status of a run (DESIGN §5.2, §7 `FinishStatus`).
///
/// The verify-gate (§7) distinguishes `Verified` from `VerifyFailed`: a run is
/// `Verified` only if the agent produced `Result{ok:true}` AND the verify
/// command passed in the worktree. `Failed` is a genuine run failure (crash /
/// spawn failure / no clean result). `NeedsRebase` is a probe-detected conflict
/// or a red re-verify against merged `main` — surfaced for human review, NEVER
/// auto-resolved (§7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishStatus {
    Verified,
    VerifyFailed,
    Failed,
    NeedsRebase,
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
    /// The configured verify command could not be spawned (e.g. a bogus path).
    /// A misconfigured gate is a hard error distinct from a red test (§7), but
    /// it is still a *task* outcome — not a supervisor crash — so the worktree
    /// is §8.4 GC'd and the run is recorded `Failed`, never leaked.
    VerifyError,
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
    /// Deliberately kept by `RunOptions::keep_branch` (real-agent safety mode):
    /// the worktree + attempt branch are left in place, NOT merged into `main`.
    PreservedBranch,
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
    /// The attempt branch the agent's work lives on (`quiver/task-<id>/attempt-N`).
    /// Always populated; load-bearing when `cleanup == PreservedBranch` so the UI
    /// can point the user at the un-merged work.
    pub branch: String,
}

/// Run one task to completion in an isolated worktree (DESIGN Phase 1 Tasks
/// 1.4 + 1.5, Phase 2 Task 2.1 verify-gate).
///
/// Steps: (1) disable auto-gc + create the attempt-1 worktree behind the §6
/// metadata lock; (2) spawn the agent (`ClaudeRunner` pointed at `runner_bin`)
/// with cwd = the worktree; (3) drain the normalized event stream; (4) classify
/// the agent outcome; (4b) if the agent produced `Result{ok:true}`, run the
/// configurable verify-gate (§7) in the worktree — a run is `Verified` only if
/// the agent succeeded AND the gate passes, otherwise `VerifyFailed`;
/// (5a) success → remove the worktree under the §6.3 dirty guard; (5b) permanent
/// failure → deterministic §8.4 GC (`prune` + `remove --force`) and a recorded
/// [`FailureReason`].
///
/// A `VerifyFailed` run is the §8.1 "ran fine, tests red" case — no restart, no
/// failure reason; its worktree is torn down under the same dirty guard as a
/// success. A spawn failure after the worktree exists still routes through the
/// §8.4 GC, so the worktree is never leaked.
///
/// Takes a shared [`GitGuard`] (not a bare repo path) because §6 mandates ONE
/// metadata mutex shared across all workers of a run — constructing a fresh
/// guard per task would defeat the serialization. The guard already carries the
/// repo root.
pub async fn run_task(
    guard: &GitGuard,
    task: &TaskSpec,
    runner_bin: &Path,
    verify: &VerifyCommand,
) -> anyhow::Result<RunOutcome> {
    run_task_with_options(guard, task, runner_bin, verify, RunOptions::default()).await
}

/// `run_task` with explicit [`RunOptions`].
///
/// Behaves identically to [`run_task`] except for the disposition of a
/// *successful* run's worktree: with `options.keep_branch == true` the verified
/// (or verify-failed) worktree + attempt branch are LEFT IN PLACE
/// ([`Cleanup::PreservedBranch`]) and NEVER merged into `main` — the real-agent
/// safety path (§9: no sandbox yet, so the user's `main` is never auto-touched).
/// Failure paths (spawn fail, crash, verify-tool misconfig) still §8.4 GC the
/// worktree regardless of `keep_branch`: a failed attempt is always abandoned.
pub async fn run_task_with_options(
    guard: &GitGuard,
    task: &TaskSpec,
    runner_bin: &Path,
    verify: &VerifyCommand,
    options: RunOptions,
) -> anyhow::Result<RunOutcome> {
    const ATTEMPT: u32 = 1;
    let branch = attempt_branch(&task.id, ATTEMPT);

    // Harden the shared object store for the run (§6.3).
    guard.disable_auto_gc().await?;

    // (1) Isolated worktree on a unique per-attempt branch.
    let worktree = guard.create(&task.id, ATTEMPT).await?;

    // (2) Spawn the agent in the worktree. A spawn failure here must still GC
    // the worktree we just created (§8.4) — never leak it.
    let runner = ClaudeRunner::new(task.id.clone()).with_extra_args(options.extra_args.iter());
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
                branch,
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

    // (4) Agent ran cleanly → run the verify-gate in its worktree (§7 step 1).
    if saw_result_ok {
        // An un-spawnable verify command is a misconfigured gate (§7 hard error,
        // distinct from a red test) — but still a task outcome, not a supervisor
        // crash. Route it through the §8.4 GC like the spawn-fail path so the
        // worktree is never leaked, and record it as a classified failure.
        let verify_result = match verify.run(worktree.as_path()).await {
            Ok(result) => result,
            Err(_verify_err) => {
                guard.force_remove(&worktree).await?;
                return Ok(RunOutcome {
                    task_id: task.id.clone(),
                    status: FinishStatus::Failed,
                    events,
                    cleanup: Cleanup::ForcedRemoved,
                    failure: Some(FailureReason::VerifyError),
                    branch,
                });
            }
        };
        let verified = verify_result == VerifyResult::Passed;
        let status = if verified {
            FinishStatus::Verified
        } else {
            FinishStatus::VerifyFailed
        };
        // (5a) Worktree disposition. In keep-branch mode (real-agent safety) we
        // leave the work on its branch + worktree untouched and never merge into
        // the user's `main`. Otherwise both Verified and VerifyFailed are "the
        // run completed" — tear the worktree down under the §6.3 dirty guard
        // (never force a dirty tree).
        let cleanup = if options.keep_branch {
            Cleanup::PreservedBranch
        } else {
            match guard.remove(&worktree).await? {
                RemoveOutcome::Removed => Cleanup::Removed,
                RemoveOutcome::PreservedDirty { status } => Cleanup::PreservedDirty { status },
            }
        };
        return Ok(RunOutcome {
            task_id: task.id.clone(),
            status,
            events,
            cleanup,
            failure: None,
            branch,
        });
    }

    // (5b) Permanent failure → deterministic §8.4 GC + classified reason. A
    // failed attempt is always abandoned, even in keep_branch mode.
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
        branch,
    })
}
