import type { StoredEvent } from '@/types/persistence.types';
import type { AgentEvent } from '@/types/agentEvent.types';
import { parseStoredPayload } from '@/utils/eventFormat';

/** One parsed transcript row: the stored seq / wall time + its typed event. */
export interface ParsedEvent {
  seq: number;
  tsMs: number;
  event: AgentEvent;
}

/** The run-level totals shown in the Logbook head, derived from the events. */
export interface LogbookTotals {
  count: number;
  cost: number | null;
  durationMs: number;
  turns: number | null;
}

/**
 * Parse a run's stored log into typed transcript rows, skipping any malformed
 * payload (so one bad row never blanks the whole scroll).
 */
export function parseStoredEvents(events: StoredEvent[]): ParsedEvent[] {
  const parsed: ParsedEvent[] = [];
  for (const row of events) {
    const event = parseStoredPayload(row.payloadJson);
    if (event) parsed.push({ seq: row.seq, tsMs: row.tsMs, event });
  }
  return parsed;
}

/**
 * The run-level totals for the commission head, computed from the stored log:
 * cost prefers a `finished` figure, falls back to `result`, then to the record's
 * `fallbackCost`; duration is the first→last wall time; turns comes from `result`.
 */
export function logbookTotals(events: StoredEvent[], fallbackCost: number | null): LogbookTotals {
  const parsed = parseStoredEvents(events);
  if (parsed.length === 0) {
    return { count: 0, cost: fallbackCost, durationMs: 0, turns: null };
  }
  let cost: number | null = null;
  let turns: number | null = null;
  for (const p of parsed) {
    if (p.event.kind === 'finished' && p.event.costUsd != null) {
      cost = p.event.costUsd;
    } else if (p.event.kind === 'result') {
      if (p.event.costUsd != null && cost == null) cost = p.event.costUsd;
      turns = p.event.numTurns;
    }
  }
  if (cost == null) cost = fallbackCost;
  const first = parsed[0].tsMs;
  const last = parsed[parsed.length - 1].tsMs;
  return { count: parsed.length, cost, durationMs: last - first, turns };
}

/** Human duration label (e.g. `12.3s` / `2m 5s`). `—` when unknown / zero. */
export function durationLabel(ms: number): string {
  if (ms <= 0) return '—';
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${Math.round(s - m * 60)}s`;
}

/** Last path segment of a project dir (the shelf / scroll show the leaf name). */
export function projectLeaf(project: string): string {
  const parts = project.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? project;
}
