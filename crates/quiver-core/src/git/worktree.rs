//! GitGuard 的 worktree 生命周期(§6/§8.4):为每个任务尝试建独立 worktree(唯一分支+目录)、
//! 干净移除 / 强制移除、清扫孤儿 worktree。共享 .git 元数据操作经 run_meta 串行 + 重试。

use std::path::{Path, PathBuf};

use super::{attempt_branch, path_arg, run_git_in};
use super::{GitGuard, RemoveOutcome, WorktreePath, SAFE_GIT_FLAGS};

impl GitGuard {
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
        // `.quiver/`(worktree 根)是 quiver 的私有目录,不该出现在用户项目的 `git status` ——
        // 本地忽略它(写 .git/info/exclude,不碰用户的 .gitignore、不在工作区留文件)。
        self.ensure_quiver_ignored();
        let _lock = self.meta_lock.lock().await;
        let dir_str = path_arg(dir)?;
        // §8.2:worktree add 会 checkout 新分支 → 触发 .gitattributes 的 smudge 过滤器,
        // 同 clone 一样是"检出即执行"的口子。前置防御 flags(它们紧跟 `-C <repo>` 之后、
        // 子命令 `worktree` 之前,正是 git 全局选项的位置)。
        let mut args: Vec<&str> = SAFE_GIT_FLAGS.to_vec();
        args.extend_from_slice(&["worktree", "add", dir_str, "-b", branch]);
        self.run_meta(&args).await?;
        Ok(WorktreePath(dir.to_path_buf()))
    }

    /// 把 `/.quiver/` 写进 `<repo>/.git/info/exclude`,让 quiver 的私有 worktree 根目录不污染
    /// 用户的 `git status`。本地忽略 —— 不碰用户 `.gitignore`、不在工作区留任何文件。
    /// best-effort:已忽略则跳过;写失败不影响 worktree 创建(顶多 status 多一行)。
    fn ensure_quiver_ignored(&self) {
        let needle = "/.quiver/";
        let info_dir = self.repo.join(".git").join("info");
        let exclude = info_dir.join("exclude");
        if let Ok(content) = std::fs::read_to_string(&exclude) {
            if content.lines().any(|l| l.trim() == needle) {
                return; // 已忽略
            }
        }
        let _ = std::fs::create_dir_all(&info_dir);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&exclude) {
            use std::io::Write;
            let _ = writeln!(f, "{needle}");
        }
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
        let mut removed = 0usize;
        // (a) 崩溃遗留:worktree 目录还在、git 标 `prunable`(上个会话被强杀,其 worker 没正常
        // 收尾)→ `worktree remove --force`。否则它一直被 git registered,下面 (c) 的"删未注册
        // 裸目录"碰不到它 → 每次崩溃累积一个、占磁盘。(崩溃后立即 sweep 时孤儿进程可能还在写、
        // remove 失败;但它退出后下次启动 sweep 必清,不再无限累积。)
        if let Ok(listed) = self.run_meta(&["worktree", "list", "--porcelain"]).await {
            let text = String::from_utf8_lossy(&listed.stdout).to_string();
            let mut cur: Option<String> = None;
            for line in text.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    cur = Some(p.to_string());
                } else if line.starts_with("prunable") {
                    if let Some(p) = cur.take() {
                        if self.run_meta(&["worktree", "remove", "--force", &p]).await.is_ok() {
                            removed += 1;
                        }
                    }
                }
            }
        }
        // (b) prune 掉"目录已删、记录还在"的失效条目。
        let _ = self.run_meta(&["worktree", "prune"]).await;
        // (c) 删 worktrees_root 下 git 完全不认识的裸目录。
        let listed = self.run_meta(&["worktree", "list", "--porcelain"]).await?;
        let text = String::from_utf8_lossy(&listed.stdout);
        let registered: Vec<PathBuf> = text
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .collect();
        let Ok(entries) = std::fs::read_dir(&self.worktrees_root) else {
            return Ok(removed);
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

    /// `git status --porcelain` run *inside* the worktree. Non-empty = dirty.
    async fn worktree_status(&self, path: &WorktreePath) -> anyhow::Result<String> {
        let out = run_git_in(path.as_path(), &["status", "--porcelain"]).await?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

}
