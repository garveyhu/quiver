pub mod adapter;
pub mod parse;

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::event::{AgentEvent, RunnerKind};
use crate::runner::{AgentRunner, SpawnedAgent};
use crate::runner::claude::adapter::ClaudeAdapter;
use crate::runner::claude::parse::parse_line;

/// Bounded buffer for the event channel — enough headroom for bursty output
/// without unbounded memory growth if the consumer lags.
const EVENT_CHANNEL_CAP: usize = 256;

/// Env vars passed through to the child (DESIGN §9.3): a minimal allowlist on top
/// of `env_clear()`. Locale (`LANG` / `LC_*`) is forwarded separately below.
///
/// `QUIVER_FAKE_DELAY_MS` is forwarded so the `fake-claude` test double can pace
/// its NDJSON output for a believable LIVE simulate run (it is inert for the real
/// `claude` binary, which ignores unknown env). It carries no auth/route meaning,
/// so forwarding it does not weaken the §9.3 subscription guard.
const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "USER", "TERM", "QUIVER_FAKE_DELAY_MS"];

/// Spawns the official `claude` CLI and streams its `stream-json` stdout as
/// normalized [`AgentEvent`]s (DESIGN §4.2 mode 1, §5.3). Subscription auth is
/// the soul path; the OAuth-route assertion (§9.3) is stubbed in Phase 0.
///
/// Extra CLI args (`--permission-mode`, `--model`, …) are configurable so the
/// Tauri shell can pass real-mode flags without the `fake-claude` test double
/// needing to understand them (it scans-and-ignores unknown args).
pub struct ClaudeRunner {
    task_id: String,
    extra_args: Vec<String>,
    /// 是否把 claude 进程关进沙箱(§8.3 for_worker:留网、禁读密钥)。默认 `true`。
    /// 经理大脑/图书管理员这类**只读思考**的轻量调用可关(它们不改 worktree、cwd 是 repo 根)。
    sandbox: bool,
}

impl ClaudeRunner {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            extra_args: Vec::new(),
            sandbox: true,
        }
    }

    /// 关掉沙箱包裹(经理大脑/图书管理员等只读思考调用用 —— 它们 cwd 是 repo 根、不改码,
    /// 包 for_worker 反而会因 cwd≠worktree 误伤)。
    pub fn without_sandbox(mut self) -> Self {
        self.sandbox = false;
        self
    }

    /// Append extra CLI args (e.g. `["--permission-mode", "acceptEdits",
    /// "--model", "sonnet"]`) passed verbatim after the base flags.
    pub fn with_extra_args(
        mut self,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.extra_args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Build the base `claude` invocation (flags + env allowlist + piped stdout)
    /// shared by [`spawn`](AgentRunner::spawn) and [`resume`](AgentRunner::resume).
    fn base_command(&self, prompt: &str, cwd: &Path, bin: &Path) -> Command {
        // claude 的固定调用参数(沙箱包裹时作为 sandbox-exec 之后的"原命令")。
        let mut claude_args: Vec<String> = vec![
            "-p".into(),
            prompt.into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
        ];
        claude_args.extend(self.extra_args.iter().cloned());

        // §8.3 worker 沙箱:macOS + sandbox 开 → 把 claude 关进 for_worker 策略(留网、禁读
        // 密钥)。其它平台/关沙箱 → 直起 claude。`program/argv` 二选一拼好,统一 spawn。
        let bin_str = bin.to_string_lossy().to_string();
        let policy = crate::sandbox::SandboxPolicy::for_worker(cwd);
        let mut command = if self.sandbox && crate::sandbox::SandboxPolicy::is_supported() {
            let (prog, args) = policy.wrap(&bin_str, &claude_args);
            let mut c = Command::new(prog);
            c.args(args);
            c
        } else {
            let mut c = Command::new(bin);
            c.args(&claude_args);
            c
        };
        command
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        apply_env_allowlist(&mut command);
        // Put the child in its OWN process group (pgid == child pid). On cancel the
        // app signals the negative pid → the whole group dies, reaping any
        // grandchildren the agent spawned. Without this, killing only the agent pid
        // would orphan its children (DESIGN §23 P3 killpg). Unix-only.
        #[cfg(unix)]
        command.process_group(0);
        command
    }

    /// Spawn an already-built `command` and drain its `stream-json` stdout into a
    /// normalized [`AgentEvent`] channel. `bin` is only used for the error message.
    /// Returns the event stream + the child PID for cancellation.
    fn drive(&self, mut command: Command, bin: &Path) -> Result<SpawnedAgent> {
        // bin 是路径却不可执行 → spawn 失败语义(SpawnFailed)。必须在(可能的)sandbox-exec
        // 包裹之前判:否则 `sandbox-exec` 自己 spawn 成功、目标 bin 不存在只会让它退出无输出,
        // 把"agent 二进制缺失"误吞成 NoResult。沙箱包裹不该模糊这个错误来源。
        if bin.to_string_lossy().contains('/') && !is_executable_file(bin) {
            anyhow::bail!("agent binary is not executable: {}", bin.display());
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn agent binary {}", bin.display()))?;

        let stdout = child
            .stdout
            .take()
            .context("child stdout was not captured")?;
        // Capture the PID before the child moves into the reader task — the app
        // kills this to cancel a running task (stdout EOF then ends the stream).
        let pid = child.id();

        let (tx, rx) = mpsc::channel::<AgentEvent>(EVENT_CHANNEL_CAP);
        let task_id = self.task_id.clone();

        tokio::spawn(async move {
            let mut adapter = ClaudeAdapter::new(task_id);
            let mut lines = BufReader::new(stdout).lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(raw) = parse_line(&line) {
                    let event = adapter.adapt(raw, now_ms());
                    if tx.send(event).await.is_err() {
                        break; // receiver dropped — stop reading
                    }
                }
            }

            // Reap the child so it doesn't linger as a zombie. tx drops here →
            // the receiver observes end-of-stream.
            let _ = child.wait().await;
        });

        Ok(SpawnedAgent { events: rx, pid })
    }
}

#[async_trait]
impl AgentRunner for ClaudeRunner {
    async fn spawn(&self, prompt: &str, cwd: &Path, bin: &Path) -> Result<SpawnedAgent> {
        self.drive(self.base_command(prompt, cwd, bin), bin)
    }

    async fn resume(
        &self,
        session_id: &str,
        prompt: &str,
        cwd: &Path,
        bin: &Path,
    ) -> Result<SpawnedAgent> {
        let mut command = self.base_command(prompt, cwd, bin);
        command.arg("--resume").arg(session_id);
        self.drive(command, bin)
    }

    fn kind(&self) -> RunnerKind {
        RunnerKind::ClaudeCli
    }
}

/// 路径是否指向一个可执行文件(存在 + 普通文件 + 任一执行位)。用于在沙箱包裹前甄别
/// "agent 二进制缺失"。非 Unix 退化为"存在即可"。
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

/// `env_clear()` then re-add only the allowlisted vars that are actually set in
/// the parent, plus any locale vars (`LANG`, `LC_*`).
fn apply_env_allowlist(command: &mut Command) {
    command.env_clear();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    for (key, value) in std::env::vars() {
        if key == "LANG" || key.starts_with("LC_") {
            command.env(key, value);
        }
    }
}

/// Supervisor wall-clock in milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
