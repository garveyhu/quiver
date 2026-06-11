//! 督导的数据契约(§5/§7/§8):任务描述、运行旋钮、终态状态、失败原因、worktree 清理结果、
//! 运行产物。把"数据形"和 mod.rs 的"执行逻辑"分开,各自聚焦、易读。

use crate::event::AgentEvent;

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
    /// When set, resume this backend session (`claude --resume <id>`) instead of
    /// starting fresh — the crash-recovery path (DESIGN §23 P0) passes the task's
    /// persisted `session_id` so a reconciled run continues its prior context.
    /// `None` (the default) → a normal fresh spawn.
    pub resume_session: Option<String>,
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
    /// The verify-gate command's captured output (tail), when it ran. Lets the UI
    /// show *why* a `VerifyFailed` run failed. `None` if the gate never ran (spawn
    /// failure, permanent agent failure).
    pub verify_output: Option<String>,
    /// The worktree HEAD SHA at run end (DESIGN §6.2) — the attempt's commit (real
    /// mode) or the base it branched from (a no-op run). Captured before cleanup so
    /// the episode can bind to its commit. `None` if the worktree never existed
    /// (spawn failure) or the read failed.
    pub commit_sha: Option<String>,
    /// `git diff --shortstat <base>` for the attempt (DESIGN §6.2 — diff size on the
    /// episode). Empty string for a no-op run; `None` if base/diff couldn't be read
    /// or the agent never ran (spawn failure).
    pub diff_stat: Option<String>,
}
