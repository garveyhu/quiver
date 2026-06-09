//! Git worktree subsystem (DESIGN §6).
//!
//! All worktrees of one task-run share a single `.git` object store. Concurrent
//! *metadata* operations on that store race (`worktree add/remove/prune`, branch
//! create/delete) and corrupt refs / leave `index.lock` behind. The rule (§6.3):
//! serialize **every** shared-`.git` metadata op behind one mutex. In-worktree
//! file edits and the agent processes themselves are NOT serialized — they touch
//! only their own directory.
//!
//! [`GitGuard`] owns that mutex and is the only door to the shared `.git`. It
//! shells out to the `git` CLI (no `git2` crate, per the design constraint).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use tokio::sync::Mutex;

/// Bounded retry + exponential backoff for transient git lock contention
/// (DESIGN §6.3). Concurrent ops behind the metadata lock shouldn't collide, but
/// background tooling (or a not-yet-released `index.lock`) can still cause a
/// transient failure; we retry a few times before surfacing it as a task error.
#[derive(Clone, Copy, Debug)]
pub struct RetryConfig {
    /// Total attempts (1 = no retry).
    pub max_attempts: u32,
    /// Backoff before the 1st retry; doubles each subsequent retry.
    pub base_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(50),
        }
    }
}

impl RetryConfig {
    /// Backoff with no sleeping — for tests that exercise the retry count without
    /// the wall-clock cost.
    #[cfg(test)]
    fn instant(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            base_delay: Duration::ZERO,
        }
    }
}

/// Does this git error message look like a transient lock contention we should
/// retry (DESIGN §6.3)? Matches the strings git emits when a `.git` lock file is
/// held: `index.lock`, `config.lock`, or "could not lock"/"Unable to create ...
/// lock". A non-lock error (real failure) returns false → surfaced immediately.
fn is_transient_lock_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("index.lock")
        || m.contains("config.lock")
        || m.contains("could not lock")
        || m.contains("unable to create")
        || m.contains("file exists") && m.contains(".lock")
}

/// Run `op` with bounded retry + exponential backoff, retrying ONLY on errors
/// classified transient by `is_transient`. Surfaces the last error once attempts
/// are exhausted, or any non-transient error immediately.
async fn retry_on_lock<T, F, Fut>(
    cfg: RetryConfig,
    is_transient: fn(&str) -> bool,
    mut op: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut delay = cfg.base_delay;
    let mut attempt = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let transient = is_transient(&e.to_string());
                if !transient || attempt >= cfg.max_attempts {
                    return Err(e);
                }
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                delay = delay.saturating_mul(2);
                attempt += 1;
            }
        }
    }
}

/// Newtype for a worktree's checkout directory, so callers can't confuse it with
/// the repo root or an arbitrary path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreePath(PathBuf);

impl WorktreePath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for WorktreePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Outcome of [`GitGuard::remove`] (DESIGN §6.3): a clean worktree is removed; a
/// dirty one is NEVER force-removed — it is preserved and surfaced for human
/// review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// Worktree was clean and has been removed.
    Removed,
    /// Worktree had uncommitted changes; left in place. Carries the
    /// `git status --porcelain` text so the caller can surface what was dirty.
    PreservedDirty { status: String },
}

/// Serializes ALL shared-`.git` metadata operations behind one async mutex
/// (DESIGN §6.3). Construct one per repo and share it (e.g. `Arc<GitGuard>`)
/// across the workers of a run.
pub struct GitGuard {
    repo: PathBuf,
    /// Where per-attempt worktree directories are created. Kept OUTSIDE the
    /// repo's working tree so the checkouts don't show up as untracked files.
    worktrees_root: PathBuf,
    /// Retry/backoff policy for transient git lock contention (§6.3).
    retry: RetryConfig,
    /// The single git-metadata lock. Held only for the duration of a metadata
    /// command, never across an agent run or a build.
    meta_lock: Mutex<()>,
}

impl GitGuard {
    /// Create a guard for the repo whose `.git` lives under `repo`. Worktrees
    /// default to `<repo>/.quiver/worktrees`; override with
    /// [`GitGuard::with_worktrees_root`].
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        let repo = repo.into();
        let worktrees_root = repo.join(".quiver").join("worktrees");
        Self {
            repo,
            worktrees_root,
            retry: RetryConfig::default(),
            meta_lock: Mutex::new(()),
        }
    }

    /// Override where worktree directories are created (e.g. a temp dir in tests).
    pub fn with_worktrees_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.worktrees_root = root.into();
        self
    }

    /// Override the retry/backoff policy (e.g. for tests).
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// The repo root this guard serializes.
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// Disable background auto-gc on the repo for the duration of a run
    /// (DESIGN §6.3): `git config gc.auto 0`. Background `git gc` repacking the
    /// shared object store mid-run is a needless source of contention/corruption.
    /// This is a config write (a `.git` metadata mutation), so it runs behind the
    /// metadata lock.
    pub async fn disable_auto_gc(&self) -> anyhow::Result<()> {
        let _lock = self.meta_lock.lock().await;
        self.run_meta(&["config", "gc.auto", "0"]).await?;
        Ok(())
    }

    /// Run a shared-`.git` metadata command at the repo root with retry/backoff.
    /// Caller must already hold `meta_lock`.
    async fn run_meta(&self, args: &[&str]) -> anyhow::Result<Output> {
        retry_on_lock(self.retry, is_transient_lock_error, || {
            run_git(&self.repo, args)
        })
        .await
    }

    /// Create a worktree for one attempt of a task (DESIGN §6.1, §6.3).
    ///
    /// Runs `git worktree add <dir> -b quiver/task-<id>/attempt-<n>` behind the
    /// metadata lock. The branch name is unique per attempt and NEVER reused
    /// across retries, so a failed attempt's stale refs can't collide with the
    /// next (§6.3).
    pub async fn create(&self, task_id: &str, attempt: u32) -> anyhow::Result<WorktreePath> {
        let branch = attempt_branch(task_id, attempt);
        let dir = self
            .worktrees_root
            .join(format!("task-{task_id}"))
            .join(format!("attempt-{attempt}"));
        self.worktree_add(&dir, &branch).await
    }

    /// `git worktree add <dir> -b <branch>` behind the metadata lock.
    ///
    /// Serialized: two concurrent callers run one-at-a-time, so neither hits an
    /// `index.lock`/`config.lock` race in the shared `.git`.
    pub async fn worktree_add(
        &self,
        dir: &Path,
        branch: &str,
    ) -> anyhow::Result<WorktreePath> {
        let _lock = self.meta_lock.lock().await;
        let dir_str = path_arg(dir)?;
        self.run_meta(&["worktree", "add", dir_str, "-b", branch])
            .await?;
        Ok(WorktreePath(dir.to_path_buf()))
    }

    /// Remove a worktree, gating on its cleanliness (DESIGN §6.3).
    ///
    /// Checks `git status --porcelain` in the worktree FIRST (this read is cheap
    /// and not a shared-`.git` mutation, so it runs outside the metadata lock).
    /// If the tree is dirty, the worktree is preserved — Quiver NEVER force-
    /// removes uncommitted work — and [`RemoveOutcome::PreservedDirty`] is
    /// returned so the caller can surface it. Only a clean worktree is removed,
    /// behind the metadata lock.
    pub async fn remove(&self, path: &WorktreePath) -> anyhow::Result<RemoveOutcome> {
        let status = self.worktree_status(path).await?;
        if !status.trim().is_empty() {
            return Ok(RemoveOutcome::PreservedDirty { status });
        }

        let dir_str = path_arg(path.as_path())?;
        let _lock = self.meta_lock.lock().await;
        self.run_meta(&["worktree", "remove", dir_str]).await?;
        Ok(RemoveOutcome::Removed)
    }

    /// Deterministically discard a failed attempt's worktree (DESIGN §8.4).
    ///
    /// This is the deliberate abandon path — distinct from [`GitGuard::remove`]'s
    /// dirty-tree guard (§6.3). When a task is permanently failed we've already
    /// decided to throw the attempt away, so we `git worktree prune` (clear any
    /// stale admin entry) then `git worktree remove --force <dir>` (discard the
    /// checkout even if the crashed agent left it dirty). Both ref-mutating
    /// commands run behind the metadata lock with retry/backoff, so a crash never
    /// leaks an orphan worktree or a stale lock.
    pub async fn force_remove(&self, path: &WorktreePath) -> anyhow::Result<()> {
        let dir_str = path_arg(path.as_path())?;
        let _lock = self.meta_lock.lock().await;
        self.run_meta(&["worktree", "prune"]).await?;
        self.run_meta(&["worktree", "remove", "--force", dir_str])
            .await?;
        // Best-effort sweep: if the agent was killed mid-run (cancel) it may still
        // hold files when `git worktree remove` runs, so git unregisters the
        // worktree but leaves the directory on disk. Remove it so a cancel/crash
        // never leaks an orphan dir.
        let _ = std::fs::remove_dir_all(path.as_path());
        Ok(())
    }

    /// Sweep orphan worktree directories: any dir under `worktrees_root` that git
    /// no longer tracks as a worktree (left behind by a cancel/crash where the
    /// dying agent held files). Safe to run at startup — by then those processes
    /// are long dead, so the dirs delete cleanly. Returns how many were removed.
    /// Registered worktrees (incl. kept real-mode ones) are never touched.
    pub async fn sweep_orphan_worktrees(&self) -> anyhow::Result<usize> {
        let _lock = self.meta_lock.lock().await;
        let _ = self.run_meta(&["worktree", "prune"]).await;
        let listed = self.run_meta(&["worktree", "list", "--porcelain"]).await?;
        let text = String::from_utf8_lossy(&listed.stdout);
        let registered: Vec<PathBuf> = text
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .collect();
        let mut removed = 0usize;
        let Ok(entries) = std::fs::read_dir(&self.worktrees_root) else {
            return Ok(0);
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let canon = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !registered.iter().any(|r| *r == canon) && std::fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

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

    /// `git status --porcelain` run *inside* the worktree. Non-empty = dirty.
    async fn worktree_status(&self, path: &WorktreePath) -> anyhow::Result<String> {
        let out = run_git_in(path.as_path(), &["status", "--porcelain"]).await?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
}

/// Result of the lock-free §7 conflict probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictProbe {
    /// The merge would apply cleanly against the current base.
    Clean,
    /// The merge would conflict — surface for human review, never auto-resolve.
    Conflict,
}

/// Result of a `git merge --no-ff` attempt (DESIGN §7 step 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// The merge committed cleanly onto `main`.
    Merged,
    /// The merge produced conflicts and left them in the index (must `--abort`).
    Conflicted,
}

/// The unique per-attempt branch name (DESIGN §6.3): `quiver/task-<id>/attempt-<n>`.
pub fn attempt_branch(task_id: &str, attempt: u32) -> String {
    format!("quiver/task-{task_id}/attempt-{attempt}")
}

/// Render a path as a `&str` argument, erroring on non-UTF-8 (we control all the
/// paths we pass, so this is a guard, not an expected failure).
fn path_arg(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path: {}", path.display()))
}

/// Run a `git` command with `-C <repo>` against the shared repo root and return
/// its captured output, erroring (with stderr) on a non-zero exit.
async fn run_git(repo: &Path, args: &[&str]) -> anyhow::Result<Output> {
    run_git_in(repo, args).await
}

/// Run a `git` command with `-C <dir>` (the repo root OR a worktree) and return
/// its captured output, erroring (with stderr) on a non-zero exit.
async fn run_git_in(dir: &Path, args: &[&str]) -> anyhow::Result<Output> {
    let dir_str = path_arg(dir)?;
    let mut full = Vec::with_capacity(args.len() + 2);
    full.push("-C");
    full.push(dir_str);
    full.extend_from_slice(args);

    let output = tokio::process::Command::new("git")
        .args(&full)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("failed to spawn git {}: {e}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(output)
}

/// Run a `git` command with `-C <dir>` and return its full [`Output`] WITHOUT
/// treating a non-zero exit as an error — the caller inspects the exit status
/// itself. Used by commands whose non-zero exit is a meaningful signal rather
/// than a failure: `git merge-tree` (exit ≠ 0 == "conflict") and `git merge`
/// (exit ≠ 0 == "merge produced conflicts"). A genuine spawn failure (git not
/// found) is still an `Err`.
async fn run_git_status(dir: &Path, args: &[&str]) -> anyhow::Result<Output> {
    let dir_str = path_arg(dir)?;
    let mut full = Vec::with_capacity(args.len() + 2);
    full.push("-C");
    full.push(dir_str);
    full.extend_from_slice(args);

    tokio::process::Command::new("git")
        .args(&full)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("failed to spawn git {}: {e}", args.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Create a temp git repo with an initial commit, returning the temp dir
    /// (kept alive by the caller) and its path.
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
        // A multi-line source file so conflict tests can edit "the same line".
        std::fs::write(path.join("code.txt"), "line1\nline2\nline3\n").expect("write code");
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial commit"]);
        dir
    }

    /// Run a git command in `dir`, asserting success (test helper).
    fn git_in(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Commit `content` to `code.txt` on a fresh branch off `main`, then return
    /// to `main`. The branch is left in the repo for probing.
    fn branch_editing_code(repo: &Path, branch: &str, content: &str) {
        git_in(repo, &["checkout", "-q", "-b", branch, "main"]);
        std::fs::write(repo.join("code.txt"), content).expect("write code");
        git_in(repo, &["commit", "-q", "-am", &format!("edit on {branch}")]);
        git_in(repo, &["checkout", "-q", "main"]);
    }

    #[tokio::test]
    async fn concurrent_worktree_add_both_succeed_no_lock_error() {
        let repo = temp_repo();
        let repo_path = repo.path().to_path_buf();
        let guard = Arc::new(GitGuard::new(&repo_path));

        let wt_root = TempDir::new().expect("wt root");
        let dir_a = wt_root.path().join("a");
        let dir_b = wt_root.path().join("b");

        let g1 = guard.clone();
        let g2 = guard.clone();
        let h1 = tokio::spawn(async move {
            g1.worktree_add(&dir_a, "quiver/task-a/attempt-1").await
        });
        let h2 = tokio::spawn(async move {
            g2.worktree_add(&dir_b, "quiver/task-b/attempt-1").await
        });

        let r1 = h1.await.expect("join a");
        let r2 = h2.await.expect("join b");

        // Both must succeed; serialization means no index.lock/config.lock race.
        let p1 = r1.expect("worktree a added");
        let p2 = r2.expect("worktree b added");
        assert!(p1.as_path().is_dir());
        assert!(p2.as_path().is_dir());

        // git itself agrees there are exactly 3 worktrees (main + a + b) and no
        // stale lock files remain in the shared .git.
        let out = std::process::Command::new("git")
            .args(["-C", repo_path.to_str().unwrap(), "worktree", "list"])
            .output()
            .expect("worktree list");
        let listing = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            listing.lines().count(),
            3,
            "expected main + 2 worktrees, got:\n{listing}"
        );
        assert!(!repo_path.join(".git/index.lock").exists());
        assert!(!repo_path.join(".git/config.lock").exists());
    }

    /// List the current worktree directories (excludes the main one) by branch.
    fn worktree_branches(repo: &Path) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "worktree", "list", "--porcelain"])
            .output()
            .expect("worktree list");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.strip_prefix("branch refs/heads/"))
            .map(str::to_string)
            .collect()
    }

    #[tokio::test]
    async fn create_uses_unique_per_attempt_branch_and_dir() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

        let p1 = guard.create("42", 1).await.expect("attempt 1");
        let p2 = guard.create("42", 2).await.expect("attempt 2");

        // Distinct directories per attempt — never reused across retries.
        assert!(p1.as_path().is_dir());
        assert!(p2.as_path().is_dir());
        assert_ne!(p1, p2);
        assert!(p1.as_path().ends_with("task-42/attempt-1"));
        assert!(p2.as_path().ends_with("task-42/attempt-2"));

        // Distinct, correctly-named branches.
        let branches = worktree_branches(repo.path());
        assert!(branches.contains(&"quiver/task-42/attempt-1".to_string()));
        assert!(branches.contains(&"quiver/task-42/attempt-2".to_string()));
    }

    #[tokio::test]
    async fn remove_clean_worktree_succeeds() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

        let wt = guard.create("clean", 1).await.expect("create");
        assert!(wt.as_path().is_dir());

        let outcome = guard.remove(&wt).await.expect("remove");
        assert_eq!(outcome, RemoveOutcome::Removed);
        assert!(!wt.as_path().exists(), "clean worktree dir should be gone");

        // git no longer tracks the attempt branch's worktree.
        let branches = worktree_branches(repo.path());
        assert!(!branches.contains(&"quiver/task-clean/attempt-1".to_string()));
    }

    #[tokio::test]
    async fn remove_dirty_worktree_is_preserved_not_forced() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

        let wt = guard.create("dirty", 1).await.expect("create");
        // Make the worktree dirty: add an untracked file.
        std::fs::write(wt.as_path().join("scratch.txt"), "uncommitted work\n")
            .expect("write dirty file");

        let outcome = guard.remove(&wt).await.expect("remove");
        match outcome {
            RemoveOutcome::PreservedDirty { status } => {
                assert!(status.contains("scratch.txt"), "status should name the dirty file: {status}");
            }
            other => panic!("expected PreservedDirty, got {other:?}"),
        }

        // Never force-removed: the dir and its uncommitted work survive.
        assert!(wt.as_path().exists(), "dirty worktree must be preserved");
        assert!(wt.as_path().join("scratch.txt").exists());
        let branches = worktree_branches(repo.path());
        assert!(branches.contains(&"quiver/task-dirty/attempt-1".to_string()));
    }

    #[tokio::test]
    async fn force_remove_discards_even_a_dirty_worktree_no_leak() {
        let repo = temp_repo();
        let wt_root = TempDir::new().expect("wt root");
        let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());

        let wt = guard.create("crashed", 1).await.expect("create");
        // Simulate a crashed agent leaving uncommitted work behind.
        std::fs::write(wt.as_path().join("partial.txt"), "half-written\n").expect("dirty");

        // Deliberate abandon (§8.4): force-remove despite the dirty tree.
        guard.force_remove(&wt).await.expect("force remove");

        assert!(!wt.as_path().exists(), "forced worktree dir should be gone");
        let branches = worktree_branches(repo.path());
        assert!(
            !branches.contains(&"quiver/task-crashed/attempt-1".to_string()),
            "no orphan worktree should remain after force_remove"
        );
        assert!(!repo.path().join(".git/index.lock").exists());
    }

    #[test]
    fn classifies_transient_lock_errors() {
        assert!(is_transient_lock_error(
            "fatal: Unable to create '/r/.git/index.lock': File exists."
        ));
        assert!(is_transient_lock_error("could not lock config file"));
        assert!(is_transient_lock_error("error: config.lock held"));
        // A real failure is NOT transient — must surface immediately.
        assert!(!is_transient_lock_error("fatal: not a git repository"));
        assert!(!is_transient_lock_error("merge conflict in foo.rs"));
    }

    #[tokio::test]
    async fn retry_succeeds_after_n_transient_lock_errors() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        // Fail with a lock error the first 2 times, succeed on the 3rd.
        let result = retry_on_lock(RetryConfig::instant(5), is_transient_lock_error, || {
            let n = calls.get() + 1;
            calls.set(n);
            async move {
                if n < 3 {
                    Err(anyhow::anyhow!("fatal: Unable to create '.git/index.lock': File exists."))
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 3);
        assert_eq!(calls.get(), 3, "should retry exactly until success");
    }

    #[tokio::test]
    async fn retry_surfaces_after_exhausting_attempts() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        let result: anyhow::Result<()> =
            retry_on_lock(RetryConfig::instant(3), is_transient_lock_error, || {
                calls.set(calls.get() + 1);
                async { Err(anyhow::anyhow!("could not lock index.lock")) }
            })
            .await;
        assert!(result.is_err(), "exhausted retries must surface the error");
        assert_eq!(calls.get(), 3, "should attempt exactly max_attempts times");
    }

    #[tokio::test]
    async fn retry_does_not_retry_non_transient_error() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        let result: anyhow::Result<()> =
            retry_on_lock(RetryConfig::instant(5), is_transient_lock_error, || {
                calls.set(calls.get() + 1);
                async { Err(anyhow::anyhow!("fatal: not a git repository")) }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(calls.get(), 1, "a non-transient error surfaces on the first try");
    }

    #[tokio::test]
    async fn disable_auto_gc_sets_repo_config() {
        let repo = temp_repo();
        let guard = GitGuard::new(repo.path());
        guard.disable_auto_gc().await.expect("disable auto gc");

        let out = std::process::Command::new("git")
            .args(["-C", repo.path().to_str().unwrap(), "config", "--get", "gc.auto"])
            .output()
            .expect("read gc.auto");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    }

    // ---- TASK 2.2: lock-free conflict probe (DESIGN §7 step 1) ----

    #[tokio::test]
    async fn conflict_probe_clean_branch_no_conflict() {
        let repo = temp_repo();
        let guard = GitGuard::new(repo.path());
        // A branch that touches a *different* file than main → no conflict.
        git_in(repo.path(), &["checkout", "-q", "-b", "clean", "main"]);
        std::fs::write(repo.path().join("other.txt"), "new file\n").expect("write");
        git_in(repo.path(), &["add", "."]);
        git_in(repo.path(), &["commit", "-q", "-m", "add other.txt"]);
        git_in(repo.path(), &["checkout", "-q", "main"]);

        let probe = guard.conflict_probe("main", "clean").await.expect("probe");
        assert_eq!(probe, ConflictProbe::Clean);

        // The probe is read-only: main is untouched and no merge is in progress.
        assert!(!repo.path().join(".git/MERGE_HEAD").exists());
    }

    #[tokio::test]
    async fn conflict_probe_detects_same_line_conflict() {
        let repo = temp_repo();
        let guard = GitGuard::new(repo.path());

        // A branch edits the same line of code.txt that main will diverge on.
        branch_editing_code(repo.path(), "feature", "line1\nFEATURE\nline3\n");
        // main diverges on the SAME line → an irreconcilable conflict.
        std::fs::write(repo.path().join("code.txt"), "line1\nMAIN\nline3\n").expect("write");
        git_in(repo.path(), &["commit", "-q", "-am", "main edits line2"]);

        let probe = guard.conflict_probe("main", "feature").await.expect("probe");
        assert_eq!(probe, ConflictProbe::Conflict);

        // Still read-only: no merge state was created on main.
        assert!(!repo.path().join(".git/MERGE_HEAD").exists());
    }
}
