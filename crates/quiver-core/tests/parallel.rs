//! Phase 2 Task 2.6: parallel merge serialization + conflict isolation.
//!
//! The §7 promise under real fan-out: many workers create worktrees, run agents,
//! verify, and merge *concurrently*, but merges are serialized by ONE shared
//! `MergeLock` so `main` is never half-merged. This test exercises the whole
//! stack on a temp repo with `fake-claude` as the agent:
//!
//!   * Several `run_task`s run concurrently (sharing one `GitGuard`) — proving
//!     worktree create/run/verify/teardown is safe in parallel and leaves no
//!     leaked worktrees.
//!   * Several diverging branches are merged concurrently through one
//!     `MergeLock` — the non-conflicting ones all land on `main` and `main` ends
//!     consistent; an injected conflicting branch ends `NeedsRebase` and never
//!     corrupts `main`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use quiver_core::git::{attempt_branch, GitGuard};
use quiver_core::merge::{merge_and_reverify, MergeDecision, MergeLock, RebaseReason};
use quiver_core::supervisor::{run_task, FinishStatus, TaskSpec};
use quiver_core::verify::VerifyCommand;
use tempfile::TempDir;

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

fn rev_parse(dir: &Path, refname: &str) -> String {
    let out = git(dir, &["rev-parse", refname]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Resolve the sibling `fake-claude` binary from this test's own exe path.
fn fake_claude_bin() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    let bin = dir.join(format!("fake-claude{}", std::env::consts::EXE_SUFFIX));
    assert!(bin.exists(), "fake-claude not found at {} — run via cargo test", bin.display());
    bin
}

fn temp_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "test@quiver.local"]);
    git(path, &["config", "user.name", "Quiver Test"]);
    std::fs::write(path.join("code.txt"), "line1\nline2\nline3\n").expect("write code");
    git(path, &["add", "."]);
    git(path, &["commit", "-q", "-m", "initial commit"]);
    dir
}

/// `git worktree list --porcelain` worktree count (main + any quiver worktrees).
fn worktree_count(repo: &Path) -> usize {
    let out = git(repo, &["worktree", "list", "--porcelain"]);
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("worktree "))
        .count()
}

fn has_quiver_worktree(repo: &Path) -> bool {
    let out = git(repo, &["worktree", "list", "--porcelain"]);
    String::from_utf8_lossy(&out.stdout).contains("quiver/")
}

/// Seed a task's attempt-1 branch with a distinct committed change, the way a
/// real agent run would leave it before the merge step. Uses the `GitGuard` to
/// create the worktree (serialized §6 metadata op), commits in it, then tears
/// the worktree down — the branch (and its commit) persists for the merge.
async fn seed_branch(guard: &GitGuard, task_id: &str, file: &str, content: &str) {
    let wt = guard.create(task_id, 1).await.expect("create worktree");
    std::fs::write(wt.as_path().join(file), content).expect("write");
    git(wt.as_path(), &["add", file]);
    git(wt.as_path(), &["commit", "-q", "-m", &format!("work for {task_id}")]);
    // Clean teardown — leaves the branch behind for merging.
    guard.remove(&wt).await.expect("remove worktree");
}

/// Several `run_task`s run concurrently against `fake-claude`: all land
/// `Verified`, and no worktree is leaked afterward (only the main worktree).
#[tokio::test]
async fn concurrent_run_tasks_all_verify_and_leak_no_worktrees() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = Arc::new(GitGuard::new(repo.path()).with_worktrees_root(wt_root.path()));
    let bin = fake_claude_bin();
    let verify = VerifyCommand::shell("exit 0");

    let mut handles = Vec::new();
    for id in ["one", "two", "three"] {
        let guard = guard.clone();
        let bin = bin.clone();
        let verify = verify.clone();
        handles.push(tokio::spawn(async move {
            let task = TaskSpec {
                id: id.to_string(),
                prompt: "do the thing".to_string(),
            };
            run_task(&guard, &task, &bin, &verify).await
        }));
    }

    for h in handles {
        let outcome = h.await.expect("join").expect("run_task");
        assert_eq!(outcome.status, FinishStatus::Verified, "task {} should verify", outcome.task_id);
    }

    // Worktrees serialized + cleaned up: only the main worktree remains.
    assert!(!has_quiver_worktree(repo.path()), "no quiver worktree should leak");
    assert_eq!(worktree_count(repo.path()), 1, "only the main worktree should remain");
    assert!(!repo.path().join(".git/index.lock").exists());
}

/// Concurrent merges through ONE `MergeLock`: three non-conflicting branches all
/// land on `main` (serialized, never half-merged) and an injected conflicting
/// branch ends `NeedsRebase` without corrupting `main`. No leaked worktrees.
#[tokio::test]
async fn concurrent_merges_serialize_and_isolate_conflict() {
    let repo = temp_repo();
    let wt_root = TempDir::new().expect("wt root");
    let guard = Arc::new(GitGuard::new(repo.path()).with_worktrees_root(wt_root.path()));
    let merge_lock = Arc::new(MergeLock::new());

    // Three non-conflicting branches (each adds its own distinct file) and one
    // conflicting branch (edits the same line main has). All seeded via the
    // guard's worktree lifecycle.
    seed_branch(&guard, "a", "a.txt", "A\n").await;
    seed_branch(&guard, "b", "b.txt", "B\n").await;
    seed_branch(&guard, "c", "c.txt", "C\n").await;
    seed_branch(&guard, "x", "code.txt", "line1\nBRANCH_X\nline3\n").await;
    // main diverges on the SAME line x edited → x will conflict at the probe.
    std::fs::write(repo.path().join("code.txt"), "line1\nMAIN\nline3\n").expect("write");
    git(repo.path(), &["commit", "-q", "-am", "main diverges on line2"]);

    // The verify command stays green for any combination of the added files —
    // so the only thing that blocks a merge here is a genuine git conflict, and
    // serialization is what keeps concurrent clean merges from racing.
    let verify = VerifyCommand::shell("exit 0");

    // Fire all four merges concurrently; one shared MergeLock serializes them.
    let mut handles = Vec::new();
    for id in ["a", "b", "c", "x"] {
        let guard = guard.clone();
        let merge_lock = merge_lock.clone();
        let verify = verify.clone();
        handles.push(tokio::spawn(async move {
            let branch = attempt_branch(id, 1);
            let report = merge_and_reverify(
                &merge_lock,
                &guard,
                &branch,
                "main",
                &format!("merge task-{id}"),
                &verify,
            )
            .await
            .expect("merge");
            (id, report)
        }));
    }

    let mut decisions = std::collections::HashMap::new();
    for h in handles {
        let (id, report) = h.await.expect("join");
        decisions.insert(id, report);
    }

    // The three non-conflicting tasks all landed.
    for id in ["a", "b", "c"] {
        assert_eq!(decisions[id].decision, MergeDecision::Merged, "task {id} should merge");
    }
    // The conflicting task was isolated, not merged.
    assert_eq!(decisions["x"].decision, MergeDecision::NeedsRebase);
    assert_eq!(decisions["x"].reason, Some(RebaseReason::ProbeConflict));

    // main is consistent: all three non-conflicting files landed, the
    // conflicting branch's content did NOT, and there is no half-merge state.
    git(repo.path(), &["checkout", "-q", "main"]);
    assert!(repo.path().join("a.txt").exists(), "a landed");
    assert!(repo.path().join("b.txt").exists(), "b landed");
    assert!(repo.path().join("c.txt").exists(), "c landed");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("code.txt")).unwrap(),
        "line1\nMAIN\nline3\n",
        "x's conflicting edit must NOT be on main"
    );
    assert!(!repo.path().join(".git/MERGE_HEAD").exists(), "no half-merge left");

    // main's tree is internally consistent (a real commit, fsck-clean refs).
    let _ = rev_parse(repo.path(), "main");
    git(repo.path(), &["fsck", "--no-dangling"]);

    // No leaked worktrees from the seeding lifecycle.
    assert!(!has_quiver_worktree(repo.path()), "no quiver worktree should leak");
    assert_eq!(worktree_count(repo.path()), 1, "only the main worktree should remain");
}
