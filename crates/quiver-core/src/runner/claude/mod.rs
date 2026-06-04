pub mod adapter;
pub mod parse;

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::{self, Receiver};

use crate::event::{AgentEvent, RunnerKind};
use crate::runner::AgentRunner;
use crate::runner::claude::adapter::ClaudeAdapter;
use crate::runner::claude::parse::parse_line;

/// Bounded buffer for the event channel — enough headroom for bursty output
/// without unbounded memory growth if the consumer lags.
const EVENT_CHANNEL_CAP: usize = 256;

/// Env vars passed through to the child (DESIGN §9.3): a minimal allowlist on top
/// of `env_clear()`. Locale (`LANG` / `LC_*`) is forwarded separately below.
const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "USER", "TERM"];

/// Spawns the official `claude` CLI and streams its `stream-json` stdout as
/// normalized [`AgentEvent`]s (DESIGN §4.2 mode 1, §5.3). Subscription auth is
/// the soul path; the OAuth-route assertion (§9.3) is stubbed in Phase 0.
pub struct ClaudeRunner {
    task_id: String,
}

impl ClaudeRunner {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
        }
    }
}

#[async_trait]
impl AgentRunner for ClaudeRunner {
    async fn spawn(&self, prompt: &str, cwd: &Path, bin: &Path) -> Result<Receiver<AgentEvent>> {
        let mut command = Command::new(bin);
        command
            .arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        apply_env_allowlist(&mut command);

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn agent binary {}", bin.display()))?;

        let stdout = child
            .stdout
            .take()
            .context("child stdout was not captured")?;

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

        Ok(rx)
    }

    fn kind(&self) -> RunnerKind {
        RunnerKind::ClaudeCli
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
