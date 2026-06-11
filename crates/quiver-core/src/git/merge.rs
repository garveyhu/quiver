//! GitGuard 的合并 / 提交 / 分支操作(§5.7/§12):冲突预探、no-ff 合并、中止 / 硬重置、读 HEAD/SHA、
//! diff 统计、在 worktree 内提交、删 attempt 分支。守「main 永不坏」的底层 git 原语。

use super::{run_git, run_git_in, run_git_status};
use super::{ConflictProbe, GitGuard, MergeOutcome, SAFE_GIT_FLAGS, WorktreePath};

impl GitGuard {
    /// Force-delete an attempt branch (`git branch -D <branch>`) behind the
    /// metadata lock (DESIGN §6.3 — a ref mutation on the shared `.git`).
    ///
    /// Used after a non-`keep_branch` run's worktree is removed: `git worktree
    /// remove` tears down the checkout but LEAVES the branch ref, so without this
    /// the next run on the same repo collides at `worktree add` (branch already
    /// exists) and `quiver/task-*` branches accumulate in the user's repo. `-D`
    /// (force) because the branch holds the attempt's commits that were never
    /// merged into `main` — they are being deliberately discarded.
    pub async fn delete_branch(&self, branch: &str) -> anyhow::Result<()> {
        let _lock = self.meta_lock.lock().await;
        self.run_meta(&["branch", "-D", branch]).await?;
        Ok(())
    }

    /// Cheap, lock-free conflict probe (DESIGN §7 step 1).
    ///
    /// Runs `git merge-tree --write-tree <base> <branch>` to test whether
    /// merging `branch` into the *current* `base` (normally `main`) would
    /// conflict — WITHOUT taking the metadata lock and WITHOUT touching the
    /// working tree, the index, or any ref. `merge-tree` is purely read-only:
    /// it computes the merge into the object store and reports the result, so it
    /// is safe to run concurrently with other workers' worktree ops. This avoids
    /// most of the §7 contention window: a branch that will obviously conflict
    /// never even reaches the merge lock.
    ///
    /// Detection is by exit status (git ≥ 2.38): 0 = clean, non-zero = conflict.
    pub async fn conflict_probe(
        &self,
        base: &str,
        branch: &str,
    ) -> anyhow::Result<ConflictProbe> {
        let out =
            run_git_status(&self.repo, &["merge-tree", "--write-tree", base, branch]).await?;
        Ok(if out.status.success() {
            ConflictProbe::Clean
        } else {
            ConflictProbe::Conflict
        })
    }

    /// `git merge --no-ff <branch>` into the currently-checked-out `main` of the
    /// repo root (DESIGN §7 step 3). This is the ref-mutating step, so it runs
    /// behind the metadata lock — but ONLY for the duration of this one command
    /// (the caller releases nothing of the §7 merge lock here; that is held
    /// across the whole pipeline by `merge.rs`). Returns whether the merge
    /// committed cleanly or produced conflicts (left in the index).
    pub async fn merge_no_ff(&self, branch: &str, message: &str) -> anyhow::Result<MergeOutcome> {
        let _lock = self.meta_lock.lock().await;
        let out = run_git_status(
            &self.repo,
            &["merge", "--no-ff", "-m", message, branch],
        )
        .await?;
        Ok(if out.status.success() {
            MergeOutcome::Merged
        } else {
            MergeOutcome::Conflicted
        })
    }

    /// `git merge --abort` at the repo root (DESIGN §7 step 6), undoing an
    /// **in-progress** (conflicted, uncommitted) merge and restoring `main` to
    /// exactly its pre-merge state. Ref-mutating → behind the metadata lock for
    /// this one command.
    ///
    /// NOTE: this only applies while a merge is in progress (`MERGE_HEAD`
    /// present). A clean `--no-ff` merge commits immediately, so to roll *that*
    /// back you must [`GitGuard::reset_hard`] to the pre-merge commit instead.
    pub async fn merge_abort(&self) -> anyhow::Result<()> {
        let _lock = self.meta_lock.lock().await;
        run_git(&self.repo, &["merge", "--abort"]).await?;
        Ok(())
    }

    /// `git reset --hard <commit>` at the repo root, used to roll back an
    /// already-committed clean merge when the re-verify against merged `main`
    /// goes red (DESIGN §7 step 6). Restores `main` (ref + working tree) to
    /// exactly `commit`, so `main` is byte-identical to before the attempt.
    /// Ref-mutating → behind the metadata lock for this one command.
    pub async fn reset_hard(&self, commit: &str) -> anyhow::Result<()> {
        let _lock = self.meta_lock.lock().await;
        run_git(&self.repo, &["reset", "--hard", commit]).await?;
        Ok(())
    }

    /// The commit SHA the repo's `main` ref currently points at (lock-free read).
    /// Used to assert `main` is byte-identical before/after an aborted attempt
    /// (DESIGN §7: a blocked task never mutates `main`).
    pub async fn head_commit(&self, refname: &str) -> anyhow::Result<String> {
        let out = run_git(&self.repo, &["rev-parse", refname]).await?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The commit SHA at a WORKTREE's HEAD — the attempt branch tip after the agent
    /// ran (its own commit in real mode; the base it branched from for a no-op run).
    /// Captured before cleanup to bind an episode to its commit (DESIGN §6.2).
    /// Lock-free read (worktree-local).
    pub async fn head_sha(&self, worktree: &WorktreePath) -> anyhow::Result<String> {
        let out = run_git_in(worktree.as_path(), &["rev-parse", "HEAD"]).await?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// `git diff --shortstat <base>` in a worktree: the one-line change summary of
    /// everything the attempt did versus `base` (committed + uncommitted). Empty
    /// for a no-op run; e.g. `" 2 files changed, 9 insertions(+), 1 deletion(-)"`
    /// for a real one. Binds the episode to its diff size (DESIGN §6.2). Lock-free.
    pub async fn diff_stat(&self, worktree: &WorktreePath, base: &str) -> anyhow::Result<String> {
        let out = run_git_in(worktree.as_path(), &["diff", "--shortstat", base]).await?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Agent 跑完后,把 worktree 工作区的改动落成一个 commit 到它的分支(DESIGN §6.2)。
    ///
    /// **为什么必须有这一步**:claude(`--permission-mode acceptEdits`)只**改文件**、**不 git
    /// commit**。而真交付(§7)合并到 `main` 的是**分支提交**——不提交则分支 HEAD 还停在 base、
    /// `merge` 是空的、reverify 看不到改动 → `needs_rebase`。这是 real 交付链路一直断裂的根因。
    ///
    /// 无改动(no-op run)→ 不造空 commit,返回 `false`。提交了 → `true`。显式 author + 禁签名/
    /// 钩子,不依赖目标 repo 的 user 配置、也不触发它的 pre-commit hook。worktree-local。
    pub async fn commit_worktree(
        &self,
        worktree: &WorktreePath,
        message: &str,
    ) -> anyhow::Result<bool> {
        let wt = worktree.as_path();
        // 工作区有改动才提交(status --porcelain 非空 = 有改动)。
        let status = run_git_in(wt, &["status", "--porcelain"]).await?;
        if String::from_utf8_lossy(&status.stdout).trim().is_empty() {
            return Ok(false); // no-op run:不创建空 commit
        }
        let mut add = SAFE_GIT_FLAGS.to_vec();
        add.extend_from_slice(&["add", "-A"]);
        run_git_in(wt, &add).await?;
        let mut commit = SAFE_GIT_FLAGS.to_vec();
        commit.extend_from_slice(&[
            "-c",
            "user.name=Quiver Agent",
            "-c",
            "user.email=agent@quiver.local",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            message,
        ]);
        run_git_in(wt, &commit).await?;
        Ok(true)
    }

}
