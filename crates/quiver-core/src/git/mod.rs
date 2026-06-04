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

use std::path::{Path, PathBuf};
use std::process::Output;

use tokio::sync::Mutex;

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
            meta_lock: Mutex::new(()),
        }
    }

    /// Override where worktree directories are created (e.g. a temp dir in tests).
    pub fn with_worktrees_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.worktrees_root = root.into();
        self
    }

    /// The repo root this guard serializes.
    pub fn repo(&self) -> &Path {
        &self.repo
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
        run_git(&self.repo, &["worktree", "add", dir_str, "-b", branch]).await?;
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
        run_git(&self.repo, &["worktree", "remove", dir_str]).await?;
        Ok(RemoveOutcome::Removed)
    }

    /// `git status --porcelain` run *inside* the worktree. Non-empty = dirty.
    async fn worktree_status(&self, path: &WorktreePath) -> anyhow::Result<String> {
        let out = run_git_in(path.as_path(), &["status", "--porcelain"]).await?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

/// The unique per-attempt branch name (DESIGN §6.3): `quiver/task-<id>/attempt-<n>`.
fn attempt_branch(task_id: &str, attempt: u32) -> String {
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
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial commit"]);
        dir
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
}
