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

/// `run_task_with_options` that ALSO fires `on_event` for each [`AgentEvent`] the
/// moment it arrives from the runner channel — BEFORE the stream is fully
/// drained, the verify-gate runs, or the worktree is torn down.
///
/// This is the live-streaming entry point the Tauri shell uses: the callback
/// pushes each event to the UI as it is produced, so the office animates in real
/// time instead of receiving the whole batch after the run finishes. The events
/// are still collected into [`RunOutcome::events`] (the §11 source of truth), so
/// callers that want the full ordered log keep getting it. [`run_task`] /
/// [`run_task_with_options`] delegate here with a no-op callback.
///
/// `on_event` is called synchronously on the supervisor task as each event is
/// received; keep it cheap (e.g. a single Tauri `emit`). It must be `Send` so the
/// future stays `Send` across the runner's `.await` points.
pub async fn run_task_streaming(
    guard: &GitGuard,
    task: &TaskSpec,
    runner_bin: &Path,
    verify: &VerifyCommand,
    options: RunOptions,
    mut on_event: impl FnMut(&AgentEvent) + Send,
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
            // A non-kept failed attempt leaves no orphan branch either (§8.4).
            guard.delete_branch(&branch).await?;
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

    // (3) Drain the normalized event stream — firing `on_event` LIVE for each
    // event as it arrives, before collecting it for the outcome.
    let mut events = Vec::new();
    let mut saw_result_ok = false;
    let mut saw_error = false;
    while let Some(event) = rx.recv().await {
        match event.payload {
            AgentEventPayload::Result { ok: true, .. } => saw_result_ok = true,
            AgentEventPayload::Error { .. } => saw_error = true,
            _ => {}
        }
        on_event(&event);
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
                guard.delete_branch(&branch).await?;
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
        // (5a) Worktree disposition (see [`run_task_with_options`] doc).
        let cleanup = if options.keep_branch {
            Cleanup::PreservedBranch
        } else {
            match guard.remove(&worktree).await? {
                RemoveOutcome::Removed => {
                    guard.delete_branch(&branch).await?;
                    Cleanup::Removed
                }
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
    guard.delete_branch(&branch).await?;
    Ok(RunOutcome {
        task_id: task.id.clone(),
        status: FinishStatus::Failed,
        events,
        cleanup: Cleanup::ForcedRemoved,
        failure: Some(failure),
        branch,
    })
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
    // Delegate to the streaming variant with a no-op callback: non-streaming
    // callers still get the full collected event log in the outcome, with
    // identical lifecycle/cleanup behavior.
    run_task_streaming(guard, task, runner_bin, verify, options, |_event| {}).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Locate the built `fake-claude` binary. It lives in the same
    /// `target/<profile>/` dir as the test executable (sibling crate in the
    /// workspace), so walk up from the test exe's directory and probe.
    fn fake_claude_bin() -> PathBuf {
        let mut dir = std::env::current_exe().expect("current_exe");
        // current_exe = target/<profile>/deps/<test-hash>; the binary is one or
        // two levels up. Walk up looking for `fake-claude`.
        while dir.pop() {
            let candidate = dir.join("fake-claude");
            if candidate.is_file() {
                return candidate;
            }
        }
        panic!("fake-claude binary not found near the test exe — run `cargo build` first");
    }

    /// A temp git repo with an initial commit on `main`. Returns the kept-alive
    /// temp dir.
    fn temp_repo() -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(path)
                .output()
                .expect("git");
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "test@quiver.local"]);
        run(&["config", "user.name", "Quiver Test"]);
        std::fs::write(path.join("README.md"), "# temp repo\n").expect("write readme");
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial commit"]);
        dir
    }

    /// All local `quiver/*` branches in the repo.
    fn quiver_branches(repo: &Path) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args([
                "-C",
                repo.to_str().unwrap(),
                "branch",
                "--list",
                "quiver/*",
                "--format=%(refname:short)",
            ])
            .output()
            .expect("git branch --list");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Worktree count reported by git (main + any attempt worktrees).
    fn worktree_count(repo: &Path) -> usize {
        let out = std::process::Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "worktree", "list"])
            .output()
            .expect("worktree list");
        String::from_utf8_lossy(&out.stdout).lines().count()
    }

    fn head_sha(repo: &Path) -> String {
        let out = std::process::Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "rev-parse", "main"])
            .output()
            .expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Two consecutive simulate-style runs on the SAME repo BOTH succeed — the
    /// repeat-run bug fix: a unique task id per run + branch deletion on cleanup
    /// means the 2nd `git worktree add` never collides on an existing branch.
    #[tokio::test]
    async fn two_consecutive_simulate_runs_both_succeed_no_branch_litter() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
        let bin = fake_claude_bin();
        let verify = VerifyCommand::shell("exit 0");

        let main_before = head_sha(repo.path());

        // Run 1 — a unique id, as run_task_cmd would generate.
        let task1 = TaskSpec {
            id: "task-1001".to_string(),
            prompt: "first".to_string(),
        };
        let out1 = run_task_with_options(&guard, &task1, &bin, &verify, RunOptions::default())
            .await
            .expect("run 1 ok");
        assert_eq!(out1.status, FinishStatus::Verified);
        assert_eq!(out1.cleanup, Cleanup::Removed);

        // Run 2 — a DIFFERENT unique id. Must also succeed (the old fixed-id bug
        // failed here at `git worktree add`: branch already exists).
        let task2 = TaskSpec {
            id: "task-1002".to_string(),
            prompt: "second".to_string(),
        };
        let out2 = run_task_with_options(&guard, &task2, &bin, &verify, RunOptions::default())
            .await
            .expect("run 2 ok");
        assert_eq!(out2.status, FinishStatus::Verified);
        assert_eq!(out2.cleanup, Cleanup::Removed);

        // No `quiver/*` branch litter, no leftover worktrees, main untouched.
        assert!(
            quiver_branches(repo.path()).is_empty(),
            "non-keep runs must leave NO quiver/* branches, got: {:?}",
            quiver_branches(repo.path())
        );
        assert_eq!(worktree_count(repo.path()), 1, "only main worktree should remain");
        assert_eq!(head_sha(repo.path()), main_before, "main must be byte-identical");
    }

    /// Even reusing the SAME task id across two non-keep runs must not collide:
    /// the first run deletes its branch on cleanup, so the second can recreate it.
    #[tokio::test]
    async fn same_task_id_reused_after_cleanup_succeeds() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
        let bin = fake_claude_bin();
        let verify = VerifyCommand::shell("exit 0");
        let task = TaskSpec {
            id: "samerepeat".to_string(),
            prompt: "p".to_string(),
        };

        let out1 = run_task_with_options(&guard, &task, &bin, &verify, RunOptions::default())
            .await
            .expect("run 1");
        assert_eq!(out1.cleanup, Cleanup::Removed);
        // Branch from run 1 must be gone before run 2 needs to create it.
        assert!(quiver_branches(repo.path()).is_empty());

        let out2 = run_task_with_options(&guard, &task, &bin, &verify, RunOptions::default())
            .await
            .expect("run 2 must not collide on the reused branch name");
        assert_eq!(out2.status, FinishStatus::Verified);
        assert_eq!(out2.cleanup, Cleanup::Removed);
        assert!(quiver_branches(repo.path()).is_empty());
    }

    /// A `keep_branch` run (real mode) PRESERVES its uniquely-named attempt branch
    /// and worktree — intentional, never merged into `main`.
    #[tokio::test]
    async fn keep_branch_run_preserves_its_branch() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
        let bin = fake_claude_bin();
        let verify = VerifyCommand::shell("exit 0");
        let task = TaskSpec {
            id: "kept-7".to_string(),
            prompt: "p".to_string(),
        };

        let out = run_task_with_options(
            &guard,
            &task,
            &bin,
            &verify,
            RunOptions {
                keep_branch: true,
                extra_args: Vec::new(),
            },
        )
        .await
        .expect("keep-branch run");

        assert_eq!(out.cleanup, Cleanup::PreservedBranch);
        let expected = attempt_branch(&task.id, 1);
        assert_eq!(out.branch, expected);
        assert!(
            quiver_branches(repo.path()).contains(&expected),
            "keep_branch must preserve the attempt branch, got: {:?}",
            quiver_branches(repo.path())
        );
    }

    /// The streaming variant fires `on_event` for EACH event AS it arrives —
    /// before the run finishes — and the callback sees the same ordered events
    /// that end up in the outcome. Live-ness: with a per-line delay in
    /// `fake-claude`, the callbacks are spread over wall-clock time rather than
    /// all firing at the end.
    #[tokio::test]
    async fn streaming_fires_callback_per_event_in_order() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
        let bin = fake_claude_bin();
        let verify = VerifyCommand::shell("exit 0");
        let task = TaskSpec {
            id: "stream-1".to_string(),
            prompt: "p".to_string(),
        };

        let seen: std::sync::Arc<std::sync::Mutex<Vec<(u64, std::time::Instant)>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_cb = std::sync::Arc::clone(&seen);
        let start = std::time::Instant::now();

        let out = run_task_streaming(
            &guard,
            &task,
            &bin,
            &verify,
            RunOptions::default(),
            move |ev| {
                seen_cb
                    .lock()
                    .expect("seen lock")
                    .push((ev.seq, std::time::Instant::now()));
            },
        )
        .await
        .expect("streaming run ok");

        let seen = std::sync::Arc::try_unwrap(seen)
            .expect("no other refs")
            .into_inner()
            .expect("seen inner");

        // The callback saw every collected event, in seq order.
        assert_eq!(
            seen.len(),
            out.events.len(),
            "callback must fire once per collected event"
        );
        assert!(seen.len() >= 4, "happy path emits several events");
        let seqs: Vec<u64> = seen.iter().map(|(s, _)| *s).collect();
        let mut sorted = seqs.clone();
        sorted.sort_unstable();
        assert_eq!(seqs, sorted, "events must arrive in seq order");

        // Live-ness: force a per-line delay in fake-claude via the env override so
        // the first and last callbacks are separated in time (not a batch dump).
        // This test sets no delay (CI speed), so we only assert ordering + count;
        // the timed live-ness check lives in the integration test
        // `tests/streaming.rs`, which sets QUIVER_FAKE_DELAY_MS.
        let _ = start;
    }

    /// A crashed (permanently-failed) non-keep run is GC'd: no orphan branch and
    /// no leftover worktree.
    #[tokio::test]
    async fn crashed_run_leaves_no_branch_or_worktree() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
        let bin = fake_claude_bin();
        let verify = VerifyCommand::shell("exit 0");
        let task = TaskSpec {
            id: "boom".to_string(),
            // fake-claude reads `--scenario crash` from extra_args and dies non-zero.
            prompt: "p".to_string(),
        };

        let out = run_task_with_options(
            &guard,
            &task,
            &bin,
            &verify,
            RunOptions {
                keep_branch: false,
                extra_args: vec!["--scenario".to_string(), "crash".to_string()],
            },
        )
        .await
        .expect("crashed run still returns an outcome");

        assert_eq!(out.status, FinishStatus::Failed);
        assert_eq!(out.cleanup, Cleanup::ForcedRemoved);
        assert!(
            quiver_branches(repo.path()).is_empty(),
            "abandoned attempt must leave no quiver/* branch"
        );
        assert_eq!(worktree_count(repo.path()), 1, "no leftover worktree");
    }
}
