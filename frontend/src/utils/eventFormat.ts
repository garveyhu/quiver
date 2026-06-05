import type { AgentEvent } from '@/types/agentEvent.types';

// Shared presentation helpers for a single AgentEvent, reused by the live event
// row (工坊) and the Logbook viewer (卷轴). Keeping the kind→glyph map and the
// human description in one place means both surfaces stay in lockstep.

// The §5.5 sprite-state precursor: a glyph hinting which pixel-worker state this
// event would drive. Purely presentational — no logic beyond the mapping.
export const EVENT_KIND_GLYPH: Record<AgentEvent['kind'], string> = {
  worker_started: '🟢',
  tool_use: '🔧',
  output_chunk: '💬',
  result: '💰',
  error: '🛑',
  finished: '🏁',
};

export function formatCost(cost: number | null): string {
  return cost === null ? '未统计' : `$${cost.toFixed(4)}`;
}

/** A one-line human description of an event, in the same voice as the live log. */
export function describeEvent(event: AgentEvent): string {
  switch (event.kind) {
    case 'worker_started':
      return `工人就位 · 模型 ${event.model ?? '未知'} · 鉴权 ${event.authMode}`;
    case 'tool_use':
      return `${event.tool}：${event.summary}`;
    case 'output_chunk':
      return event.text;
    case 'result':
      return `结果 ok=${event.ok} · 轮次 ${event.numTurns} · 花费 ${formatCost(event.costUsd)}`;
    case 'error':
      return `[${event.code}] ${event.message}`;
    case 'finished': {
      const base = `完成：${event.status} · 总花费 ${formatCost(event.costUsd)}`;
      return event.branch ? `${base} · 产物留在分支 ${event.branch}（未并入 main）` : base;
    }
  }
}

/**
 * Parse a `StoredEvent.payloadJson` (verbatim flat camelCase AgentEvent JSON)
 * into a typed `AgentEvent`. Returns `null` if the payload is malformed so the
 * Logbook can skip a bad row rather than crash the whole replay.
 */
export function parseStoredPayload(payloadJson: string): AgentEvent | null {
  try {
    return JSON.parse(payloadJson) as AgentEvent;
  } catch {
    return null;
  }
}

/** A HH:MM:SS clock stamp for one event's wall time (transcript gutter). */
export function clockStamp(tsMs: number): string {
  const d = new Date(tsMs);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/**
 * The dense, scan-readable body for one transcript line in the Logbook scroll
 * (the §6 two-layer rule: the scroll is a diegetic frame, but its contents are a
 * clean monospace log — never handwritten parchment script). Returns the line's
 * head (kind + key facts) and an optional verbatim block (output text) so the
 * scene can render the block in a distinct ink.
 */
export interface TranscriptLine {
  glyph: string;
  /** Short head: kind label + inline facts (tool name, ok/turns/cost, status). */
  head: string;
  /** Verbatim multi-line output (only for output_chunk / error message). */
  block: string | null;
  /** Drives the per-kind accent stripe + block ink. */
  kind: AgentEvent['kind'];
}

export function transcriptLine(event: AgentEvent): TranscriptLine {
  const glyph = EVENT_KIND_GLYPH[event.kind];
  switch (event.kind) {
    case 'worker_started':
      return {
        glyph,
        head: `worker_started  model=${event.model ?? '—'}  auth=${event.authMode}  runner=${event.runner}`,
        block: null,
        kind: event.kind,
      };
    case 'tool_use':
      return {
        glyph,
        head: `tool_use ${event.tool}  ${event.summary}`,
        block: null,
        kind: event.kind,
      };
    case 'output_chunk':
      return { glyph, head: 'output', block: event.text, kind: event.kind };
    case 'result':
      return {
        glyph,
        head: `result  ok=${event.ok}  turns=${event.numTurns}  cost=${formatCost(event.costUsd)}`,
        block: null,
        kind: event.kind,
      };
    case 'error':
      return {
        glyph,
        head: `error [${event.code}]`,
        block: event.message,
        kind: event.kind,
      };
    case 'finished': {
      const branch = event.branch ? `  branch=${event.branch}` : '';
      return {
        glyph,
        head: `finished  ${event.status}  cost=${formatCost(event.costUsd)}${branch}`,
        block: null,
        kind: event.kind,
      };
    }
  }
}
