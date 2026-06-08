pub mod claude;

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;

use crate::event::{AgentEvent, RunnerKind};

/// A spawned agent: the normalized event stream + the child PID (for cancellation
/// — the supervisor/app can kill this PID to stop a running task; stdout EOF then
/// ends the stream through the normal path). `pid` is `None` if unavailable.
pub struct SpawnedAgent {
    pub events: Receiver<AgentEvent>,
    pub pid: Option<u32>,
}

/// The ONLY backend-specific surface (DESIGN §4.1). One impl per backend.
///
/// An implementor spawns the agent CLI as a child process and normalizes its
/// stdout into a stream of [`AgentEvent`]s, delivered over an mpsc channel. The
/// sender is dropped on the child's stdout EOF, so the receiver closing is the
/// run's end-of-stream signal.
#[async_trait]
pub trait AgentRunner: Send + Sync {
    /// Spawn the agent for `prompt` in working directory `cwd`, executing the
    /// binary at `bin` (configurable so tests can point it at `fake-claude`).
    /// Returns the normalized event stream + the child PID for cancellation.
    async fn spawn(
        &self,
        prompt: &str,
        cwd: &Path,
        bin: &Path,
    ) -> Result<SpawnedAgent>;

    /// Which kind, for provenance/logging and the `RunnerKind` stamp on events.
    fn kind(&self) -> RunnerKind;
}
