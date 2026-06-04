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
use quiver_core::git::{GitGuard, RemoveOutcome};
use quiver_core::supervisor::{run_task, FinishStatus, TaskSpec};
use tempfile::TempDir;

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

    let outcome = run_task(&guard, &task, &fake_claude_bin())
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

    // Stub verify-gate: a Result{ok:true} → Verified.
    assert_eq!(outcome.status, FinishStatus::Verified);

    // Clean worktree → removed, not preserved.
    assert_eq!(outcome.cleanup, RemoveOutcome::Removed);

    // Repo is left clean: no stray quiver/... worktrees beyond the main one.
    let list = git(repo.path(), &["worktree", "list", "--porcelain"]);
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(
        !listing.contains("quiver/"),
        "no quiver worktree should remain:\n{listing}"
    );
    let worktree_count = listing.lines().filter(|l| l.starts_with("worktree ")).count();
    assert_eq!(worktree_count, 1, "only the main worktree should remain:\n{listing}");
}
