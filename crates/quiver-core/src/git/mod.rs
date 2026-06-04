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

/// Serializes ALL shared-`.git` metadata operations behind one async mutex
/// (DESIGN §6.3). Construct one per repo and share it (e.g. `Arc<GitGuard>`)
/// across the workers of a run.
pub struct GitGuard {
    repo: PathBuf,
    /// The single git-metadata lock. Held only for the duration of a metadata
    /// command, never across an agent run or a build.
    meta_lock: Mutex<()>,
}

impl GitGuard {
    /// Create a guard for the repo whose `.git` lives under `repo`.
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self {
            repo: repo.into(),
            meta_lock: Mutex::new(()),
        }
    }

    /// The repo root this guard serializes.
    pub fn repo(&self) -> &Path {
        &self.repo
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
        run_git(
            &self.repo,
            &["worktree", "add", dir_str, "-b", branch],
        )
        .await?;
        Ok(WorktreePath(dir.to_path_buf()))
    }
}

/// Render a path as a `&str` argument, erroring on non-UTF-8 (we control all the
/// paths we pass, so this is a guard, not an expected failure).
fn path_arg(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path: {}", path.display()))
}

/// Run a `git` command with `-C <repo>` and return its captured output, erroring
/// (with stderr) on a non-zero exit.
async fn run_git(repo: &Path, args: &[&str]) -> anyhow::Result<Output> {
    let repo_str = path_arg(repo)?;
    let mut full = Vec::with_capacity(args.len() + 2);
    full.push("-C");
    full.push(repo_str);
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
}
