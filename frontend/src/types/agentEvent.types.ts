// Wire shape of a normalized AgentEvent as serialized by quiver-core
// (crates/quiver-core/src/event.rs): camelCase, flat, tagged by `kind`.
// This is the load-bearing contract between the Rust core and this UI — it must
// stay in lockstep with the Rust `#[serde(tag = "kind", rename_all = "snake_case",
// rename_all_fields = "camelCase")]` enum.

export type RunnerKind = 'claude_cli' | 'codex_cli' | 'anthropic_api';

export type AuthMode = 'subscription' | 'apiKey' | 'openAiEndpoint';

interface AgentEventEnvelope {
  taskId: string;
  seq: number;
  tsMs: number;
  runner: RunnerKind;
}

export type AgentEvent = AgentEventEnvelope &
  (
    | { kind: 'worker_started'; model: string | null; authMode: AuthMode }
    | { kind: 'tool_use'; tool: string; summary: string }
    | { kind: 'output_chunk'; text: string }
    | { kind: 'result'; ok: boolean; costUsd: number | null; numTurns: number }
    | { kind: 'error'; code: string; message: string }
    // Synthesized by the Tauri command after run_task returns, carrying the
    // terminal FinishStatus (Verified | VerifyFailed | Failed | NeedsRebase).
    // Not a quiver-core AgentEvent variant — see src-tauri/src/lib.rs.
    | { kind: 'finished'; status: string; costUsd: number | null }
  );

export type AgentEventKind = AgentEvent['kind'];
