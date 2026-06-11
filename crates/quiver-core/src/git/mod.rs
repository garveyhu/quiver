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
use std::time::Duration;

use tokio::sync::Mutex;

// 按职责拆出的子模块(单一职责):瞬时锁重试、worktree 生命周期、合并/提交。impl GitGuard 跨文件
// (私有方法/helper 对后代模块可见);pub 类型 + run_git* + run_meta + 基础方法留本文件。
mod merge;
mod retry;
mod worktree;

use retry::{is_transient_lock_error, retry_on_lock};

/// §8.2 防御性 git 配置(作为 `git -c K=V …` 前缀注入到任何会 **clone/checkout** 的命令)。
/// 恶意仓库能在检出**那一瞬间**就执行代码 —— 比 git 钩子更隐蔽的几个口子全堵上:
/// - `core.hooksPath=/dev/null`:关掉仓库钩子(post-checkout 等)。
/// - `core.fsmonitor=false`:关掉可被劫持的 fsmonitor 程序。
/// - `core.attributesFile=/dev/null`:不读用户全局 `.gitattributes`(filter/diff driver 触发点)。
/// - `core.symlinks=false`:检出符号链接当普通文件,挡符号链接逃逸。
///
/// 子模块 `update=!cmd` 注入靠**调用方加 `--no-recurse-submodules`** 挡(子模块根本不检出);
/// 子模块的 `file://` 协议在现代 git(≥2.38,CVE-2022-39253)默认已禁,故**不**显式设
/// `protocol.file.allow=never`——那会连可信的本地主仓库 clone(`file` 源)一起误杀。
/// 仓库内 `.gitattributes` 仍可能声明 filter,但没有 config 里的 driver 命令就执行不了;
/// 这组 flag + 沙箱(进程被 seatbelt 罩住)是 §8.2「克隆前就位」的双保险。
pub const SAFE_GIT_FLAGS: &[&str] = &[
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.attributesFile=/dev/null",
    "-c",
    "core.symlinks=false",
];

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
