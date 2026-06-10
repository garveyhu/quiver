//! Serialized merge with re-verify against merged `main` (DESIGN §7).
//!
//! The verify-gate's promise — `main` is never half-merged or broken — is
//! hardest when several workers finish at once: A verifies green against
//! `main@T0`, B merges and moves `main` to T1, then A merges on top, and A's
//! green result was never checked against B. Two individually-green branches can
//! be collectively red. This module closes that hole.
//!
//! ## The merge lock is DELIBERATELY separate from the §6 metadata lock
//!
//! [`MergeLock`] is a `tokio::sync::Mutex<()>` **distinct** from `GitGuard`'s
//! metadata mutex. It is held across BOTH the `git merge` AND the (potentially
//! long) re-verify build. If it were the *same* mutex that serializes
//! `worktree add/remove/prune`, then during one merge's re-verify **no other
//! worker could create or tear down a worktree** — a long verify run would stall
//! the whole fan-out's worktree lifecycle (DESIGN §7 step 2). So the ref-mutating
//! git commands (`git merge` / `--abort`) take the metadata lock only for their
//! own brief duration (inside [`GitGuard`]); the re-verify runs while holding
//! ONLY this merge lock, leaving the metadata lock free for other workers.
//!
//! ## Never auto-resolve (DESIGN §7 hard rule)
//!
//! A probe-detected conflict, a merge that conflicts, OR a red re-verify all end
//! in `git merge --abort` (leaving `main` byte-identical to before the attempt)
//! and [`MergeDecision::NeedsRebase`]. No auto-rebase, no "accept theirs", no LLM
//! conflict resolution overnight — a blocked task waits for a human.

use tokio::sync::Mutex;

use crate::git::{ConflictProbe, GitGuard, MergeOutcome};
use crate::verify::{VerifyCommand, VerifyResult};

/// The single §7 merge lock: only one merge+re-verify happens at a time, full
/// stop. **Distinct from `GitGuard`'s metadata mutex** (see module docs) — held
/// across the whole merge pipeline including the long re-verify, so it must not
/// be the lock that serializes worktree lifecycle ops.
#[derive(Default)]
pub struct MergeLock {
    lock: Mutex<()>,
}

impl MergeLock {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The terminal decision of the §7 merge pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeDecision {
    /// Merged into `main` and the re-verify against merged `main` passed (§7
    /// step 5). `main` advanced; the work is integrated.
    Merged,
    /// A probe/merge conflict OR a red re-verify against merged `main`. The
    /// merge was aborted, `main` is untouched, and the task is surfaced for human
    /// review (§7 step 6). NEVER auto-resolved.
    NeedsRebase,
}

/// Why a `NeedsRebase` decision was reached, for the morning report / logging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebaseReason {
    /// The lock-free or under-lock conflict probe found a conflict (§7 step 1).
    ProbeConflict,
    /// `git merge` itself produced conflicts (a race the probe didn't catch).
    MergeConflict,
    /// The merge applied, but the re-verify against merged `main` went red — the
    /// "A+B green alone, red together" case (§7 step 4).
    ReVerifyRed,
}

/// The full outcome of one merge attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeReport {
    pub decision: MergeDecision,
    /// `Some` iff `decision == NeedsRebase`.
    pub reason: Option<RebaseReason>,
}

/// Run the §7 serialized merge pipeline for one verified branch.
///
/// Acquires the [`MergeLock`] for the whole call (so merges are globally
/// serialized) and:
///
/// 1. Re-probes the branch against the *current* `main` under the lock (main may
///    have advanced since the cheap pre-lock probe). Conflict → abort path.
/// 2. `git merge --no-ff <branch>` into `main` — the ref-mutating step, which
///    takes the §6 metadata lock only for its own duration. A merge that
///    conflicts → `git merge --abort` → `NeedsRebase{MergeConflict}`.
/// 3. **Re-runs `verify` against the merged `main`'s working tree** (the repo
///    root). This is the step that catches collectively-red merges. It runs
///    while holding ONLY the merge lock — the metadata lock is free, so other
///    workers' worktree ops are not stalled.
/// 4. Green → keep the merge → `Merged`. Red → `git merge --abort` →
///    `NeedsRebase{ReVerifyRed}`.
///
/// `main_ref`/`merged_message` parameterize the target branch name and the merge
/// commit message. The re-verify runs with cwd = the repo root because, after a
/// successful `git merge` there, the repo's working tree *is* merged `main`.
pub async fn merge_and_reverify(
    merge_lock: &MergeLock,
    guard: &GitGuard,
    branch: &str,
    main_ref: &str,
    merged_message: &str,
    verify: &VerifyCommand,
) -> anyhow::Result<MergeReport> {
    // Globally serialize merges (§7 step 2). Held across merge AND re-verify.
    let _merge = merge_lock.lock.lock().await;

    // Snapshot `main` before we touch it, so a red re-verify can restore it
    // byte-for-byte. A clean `--no-ff` merge commits immediately (it does NOT
    // stay "in progress"), so rolling that back is a `reset --hard` to this SHA,
    // not a `merge --abort` (which only undoes an uncommitted conflicted merge).
    let main_before = guard.head_commit(main_ref).await?;

    // (1) Re-probe under the lock: `main` may have advanced since the cheap
    // pre-lock probe, so the only authoritative conflict check is here.
    if guard.conflict_probe(main_ref, branch).await? == ConflictProbe::Conflict {
        return Ok(needs_rebase(RebaseReason::ProbeConflict));
    }

    // (2) Merge into `main` (ref-mutating; brief metadata-lock acquisition).
    match guard.merge_no_ff(branch, merged_message).await? {
        MergeOutcome::Merged => {}
        MergeOutcome::Conflicted => {
            // A conflicted merge is in progress and uncommitted → `--abort`
            // restores `main` exactly. Leave `main` untouched.
            guard.merge_abort().await?;
            return Ok(needs_rebase(RebaseReason::MergeConflict));
        }
    }

    // (3) Re-verify against the merged `main` working tree (repo root). Held
    // under the merge lock ONLY — metadata lock stays free for other workers.
    let reverify = verify.run(guard.repo()).await?;

    // (4) Green keeps the merge; red rolls `main` back to its pre-merge commit —
    // never auto-resolve. The merge was already committed, so undo via reset.
    if reverify == VerifyResult::Passed {
        Ok(MergeReport {
            decision: MergeDecision::Merged,
            reason: None,
        })
    } else {
        guard.reset_hard(&main_before).await?;
        Ok(needs_rebase(RebaseReason::ReVerifyRed))
    }
}

fn needs_rebase(reason: RebaseReason) -> MergeReport {
    MergeReport {
        decision: MergeDecision::NeedsRebase,
        reason: Some(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitGuard;
    use crate::verify::VerifyCommand;
    use std::process::Command;
    use tempfile::TempDir;

    /// 在临时目录建一个 repo:main 有 base.txt;分支 `branch` 在 `file` 写 `content`。
    /// 回到 main。返回 TempDir(持有目录生命周期)。
    fn repo_with_branch(branch: &str, file: &str, content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();
        let git = move |args: &[&str]| {
            let o = Command::new("git").current_dir(&p).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["-c", "init.defaultBranch=main", "init", "--quiet"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(dir.path().join("base.txt"), "base").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "init"]);
        git(&["checkout", "--quiet", "-b", branch]);
        std::fs::write(dir.path().join(file), content).unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "work"]);
        git(&["checkout", "--quiet", "main"]);
        dir
    }

    #[tokio::test]
    async fn green_merge_advances_main_and_lands_file() {
        let dir = repo_with_branch("feat", "new.txt", "hello");
        let guard = GitGuard::new(dir.path());
        let before = guard.head_commit("main").await.unwrap();
        let report = merge_and_reverify(
            &MergeLock::new(), &guard, "feat", "main", "merge feat", &VerifyCommand::shell("exit 0"),
        )
        .await
        .unwrap();
        assert_eq!(report.decision, MergeDecision::Merged);
        assert_ne!(guard.head_commit("main").await.unwrap(), before, "main 推进了");
        assert!(dir.path().join("new.txt").exists(), "合并的文件落到了 main");
    }

    #[tokio::test]
    async fn red_reverify_rolls_main_back_byte_for_byte() {
        // 「main 永不坏」铁律:合并后重验红 → main reset 回合并前,文件不留。
        let dir = repo_with_branch("feat", "new.txt", "hello");
        let guard = GitGuard::new(dir.path());
        let before = guard.head_commit("main").await.unwrap();
        let report = merge_and_reverify(
            &MergeLock::new(), &guard, "feat", "main", "merge feat", &VerifyCommand::shell("exit 1"),
        )
        .await
        .unwrap();
        assert_eq!(report.decision, MergeDecision::NeedsRebase);
        assert_eq!(report.reason, Some(RebaseReason::ReVerifyRed));
        assert_eq!(guard.head_commit("main").await.unwrap(), before, "main 还原到合并前");
        assert!(!dir.path().join("new.txt").exists(), "红验证 → 坏改动没进 main");
    }

    #[tokio::test]
    async fn conflict_leaves_main_untouched() {
        // 分支和 main 改同一文件 → 冲突 → NeedsRebase,main 一字不动。
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();
        let git = move |args: &[&str]| {
            let o = Command::new("git").current_dir(&p).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["-c", "init.defaultBranch=main", "init", "--quiet"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(dir.path().join("f.txt"), "base").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "init"]);
        git(&["checkout", "--quiet", "-b", "feat"]);
        std::fs::write(dir.path().join("f.txt"), "feat-version").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "feat"]);
        git(&["checkout", "--quiet", "main"]);
        std::fs::write(dir.path().join("f.txt"), "main-version").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "main2"]);

        let guard = GitGuard::new(dir.path());
        let before = guard.head_commit("main").await.unwrap();
        let report = merge_and_reverify(
            &MergeLock::new(), &guard, "feat", "main", "merge feat", &VerifyCommand::shell("exit 0"),
        )
        .await
        .unwrap();
        assert_eq!(report.decision, MergeDecision::NeedsRebase);
        assert_eq!(guard.head_commit("main").await.unwrap(), before, "冲突 → main 不动");
    }
}
