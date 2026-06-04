import type { AgentEvent } from '@/types/agentEvent.types';
import { EVENT_KIND_LABEL } from '@/strings';

interface EventRowProps {
  event: AgentEvent;
}

// The §5.5 sprite-state precursor: a glyph hinting which pixel-worker state this
// event would drive. Purely presentational — no logic beyond the mapping.
const KIND_GLYPH: Record<AgentEvent['kind'], string> = {
  worker_started: '🟢',
  tool_use: '🔧',
  output_chunk: '💬',
  result: '💰',
  error: '🛑',
  finished: '🏁',
};

function describe(event: AgentEvent): string {
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

function formatCost(cost: number | null): string {
  return cost === null ? '未统计' : `$${cost.toFixed(4)}`;
}

export function EventRow({ event }: EventRowProps) {
  return (
    <li className="event-row">
      <span className="event-glyph">{KIND_GLYPH[event.kind]}</span>
      <span className="event-kind">{EVENT_KIND_LABEL[event.kind]}</span>
      <span className="event-desc">{describe(event)}</span>
    </li>
  );
}
