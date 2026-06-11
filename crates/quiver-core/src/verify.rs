//! The verify-gate: run a configurable build/test command in a worktree
//! (DESIGN §7).
//!
//! A run is only allowed to merge if the agent produced `Result{ok:true}` AND
//! this command exits 0 against the worktree's contents. The same command is
//! re-run against the merged `main` (DESIGN §7 step 4) to catch the
//! "A and B each green alone, red together" case — so this module is the single
//! definition of "what green means," used both pre-merge and post-merge.
//!
//! The command is an explicit argv (`Vec<String>`); a shell string is expressed
//! as `["sh", "-c", "<script>"]` by the caller. Exit 0 → [`VerifyResult::Passed`];
//! any non-zero exit (or a signal) → [`VerifyResult::Failed`]. A command that
//! cannot even be spawned is a hard error (surfaced to the caller), not a silent
//! "failed", because an un-spawnable verify command is a misconfiguration, not a
//! red test.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

/// verify 命令的硬超时:跑的是 worker 改过的代码,可能死循环/挂起。超了就判失败 + 杀进程,绝不让
/// 一个卡住的 verify 持着全局合并锁、锁死整条交付链路(§7)。10 分钟对正常 build/test 足够宽。
const VERIFY_TIMEOUT: Duration = Duration::from_secs(600);

/// A configurable verify command (DESIGN §7): the argv executed in the task's
/// worktree to decide whether the work is green.
///
/// Built from an explicit argv. For a shell one-liner, construct it from
/// `["sh", "-c", script]` — the gate never implicitly wraps anything in a shell,
/// so what runs is exactly what the caller specified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyCommand {
    argv: Vec<String>,
}

impl VerifyCommand {
    /// Build a verify command from an explicit argv. The first element is the
    /// program; the rest are its arguments.
    pub fn new(argv: impl IntoIterator<Item = impl Into<String>>) -> Result<Self> {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.is_empty() {
            anyhow::bail!("verify command argv must not be empty");
        }
        Ok(Self { argv })
    }

    /// Convenience for the common shell-string form: runs `sh -c <script>`.
    pub fn shell(script: impl Into<String>) -> Self {
        Self {
            argv: vec!["sh".to_string(), "-c".to_string(), script.into()],
        }
    }

    /// The argv that will be executed (for logging / display).
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// Run the command with `cwd = dir` and classify the exit (DESIGN §7).
    ///
    /// Exit 0 → [`VerifyResult::Passed`]; any non-zero exit or termination by
    /// signal → [`VerifyResult::Failed`]. A spawn failure (program not found,
    /// not executable) is returned as an `Err` — that is a misconfigured gate,
    /// distinct from a red test.
    pub async fn run(&self, dir: &Path) -> Result<VerifyResult> {
        Ok(self.run_capturing(dir).await?.0)
    }

    /// Like [`run`](Self::run) but also returns the command's captured output
    /// (combined stdout+stderr, tail-truncated). The pre-merge gate uses this so a
    /// red verify can show the user *why* it failed, not just that it did.
    pub async fn run_capturing(&self, dir: &Path) -> Result<(VerifyResult, String)> {
        self.run_capturing_to(dir, VERIFY_TIMEOUT).await
    }

    /// `run_capturing` 的可注入超时版(测试用短超时验证「卡住的 verify 判失败、不挂起」)。
    async fn run_capturing_to(
        &self,
        dir: &Path,
        timeout: Duration,
    ) -> Result<(VerifyResult, String)> {
        let (program, args) = self
            .argv
            .split_first()
            .expect("argv is non-empty by construction");

        // 程序是绝对/相对路径却不可执行 → 这是**配错的 gate**(VerifyError),不是测试红。
        // 必须在包沙箱前判:否则 `sandbox-exec` 自己能 spawn、execvp 目标失败只会非 0 退出,
        // 把"配置错误"误吞成"测试红"(VerifyFailed),丢掉 §7 的这层区分。PATH 名(无 '/')
        // 交给执行层解析,不在这里拦。
        if program.contains('/') && !is_executable_file(Path::new(program)) {
            anyhow::bail!("verify command program is not executable: {program:?}");
        }

        // §8 沙箱:verify 跑的是 **worker 改过的代码**(测试/构建脚本可任意执行),最该关进
        // 沙箱 —— 写只限 `dir`(这个 worktree)、默认不出网。macOS 用 seatbelt 包一层;其它
        // 平台 SandboxPolicy::is_supported()=false,原样跑(不静默假装安全,见 §8.2)。
        let policy = crate::sandbox::SandboxPolicy::for_worktree(dir);
        let mut cmd = if crate::sandbox::SandboxPolicy::is_supported() {
            let (wprog, wargs) = policy.wrap(program, args);
            let mut c = tokio::process::Command::new(wprog);
            c.args(wargs);
            c
        } else {
            let mut c = tokio::process::Command::new(program);
            c.args(args);
            c
        };

        // §7 verify 超时硬上限:merge_and_reverify 全程持全局合并锁 → 一个卡住的 verify(测试死循环/
        // 挂起)会**锁死整条交付链路**(后续所有交付阻塞)。超时即判失败(red,不是配错)并杀进程
        // (kill_on_drop:future 被 timeout drop → child drop → 杀进程组)。stdin null 已防等输入。
        let child = cmd
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn verify command {program:?}"))?;
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(res) => res.with_context(|| format!("verify command {program:?} failed to run"))?,
            Err(_) => {
                return Ok((
                    VerifyResult::Failed,
                    format!(
                        "verify 命令超过 {}s 未结束,已判失败并终止(防卡死交付链路 §7)",
                        timeout.as_secs()
                    ),
                ));
            }
        };

        let result = if output.status.success() {
            VerifyResult::Passed
        } else {
            VerifyResult::Failed
        };
        let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }
        Ok((result, tail(&combined, 2000)))
    }
}

/// 路径是否指向一个可执行文件(存在 + 普通文件 + 任一执行位)。用于在包沙箱前甄别
/// "gate 程序配错"。非 Unix 退化为"存在即可"。
fn is_executable_file(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return meta.permissions().mode() & 0o111 != 0;
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Keep the last `max` chars of `s` (the failure summary is usually at the end),
/// prefixing an ellipsis when truncated.
fn tail(s: &str, max: usize) -> String {
    let s = s.trim_end();
    if s.len() <= max {
        return s.to_string();
    }
    let start = s.len() - max;
    // Snap to a char boundary so we never split a multibyte char.
    let start = (start..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(s.len());
    format!("…\n{}", &s[start..])
}

/// Outcome of running the verify-gate (DESIGN §7): green (exit 0) or red.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyResult {
    Passed,
    Failed,
}

impl VerifyResult {
    /// Did the gate pass?
    pub fn passed(self) -> bool {
        matches!(self, VerifyResult::Passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn passing_command_yields_passed() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::shell("exit 0");
        let res = cmd.run(dir.path()).await.expect("run");
        assert_eq!(res, VerifyResult::Passed);
        assert!(res.passed());
    }

    #[tokio::test]
    async fn failing_command_yields_failed() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::shell("exit 1");
        let res = cmd.run(dir.path()).await.expect("run");
        assert_eq!(res, VerifyResult::Failed);
        assert!(!res.passed());
    }

    #[tokio::test]
    async fn command_runs_in_the_given_cwd() {
        // The command observes the worktree it is pointed at: a file present in
        // `dir` makes the gate pass, its absence makes it fail.
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("marker"), "x").expect("write marker");

        let present = VerifyCommand::shell("test -f marker");
        assert_eq!(present.run(dir.path()).await.expect("run"), VerifyResult::Passed);

        let absent = VerifyCommand::shell("test -f does-not-exist");
        assert_eq!(absent.run(dir.path()).await.expect("run"), VerifyResult::Failed);
    }

    #[tokio::test]
    async fn explicit_argv_is_run_verbatim() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::new(["true"]).expect("argv");
        assert_eq!(cmd.run(dir.path()).await.expect("run"), VerifyResult::Passed);

        let cmd = VerifyCommand::new(["false"]).expect("argv");
        assert_eq!(cmd.run(dir.path()).await.expect("run"), VerifyResult::Failed);
    }

    #[tokio::test]
    async fn unspawnable_command_is_an_error_not_a_red_test() {
        let dir = TempDir::new().expect("tempdir");
        // 绝对路径程序不存在 = 配错的 gate。包沙箱前的可执行性预检把它甄别成 Err
        // (VerifyError),不是测试红(VerifyFailed)—— 沙箱包裹也不丢这层区分(§7)。
        let cmd = VerifyCommand::new(["/nonexistent/quiver/definitely-not-a-program"])
            .expect("argv");
        assert!(
            cmd.run(dir.path()).await.is_err(),
            "an un-spawnable verify command must surface as an error, not Failed"
        );
    }

    #[test]
    fn empty_argv_is_rejected() {
        let empty: Vec<String> = Vec::new();
        assert!(VerifyCommand::new(empty).is_err());
    }

    #[tokio::test]
    async fn run_capturing_returns_combined_output() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::shell("echo out-line; echo err-line >&2; exit 1");
        let (res, out) = cmd.run_capturing(dir.path()).await.expect("run");
        assert_eq!(res, VerifyResult::Failed);
        assert!(out.contains("out-line"), "stdout should be captured: {out:?}");
        assert!(out.contains("err-line"), "stderr should be captured: {out:?}");
    }

    #[tokio::test]
    async fn hung_verify_times_out_as_failed_not_hang() {
        // 卡住的 verify(sleep 30)在短超时下应判失败并立刻返回(不挂起锁死交付链路),进程被杀。
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::shell("sleep 30");
        let started = std::time::Instant::now();
        let (res, out) = cmd
            .run_capturing_to(dir.path(), Duration::from_millis(300))
            .await
            .expect("run");
        assert_eq!(res, VerifyResult::Failed, "卡住的 verify 应判失败");
        assert!(out.contains("超过"), "应说明是超时: {out:?}");
        assert!(started.elapsed() < Duration::from_secs(5), "应在超时后立刻返回,不等命令跑完");
    }
}
