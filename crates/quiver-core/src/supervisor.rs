//! Per-task supervision: run one agent in an isolated worktree and clean up
//! (DESIGN §8, Phase 1 subset).
//!
//! `run_task` ties together the Phase-0 runner and the §6 worktree subsystem:
//! create a worktree → spawn the agent in it → drain its [`AgentEvent`] stream →
//! emit a terminal [`FinishStatus`] → tear the worktree down per the §6.3
//! dirty-tree rule. The verify-gate that decides `Verified` vs `VerifyFailed`
//! is Phase 2; for now `FinishStatus` is a stub derived purely from whether the
//! run produced a successful `Result`.

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

/// What `run_task` produced: the ordered events, the terminal status, and how
/// the worktree teardown went (a dirty tree is preserved, not removed; §6.3).
#[derive(Debug)]
pub struct RunOutcome {
    pub task_id: String,
    pub status: FinishStatus,
    pub events: Vec<AgentEvent>,
    pub cleanup: RemoveOutcome,
}

/// Run one task to completion in an isolated worktree (DESIGN Phase 1, Task 1.4).
///
/// Steps: (1) disable auto-gc + create the attempt-1 worktree behind the §6
/// metadata lock; (2) spawn the agent (`ClaudeRunner` pointed at `runner_bin`)
/// with cwd = the worktree; (3) drain the normalized event stream; (4) derive a
/// stub [`FinishStatus`]; (5) remove the worktree (clean → removed; dirty →
/// preserved per §6.3).
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

    // (2)+(3) Spawn the agent in the worktree and drain its events.
    let runner = ClaudeRunner::new(task.id.clone());
    let mut rx = runner
        .spawn(&task.prompt, worktree.as_path(), runner_bin)
        .await?;

    let mut events = Vec::new();
    let mut saw_result_ok = false;
    while let Some(event) = rx.recv().await {
        if let AgentEventPayload::Result { ok: true, .. } = event.payload {
            saw_result_ok = true;
        }
        events.push(event);
    }

    // (4) Stub terminal status (real verify-gate is Phase 2).
    let status = if saw_result_ok {
        FinishStatus::Verified
    } else {
        FinishStatus::Failed
    };

    // (5) Tear down: clean → removed; dirty → preserved + surfaced (§6.3).
    let cleanup = guard.remove(&worktree).await?;

    Ok(RunOutcome {
        task_id: task.id.clone(),
        status,
        events,
        cleanup,
    })
}
