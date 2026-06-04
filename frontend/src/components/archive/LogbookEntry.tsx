import type { AgentEvent } from '@/types/agentEvent.types';
import { EVENT_KIND_LABEL } from '@/strings';
import { EVENT_KIND_GLYPH, describeEvent, formatCost } from '@/utils/eventFormat';

interface LogbookEntryProps {
  event: AgentEvent;
  tsMs: number;
}

function clockLabel(tsMs: number): string {
  return new Date(tsMs).toLocaleTimeString();
}

// Per-kind extra detail rendered below the one-line description, so the Logbook
// reads as a rich record rather than a flat list. output_chunk text gets its own
// block; tool_use shows tool + summary distinctly; result/finished show the
// numeric facts. Distinct CSS hooks let each kind style differently.
function detail(event: AgentEvent): React.ReactNode {
  switch (event.kind) {
    case 'worker_started':
      return (
        <div className="logbook-row-detail">
          <span className="logbook-kv">模型 {event.model ?? '未知'}</span>
          <span className="logbook-kv">鉴权 {event.authMode}</span>
          <span className="logbook-kv">runner {event.runner}</span>
        </div>
      );
    case 'tool_use':
      return (
        <div className="logbook-row-detail">
          <span className="logbook-tool">{event.tool}</span>
          <code className="logbook-summary">{event.summary}</code>
        </div>
      );
    case 'output_chunk':
      return <pre className="logbook-output">{event.text}</pre>;
    case 'result':
      return (
        <div className="logbook-row-detail">
          <span className="logbook-kv">ok={String(event.ok)}</span>
          <span className="logbook-kv">轮次 {event.numTurns}</span>
          <span className="logbook-kv">花费 {formatCost(event.costUsd)}</span>
        </div>
      );
    case 'error':
      return (
        <div className="logbook-row-detail logbook-row-detail-error">
          <span className="logbook-kv">[{event.code}]</span>
          <span className="logbook-summary">{event.message}</span>
        </div>
      );
    case 'finished':
      return (
        <div className="logbook-row-detail">
          <span className="logbook-kv">状态 {event.status}</span>
          <span className="logbook-kv">总花费 {formatCost(event.costUsd)}</span>
          {event.branch && <span className="logbook-kv">分支 {event.branch}</span>}
        </div>
      );
  }
}

/**
 * One event in the Logbook timeline: glyph + kind label + clock + a one-line
 * description, then a kind-specific detail block (the verbatim output text, the
 * tool command, the result numbers). Grouped/iconed by kind, reusing the live
 * event-kind labels/glyphs.
 */
export function LogbookEntry({ event, tsMs }: LogbookEntryProps) {
  return (
    <li className={`logbook-row logbook-row-${event.kind}`}>
      <div className="logbook-row-head">
        <span className="logbook-row-glyph">{EVENT_KIND_GLYPH[event.kind]}</span>
        <span className="logbook-row-kind">{EVENT_KIND_LABEL[event.kind]}</span>
        <span className="logbook-row-time">{clockLabel(tsMs)}</span>
      </div>
      {event.kind !== 'output_chunk' && (
        <p className="logbook-row-desc" title={describeEvent(event)}>
          {describeEvent(event)}
        </p>
      )}
      {detail(event)}
    </li>
  );
}
