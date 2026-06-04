//! Demo: spawn one agent run via `ClaudeRunner` and print each normalized
//! `AgentEvent` as a single JSON line — "watch the soul work" (DESIGN Phase 0).
//!
//! Usage:
//!   cargo run -p quiver-core --example run_once -- [BIN] [PROMPT]
//!
//! BIN defaults to "claude" (the real, PAID CLI — do NOT run that here unless you
//! mean to spend credit). For a free, deterministic run, point it at the built
//! fake-claude binary, e.g.:
//!   cargo build -p fake-claude
//!   cargo run -p quiver-core --example run_once -- ./target/debug/fake-claude "hello"

use std::path::PathBuf;

use quiver_core::runner::AgentRunner;
use quiver_core::runner::claude::ClaudeRunner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let bin: PathBuf = args.next().unwrap_or_else(|| "claude".to_string()).into();
    let prompt = args
        .next()
        .unwrap_or_else(|| "Read README.md and reply with one sentence.".to_string());
    let cwd = std::env::current_dir()?;

    let runner = ClaudeRunner::new("run-once");
    let mut rx = runner.spawn(&prompt, &cwd, &bin).await?;

    while let Some(event) = rx.recv().await {
        println!("{}", serde_json::to_string(&event)?);
    }

    Ok(())
}
