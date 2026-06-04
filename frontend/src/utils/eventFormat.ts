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
