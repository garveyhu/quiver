import type { AgentEvent } from '@/types/agentEvent.types';

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
      return `worker started · model ${event.model ?? 'unknown'} · auth ${event.authMode}`;
    case 'tool_use':
      return `${event.tool}: ${event.summary}`;
    case 'output_chunk':
      return event.text;
    case 'result':
      return `result ok=${event.ok} · turns ${event.numTurns} · cost ${formatCost(event.costUsd)}`;
    case 'error':
      return `[${event.code}] ${event.message}`;
    case 'finished':
      return `finished: ${event.status} · total cost ${formatCost(event.costUsd)}`;
  }
}

function formatCost(cost: number | null): string {
  return cost === null ? 'untracked' : `$${cost.toFixed(4)}`;
}

export function EventRow({ event }: EventRowProps) {
  return (
    <li className="event-row">
      <span className="event-glyph">{KIND_GLYPH[event.kind]}</span>
      <span className="event-kind">{event.kind}</span>
      <span className="event-desc">{describe(event)}</span>
    </li>
  );
}
