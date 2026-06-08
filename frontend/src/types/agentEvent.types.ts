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
    // sessionId 是后端 session 句柄(Claude --resume token),用于经理持久续接 worker 上下文。
    | { kind: 'worker_started'; sessionId: string | null; model: string | null; authMode: AuthMode }
    | { kind: 'tool_use'; tool: string; summary: string }
    | { kind: 'output_chunk'; text: string }
    | {
        kind: 'result';
        ok: boolean;
        costUsd: number | null;
        numTurns: number;
        // 由 quiver-core 从 claude result 行的 usage / duration_ms 透出(可选,旧流没有)。
        tokens?: number | null;
        durationMs?: number | null;
      }
    | { kind: 'error'; code: string; message: string }
    // Synthesized by the Tauri command after run_task returns, carrying the
    // terminal FinishStatus (Verified | VerifyFailed | Failed | NeedsRebase).
    // `branch` is set only when the work was left un-merged on a branch
    // (real-mode safety). Not a quiver-core AgentEvent variant — see
    // src-tauri/src/lib.rs.
    | {
        kind: 'finished';
        status: string;
        costUsd: number | null;
        branch: string | null;
        // verify-gate 输出(尾部),仅 VerifyFailed 时有 —— 让 UI 显示为何没过。
        verifyOutput?: string | null;
      }
  );

export type AgentEventKind = AgentEvent['kind'];
