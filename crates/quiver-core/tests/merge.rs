//! Integration tests for the §7 serialized merge + re-verify pipeline
//! (`merge::merge_and_reverify`, Phase 2 Tasks 2.3 + 2.4 + 2.5).
//!
//! Exercised against real temp git repos (the `git` CLI), no agent involved —
//! the merge protocol is pure git + verify-gate logic. Key cases:
//!   * a normal merge advances `main`;
//!   * the "A+B green alone, red together" case — the second branch's re-verify
//!     against merged `main` goes red and is ABORTED, not left on `main`;
//!   * a conflicting branch ends `NeedsRebase` with `main` byte-identical.

use std::path::Path;
use std::process::Command;

use quiver_core::git::GitGuard;
use quiver_core::merge::{merge_and_reverify, MergeDecision, MergeLock, RebaseReason};
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

/// A temp git repo on `main` with one initial commit (a `code.txt`).
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

/// Create a fresh branch off `main` that adds a brand-new file `name` (a
/// non-conflicting change — different files never collide in git).
fn branch_adding_file(repo: &Path, branch: &str, name: &str) {
    git(repo, &["checkout", "-q", "-b", branch, "main"]);
    std::fs::write(repo.join(name), "x\n").expect("write file");
    git(repo, &["add", name]);
    git(repo, &["commit", "-q", "-m", &format!("add {name} on {branch}")]);
    git(repo, &["checkout", "-q", "main"]);
}

/// TASK 2.3: a normal, non-conflicting merge whose re-verify passes → `Merged`,
/// and `main` advances to a new commit that actually contains the branch's work.
#[tokio::test]
async fn normal_merge_advances_main() {
    let repo = temp_repo();
    let guard = GitGuard::new(repo.path());
    let merge_lock = MergeLock::new();

    branch_adding_file(repo.path(), "quiver/task-a/attempt-1", "a.txt");
    let main_before = rev_parse(repo.path(), "main");

    let report = merge_and_reverify(
        &merge_lock,
        &guard,
        "quiver/task-a/attempt-1",
        "main",
        "merge task-a",
        &VerifyCommand::shell("exit 0"),
    )
    .await
    .expect("merge");

    assert_eq!(report.decision, MergeDecision::Merged);
    assert!(report.reason.is_none());

    // main moved forward and the merged file is present on main's working tree.
    let main_after = rev_parse(repo.path(), "main");
    assert_ne!(main_after, main_before, "main should have advanced");
    assert!(repo.path().join("a.txt").exists(), "merged file must be on main");
}

/// TASK 2.4 — the headline A+B case: two branches that each PASS verify alone,
/// but whose merged result FAILS the re-run verify. Assert the second is aborted
/// (rolled back), not left on `main`.
///
/// Construction: A adds `a.txt`, B adds `b.txt` (non-conflicting — different
/// files). The verify command is RED iff BOTH files exist. So:
///   * A alone (only a.txt) → green → merges, main advances.
///   * B re-verified against merged main (a.txt AND b.txt) → red → aborted.
#[tokio::test]
async fn a_plus_b_green_alone_red_together_second_is_aborted() {
    let repo = temp_repo();
    let guard = GitGuard::new(repo.path());
    let merge_lock = MergeLock::new();

    branch_adding_file(repo.path(), "quiver/task-a/attempt-1", "a.txt");
    branch_adding_file(repo.path(), "quiver/task-b/attempt-1", "b.txt");

    // Red exactly when both files coexist on the merged tree.
    let verify = VerifyCommand::shell("! ( test -f a.txt && test -f b.txt )");

    // A merges first (only a.txt present after merge) → green → Merged.
    let report_a = merge_and_reverify(
        &merge_lock,
        &guard,
        "quiver/task-a/attempt-1",
        "main",
        "merge task-a",
        &verify,
    )
    .await
    .expect("merge a");
    assert_eq!(report_a.decision, MergeDecision::Merged);

    // Snapshot main AFTER A — this is what B must roll back to, not the original.
    let main_after_a = rev_parse(repo.path(), "main");

    // B's re-verify against merged main (a.txt + b.txt) goes red → aborted.
    let report_b = merge_and_reverify(
        &merge_lock,
        &guard,
        "quiver/task-b/attempt-1",
        "main",
        "merge task-b",
        &verify,
    )
    .await
    .expect("merge b");

    assert_eq!(report_b.decision, MergeDecision::NeedsRebase);
    assert_eq!(report_b.reason, Some(RebaseReason::ReVerifyRed));

    // main is byte-identical to its post-A state: B was NOT left on main.
    assert_eq!(
        rev_parse(repo.path(), "main"),
        main_after_a,
        "B's collectively-red merge must be rolled back, not left on main"
    );
    assert!(repo.path().join("a.txt").exists(), "A's work stays merged");
    assert!(
        !repo.path().join("b.txt").exists(),
        "B's work must be reverted off main's working tree"
    );
    // No half-merge state left behind.
    assert!(!repo.path().join(".git/MERGE_HEAD").exists());
}
