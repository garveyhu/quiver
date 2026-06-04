//! Integration test for `supervisor::run_task` (DESIGN Phase 1, Task 1.4).
//!
//! Drives the full pipeline against the scripted `fake-claude` test double
//! (free, deterministic — never the real, paid `claude`): create a worktree,
//! run the agent in it, drain the normalized events, then clean up. Asserts the
//! ordered `AgentEvent`s come through AND the repo is left clean (worktree
//! removed, no stray `quiver/...` worktrees per `git worktree list`).

use std::path::{Path, PathBuf};
use std::process::Command;

use quiver_core::event::AgentEventPayload;
use quiver_core::git::GitGuard;
use quiver_core::supervisor::{
    run_task, run_task_with_options, Cleanup, FailureReason, FinishStatus, RunOptions, TaskSpec,
};
use quiver_core::verify::VerifyCommand;
use tempfile::TempDir;

/// A trivially-passing verify command for tests that don't exercise the gate.
fn pass() -> VerifyCommand {
    VerifyCommand::shell("exit 0")
}

/// Resolve the sibling `fake-claude` binary from this test's own exe path
/// (`<target>/<profile>/deps/<test>` → `<target>/<profile>/fake-claude`).
fn fake_claude_bin() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    let bin = dir.join(format!("fake-claude{}", std::env::consts::EXE_SUFFIX));
    assert!(
        bin.exists(),
        "fake-claude binary not found at {} — run via `cargo test`",
        bin.display()
    );
    bin
}

/// `ClaudeRunner` does not forward a `--scenario` flag, so to drive the crash
/// scenario through the real `run_task` → spawn path we write a tiny executable
/// shim into `dir` that execs `fake-claude --scenario crash "$@"`. Returns the
/// shim path. (`fake-claude` scans all its args for `--scenario`, so prepending
/// it plus the runner's own flags works.)
fn crash_shim(dir: &Path) -> PathBuf {
    let real = fake_claude_bin();
    let shim = dir.join("fake-claude-crash.sh");
    let script = format!(
        "#!/bin/sh\nexec \"{}\" --scenario crash \"$@\"\n",
        real.display()
    );
    std::fs::write(&shim, script).expect("write shim");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&shim).expect("stat shim").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim, perms).expect("chmod shim");
    }
    shim
}

/// Run a git command in `dir`, asserting success.
fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// A temp git repo with one initial commit.
fn temp_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "test@quiver.local"]);
    git(path, &["config", "user.name", "Quiver Test"]);
    std::fs::write(path.join("README.md"), "# temp repo\n").expect("write readme");
    git(path, &["add", "."]);
    git(path, &["commit", "-q", "-m", "initial commit"]);
    dir
}

#[tokio::test]
async fn run_task_streams_events_and_leaves_repo_clean() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "alpha".to_string(),
        prompt: "do the thing".to_string(),
    };

    let outcome = run_task(&guard, &task, &fake_claude_bin(), &pass())
        .await
        .expect("run_task");

    // Ordered, envelope-stamped events from the fake-claude happy path.
    assert_eq!(outcome.task_id, "alpha");
    for (i, ev) in outcome.events.iter().enumerate() {
        assert_eq!(ev.task_id, "alpha");
        assert_eq!(ev.seq, i as u64);
    }
    let kinds: Vec<&AgentEventPayload> = outcome.events.iter().map(|e| &e.payload).collect();
    assert!(matches!(kinds[0], AgentEventPayload::WorkerStarted { .. }));
    assert!(matches!(kinds[1], AgentEventPayload::ToolUse { .. }));
    assert!(matches!(kinds[2], AgentEventPayload::OutputChunk { .. }));
    assert!(matches!(kinds[3], AgentEventPayload::Result { ok: true, .. }));

    // Verify-gate: Result{ok:true} AND a passing verify command → Verified.
    assert_eq!(outcome.status, FinishStatus::Verified);
    assert!(outcome.failure.is_none());

    // Clean worktree → removed, not preserved.
    assert_eq!(outcome.cleanup, Cleanup::Removed);

    // Repo is left clean: no stray quiver/... worktrees beyond the main one.
    assert_no_quiver_worktree_leak(repo.path());
}

/// TASK 2.1: the agent succeeds (`Result{ok:true}`) but the verify command
/// fails in the worktree → the run is `VerifyFailed`, not `Verified`. It is
/// "ran fine, tests red" (§8.1): no failure reason, worktree torn down cleanly.
#[tokio::test]
async fn run_task_verify_failed_when_gate_command_fails() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "redtests".to_string(),
        prompt: "do the thing".to_string(),
    };

    let outcome = run_task(&guard, &task, &fake_claude_bin(), &VerifyCommand::shell("exit 1"))
        .await
        .expect("run_task");

    assert_eq!(outcome.status, FinishStatus::VerifyFailed);
    assert!(outcome.failure.is_none(), "verify-red is not a run failure");
    assert_eq!(outcome.cleanup, Cleanup::Removed);
    assert_no_quiver_worktree_leak(repo.path());
}

/// TASK 2.1: the agent succeeds AND the verify command passes → `Verified`.
/// (Mirrors the happy-path test but pins the verify-gate's positive case
/// explicitly with a trivially-passing command.)
#[tokio::test]
async fn run_task_verified_when_gate_command_passes() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "greentests".to_string(),
        prompt: "do the thing".to_string(),
    };

    let outcome = run_task(&guard, &task, &fake_claude_bin(), &VerifyCommand::shell("exit 0"))
        .await
        .expect("run_task");

    assert_eq!(outcome.status, FinishStatus::Verified);
    assert!(outcome.failure.is_none());
    assert_eq!(outcome.cleanup, Cleanup::Removed);
    assert_no_quiver_worktree_leak(repo.path());
}

/// A bogus runner binary path: the worktree was created, the spawn fails, and
/// the §8.4 GC must still run so nothing leaks.
#[tokio::test]
async fn run_task_force_cleans_up_on_spawn_failure() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "ghost".to_string(),
        prompt: "never runs".to_string(),
    };
    let bogus = PathBuf::from("/nonexistent/quiver/definitely-not-a-binary");

    let outcome = run_task(&guard, &task, &bogus, &pass())
        .await
        .expect("run_task");

    assert_eq!(outcome.status, FinishStatus::Failed);
    assert_eq!(outcome.failure, Some(FailureReason::SpawnFailed));
    assert_eq!(outcome.cleanup, Cleanup::ForcedRemoved);
    assert_no_quiver_worktree_leak(repo.path());
}

/// The agent succeeds (`Result{ok:true}`) but the configured verify command
/// cannot be spawned (a non-existent binary — `verify.run` returns `Err`, the
/// §7 "misconfigured gate" hard error, distinct from a red test). This must NOT
/// surface as an `Err` from `run_task`: a verify-tool misconfiguration is a task
/// outcome, so the worktree is §8.4 force-GC'd and a terminal `RunOutcome` with
/// `FinishStatus::Failed` + a classified reason is returned. Never a leak.
#[tokio::test]
async fn run_task_force_cleans_up_when_verify_command_cannot_spawn() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "badverify".to_string(),
        prompt: "do the thing".to_string(),
    };
    // The agent runs fine; the verify gate points at a binary that does not
    // exist, so `verify.run(...)` returns Err (spawn failure).
    let unspawnable =
        VerifyCommand::new(["/nonexistent/quiver/definitely-not-a-verify-tool"]).expect("argv");

    let outcome = run_task(&guard, &task, &fake_claude_bin(), &unspawnable)
        .await
        .expect("run_task must not propagate the verify spawn failure as Err");

    // Terminal failure outcome, classified — not a panic, not an Err.
    assert_eq!(outcome.status, FinishStatus::Failed);
    assert_eq!(outcome.failure, Some(FailureReason::VerifyError));
    assert_eq!(outcome.cleanup, Cleanup::ForcedRemoved);

    // No orphan worktree, no stale lock — the §8.4 invariant holds.
    assert_no_quiver_worktree_leak(repo.path());
    assert!(!repo.path().join(".git/index.lock").exists());
}

/// A crashed agent (`fake-claude --scenario crash`: emits init, then exits
/// non-zero with no clean `result`): deterministic GC, no leak, and the outcome
/// carries the classified failure.
#[tokio::test]
async fn run_task_cleans_up_deterministically_on_crash() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "boom".to_string(),
        prompt: "crash please".to_string(),
    };

    let shim_dir = TempDir::new().expect("shim dir");
    let outcome = run_task(&guard, &task, &crash_shim(shim_dir.path()), &pass())
        .await
        .expect("run_task");

    // The init line still produced a WorkerStarted before the crash.
    assert!(matches!(
        outcome.events.first().map(|e| &e.payload),
        Some(AgentEventPayload::WorkerStarted { .. })
    ));
    // No clean result → permanent failure, recorded reason.
    assert_eq!(outcome.status, FinishStatus::Failed);
    assert_eq!(outcome.failure, Some(FailureReason::NoResult));
    assert_eq!(outcome.cleanup, Cleanup::ForcedRemoved);

    // No orphan worktree, no stale lock.
    assert_no_quiver_worktree_leak(repo.path());
    assert!(!repo.path().join(".git/index.lock").exists());
}

/// SAFETY (real-agent mode): with `RunOptions::keep_branch`, a verified run must
/// LEAVE the agent's work on its attempt branch + worktree and NOT merge into
/// `main`. The outcome reports the branch name and `Cleanup::PreservedBranch`,
/// the worktree directory still exists, the attempt branch is still listed, and
/// `main` is byte-identical to before the run.
#[tokio::test]
async fn run_task_keep_branch_preserves_worktree_and_does_not_touch_main() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let main_before = git(repo.path(), &["rev-parse", "main"]);
    let main_before = String::from_utf8_lossy(&main_before.stdout).trim().to_string();

    let task = TaskSpec {
        id: "keepme".to_string(),
        prompt: "do the thing on a branch".to_string(),
    };

    let outcome = run_task_with_options(
        &guard,
        &task,
        &fake_claude_bin(),
        &pass(),
        RunOptions { keep_branch: true },
    )
    .await
    .expect("run_task_with_options");

    // Verified, but the worktree/branch is deliberately preserved (not merged).
    assert_eq!(outcome.status, FinishStatus::Verified);
    assert!(outcome.failure.is_none());
    assert_eq!(outcome.cleanup, Cleanup::PreservedBranch);
    assert_eq!(outcome.branch, "quiver/task-keepme/attempt-1");

    // The branch is still present in the repo (the work is recoverable).
    let branches = git(repo.path(), &["branch", "--list", "quiver/task-keepme/attempt-1"]);
    assert!(
        String::from_utf8_lossy(&branches.stdout).contains("quiver/task-keepme/attempt-1"),
        "attempt branch should be preserved"
    );

    // `main` is byte-identical to before the run — nothing was auto-merged.
    let main_after = git(repo.path(), &["rev-parse", "main"]);
    let main_after = String::from_utf8_lossy(&main_after.stdout).trim().to_string();
    assert_eq!(main_before, main_after, "main must NOT move in keep_branch mode");
}

/// A failed run in keep_branch mode is STILL abandoned (§8.4): keep_branch only
/// preserves *successful* work, never a crashed attempt.
#[tokio::test]
async fn run_task_keep_branch_still_gcs_a_crash() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "keepboom".to_string(),
        prompt: "crash please".to_string(),
    };
    let shim_dir = TempDir::new().expect("shim dir");

    let outcome = run_task_with_options(
        &guard,
        &task,
        &crash_shim(shim_dir.path()),
        &pass(),
        RunOptions { keep_branch: true },
    )
    .await
    .expect("run_task_with_options");

    assert_eq!(outcome.status, FinishStatus::Failed);
    assert_eq!(outcome.failure, Some(FailureReason::NoResult));
    assert_eq!(outcome.cleanup, Cleanup::ForcedRemoved);
    assert_no_quiver_worktree_leak(repo.path());
}

/// The default `run_task` path still reports the attempt branch even though it
/// removes the worktree (the branch field is always populated).
#[tokio::test]
async fn run_task_reports_attempt_branch() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

    let task = TaskSpec {
        id: "named".to_string(),
        prompt: "do the thing".to_string(),
    };

    let outcome = run_task(&guard, &task, &fake_claude_bin(), &pass())
        .await
        .expect("run_task");
    assert_eq!(outcome.branch, "quiver/task-named/attempt-1");
}

/// Assert `git worktree list` shows only the main worktree — no quiver/... leak.
fn assert_no_quiver_worktree_leak(repo: &Path) {
    let list = git(repo, &["worktree", "list", "--porcelain"]);
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(
        !listing.contains("quiver/"),
        "no quiver worktree should remain:\n{listing}"
    );
    let worktree_count = listing.lines().filter(|l| l.starts_with("worktree ")).count();
    assert_eq!(
        worktree_count, 1,
        "only the main worktree should remain:\n{listing}"
    );
}
