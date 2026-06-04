import { useEffect, useMemo, useState } from 'react';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import type { StoredEvent, TaskRecord } from '@/types/persistence.types';
import type { AgentEvent } from '@/types/agentEvent.types';
import { formatCost, parseStoredPayload } from '@/utils/eventFormat';
import { LogbookEntry } from '@/components/archive/LogbookEntry';

interface LogbookViewerProps {
  record: TaskRecord;
  loadEvents: (taskId: string) => Promise<StoredEvent[]>;
  onClose: () => void;
  onReplay?: (events: StoredEvent[]) => void;
}

interface ParsedEvent {
  seq: number;
  tsMs: number;
  event: AgentEvent;
}

function projectName(project: string): string {
  const parts = project.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? project;
}

function durationLabel(ms: number): string {
  if (ms <= 0) return '—';
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${Math.round(s - m * 60)}s`;
}

/**
 * The 卷轴 / Logbook: a parchment scroll replaying ONE run's complete I/O from
 * the §11 event log. Header shows the commission (prompt, project, mode, status,
 * branch) + totals (events, cost, duration); the body is the ordered timeline of
 * tool calls, output chunks, the result, and the finished marker. Loaded lazily
 * via `loadEvents` when opened. Optionally offers a "回放" hand-off to the
 * workshop (juice — never runs real claude).
 */
export function LogbookViewer({ record, loadEvents, onClose, onReplay }: LogbookViewerProps) {
  const [raw, setRaw] = useState<StoredEvent[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setRaw(null);
    setError(null);
    loadEvents(record.id)
      .then(events => {
        if (active) setRaw(events);
      })
      .catch(e => {
        if (active) setError(typeof e === 'string' ? e : String(e));
      });
    return () => {
      active = false;
    };
  }, [record.id, loadEvents]);

  // Close on Escape — a parchment scroll should roll up with one key.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const parsed = useMemo<ParsedEvent[]>(() => {
    if (!raw) return [];
    const out: ParsedEvent[] = [];
    for (const row of raw) {
      const event = parseStoredPayload(row.payloadJson);
      if (event) out.push({ seq: row.seq, tsMs: row.tsMs, event });
    }
    return out;
  }, [raw]);

  const totals = useMemo(() => {
    if (parsed.length === 0) return { count: 0, cost: null as number | null, duration: 0 };
    // Prefer the terminal cost carried on finished/result; fall back to null.
    let cost: number | null = null;
    for (const p of parsed) {
      if (p.event.kind === 'finished' && p.event.costUsd != null) cost = p.event.costUsd;
      else if (p.event.kind === 'result' && p.event.costUsd != null && cost == null)
        cost = p.event.costUsd;
    }
    const first = parsed[0].tsMs;
    const last = parsed[parsed.length - 1].tsMs;
    return { count: parsed.length, cost, duration: last - first };
  }, [parsed]);

  const modeLabel = record.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate;
  const statusLabel = TASK_STATUS_LABEL[record.status] ?? record.status;

  return (
    <div className="logbook-overlay" role="dialog" aria-modal="true" onClick={onClose}>
      <div className="logbook-scroll" onClick={e => e.stopPropagation()}>
        <header className="logbook-head">
          <div className="logbook-head-text">
            <h2 className="logbook-title">{STR.logbookTitle}</h2>
            <div className="logbook-totals">
              <span className="logbook-total">
                {STR.logbookTotalEvents} {totals.count}
              </span>
              <span className="logbook-total">
                {STR.logbookTotalCost} {formatCost(totals.cost)}
              </span>
              <span className="logbook-total">
                {STR.logbookTotalDuration} {durationLabel(totals.duration)}
              </span>
            </div>
          </div>
          <div className="logbook-head-actions">
            {onReplay && raw && raw.length > 0 && (
              <button
                type="button"
                className="logbook-replay"
                title={STR.logbookReplayTip}
                onClick={() => onReplay(raw)}
              >
                {STR.logbookReplay}
              </button>
            )}
            <button
              type="button"
              className="logbook-close"
              aria-label={STR.logbookCloseAria}
              onClick={onClose}
            >
              {STR.logbookClose}
            </button>
          </div>
        </header>

        <div className="logbook-commission">
          <div className="logbook-field">
            <span className="logbook-field-label">{STR.logbookPromptLabel}</span>
            <p className="logbook-field-prompt">{record.prompt}</p>
          </div>
          <div className="logbook-field-grid">
            <span className="logbook-field-label">{STR.logbookProjectLabel}</span>
            <span className="logbook-field-value" title={record.project}>
              {projectName(record.project)}
            </span>
            <span className="logbook-field-label">{STR.logbookModeLabel}</span>
            <span className="logbook-field-value">{modeLabel}</span>
            <span className="logbook-field-label">{STR.logbookStatusLabel}</span>
            <span className="logbook-field-value">{statusLabel}</span>
            {record.branch && (
              <>
                <span className="logbook-field-label">{STR.logbookBranchLabel}</span>
                <span className="logbook-field-value" title={record.branch}>
                  {record.branch}
                </span>
              </>
            )}
          </div>
        </div>

        <div className="logbook-body">
          <h3 className="logbook-timeline-title">{STR.logbookTimelineTitle}</h3>
          {error ? (
            <p className="app-error">{STR.logbookError}</p>
          ) : raw === null ? (
            <p className="logbook-loading">{STR.logbookLoading}</p>
          ) : parsed.length === 0 ? (
            <p className="logbook-empty">{STR.logbookEmpty}</p>
          ) : (
            <ul className="logbook-timeline">
              {parsed.map(p => (
                <LogbookEntry key={p.seq} event={p.event} tsMs={p.tsMs} />
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
