import { useCallback, useEffect, useMemo, useState } from 'react';
import { EventBus, BUS, type TaskRecord, type ArchiveMeta } from '@/game/EventBus';
import { projectLeaf } from '@/game/logbookState';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import { play } from '@/utils/sound';

/**
 * The 委托档案库 / run archive as a real React DOM overlay (replaces the old Phaser
 * ArchiveScene). Same §6 two-layer rule as the Logbook: a cozy parchment/timber
 * skin (the shared `--scroll-*` CSS tokens) wraps a HIGH-DENSITY, scan-readable
 * run list rendered in a native `<div overflow:auto>`.
 *
 * Why DOM, not a hand-painted Phaser list + GeometryMask: the browser's own
 * hover hit-testing is pixel-exact (the old canvas list mis-highlighted — hover
 * row 1, row 2 lit), and native overflow NEVER overruns its container at any
 * window size (the old list spilled the shelf on large windows). Both bugs are
 * structurally impossible here.
 *
 * Data flow is unchanged: the GameBridge owns the IPC (useArchive.records, the
 * §11 list) and pushes them on `BUS.archiveRecords` / `BUS.archiveMeta`. This
 * component only renders + filters them client-side (the SAME predicate the old
 * scene used) and, on a row click, hands the run to the existing DOM Logbook over
 * `BUS.openLogbook` (which the bridge satisfies via useArchive.loadEvents).
 */

const ALL = '__all__';

function costLabel(cost: number | null): string {
  return cost === null ? STR.archiveOverlayCostUnknown : `${STR.archiveOverlayCostPrefix}${cost.toFixed(2)}`;
}

function durationHint(record: TaskRecord): string {
  const ms = Math.max(0, record.updatedAt - record.createdAt);
  if (ms <= 0) return '';
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(0)}s`;
  const m = Math.floor(s / 60);
  return `${m}m${Math.round(s - m * 60)}s`;
}

// Per-status accent (the spine colour). Keyed off the Rust task.status vocabulary,
// resolved against the shared --kind-* / status CSS tokens via a class.
function statusClass(status: string): string {
  switch (status) {
    case 'queued':
      return 'archive-status-queued';
    case 'running':
      return 'archive-status-running';
    case 'verifying':
      return 'archive-status-verifying';
    case 'verified':
    case 'done':
      return 'archive-status-done';
    default:
      return 'archive-status-failed';
  }
}

export function ArchiveOverlay() {
  const [open, setOpen] = useState(false);
  const [records, setRecords] = useState<TaskRecord[]>([]);
  const [meta, setMeta] = useState<ArchiveMeta>({ error: null });

  const [query, setQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>(ALL);
  const [projectFilter, setProjectFilter] = useState<string>(ALL);

  // Subscribe to the open/close commands + the archive snapshots the bridge pushes.
  useEffect(() => {
    const onOpen = () => {
      EventBus.emit(BUS.sceneReady); // ask the bridge to flush the current snapshot
      setOpen(true);
    };
    const onClose = () => setOpen(false);
    const onRecords = (next: TaskRecord[]) => setRecords(next);
    const onMeta = (next: ArchiveMeta) => setMeta(next);
    EventBus.on(BUS.openArchive, onOpen);
    EventBus.on(BUS.closeArchive, onClose);
    EventBus.on(BUS.archiveRecords, onRecords);
    EventBus.on(BUS.archiveMeta, onMeta);
    return () => {
      EventBus.off(BUS.openArchive, onOpen);
      EventBus.off(BUS.closeArchive, onClose);
      EventBus.off(BUS.archiveRecords, onRecords);
      EventBus.off(BUS.archiveMeta, onMeta);
    };
  }, []);

  const close = useCallback(() => {
    play('close');
    EventBus.emit(BUS.closeArchive);
  }, []);

  // Escape closes the shelf (parity with the world's other overlays).
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, close]);

  // The distinct status / project values present, to populate the filter dropdowns.
  const statuses = useMemo(() => [...new Set(records.map(r => r.status))], [records]);
  const projects = useMemo(() => [...new Set(records.map(r => r.project))], [records]);

  // Client-side search + filter — the SAME predicate the old scene used (status /
  // project exact match + prompt substring).
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return records.filter(r => {
      if (statusFilter !== ALL && r.status !== statusFilter) return false;
      if (projectFilter !== ALL && r.project !== projectFilter) return false;
      if (q && !r.prompt.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [records, query, statusFilter, projectFilter]);

  const openBook = useCallback((record: TaskRecord) => {
    play('open');
    EventBus.emit(BUS.openLogbook, {
      meta: {
        taskId: record.id,
        prompt: record.prompt,
        project: record.project,
        mode: record.mode,
        status: record.status,
        branch: record.branch,
        costUsd: record.costUsd,
      },
      from: 'archive',
    });
  }, []);

  if (!open) return null;

  return (
    <div className="archive-overlay" role="dialog" aria-modal="true" aria-label={STR.archiveOverlayTitle}>
      <div className="archive-scrim" onClick={close} role="presentation" />
      <div className="archive-panel" role="document">
        <header className="archive-header">
          <div className="archive-headline">
            <h2 className="archive-title">{STR.archiveOverlayTitle}</h2>
            <button
              type="button"
              className="archive-close"
              onClick={close}
              aria-label={STR.archiveOverlayCloseAria}
            >
              {STR.archiveOverlayClose}
            </button>
          </div>
          <p className="archive-subtitle">{STR.archiveOverlaySubtitle}</p>

          <div className="archive-controls">
            <div className="archive-search">
              <span className="archive-search-glyph" aria-hidden="true">
                🔍
              </span>
              <input
                type="text"
                className="archive-search-input"
                value={query}
                placeholder={STR.archiveOverlaySearchPlaceholder}
                aria-label={STR.archiveOverlaySearchAria}
                onChange={e => setQuery(e.target.value)}
              />
              {query.trim() !== '' ? (
                <button
                  type="button"
                  className="archive-search-clear"
                  onClick={() => setQuery('')}
                  aria-label={STR.archiveOverlaySearchClear}
                  title={STR.archiveOverlaySearchClear}
                >
                  ✕
                </button>
              ) : null}
            </div>

            <select
              className="archive-filter"
              value={statusFilter}
              onChange={e => setStatusFilter(e.target.value)}
              aria-label={STR.archiveOverlayFilterStatusAll}
            >
              <option value={ALL}>{STR.archiveOverlayFilterStatusAll}</option>
              {statuses.map(s => (
                <option key={s} value={s}>
                  {TASK_STATUS_LABEL[s] ?? s}
                </option>
              ))}
            </select>

            <select
              className="archive-filter"
              value={projectFilter}
              onChange={e => setProjectFilter(e.target.value)}
              aria-label={STR.archiveOverlayFilterProjectAll}
            >
              <option value={ALL}>{STR.archiveOverlayFilterProjectAll}</option>
              {projects.map(p => (
                <option key={p} value={p}>
                  {projectLeaf(p)}
                </option>
              ))}
            </select>

            <span className="archive-count">
              {STR.archiveOverlayCountPrefix}
              {filtered.length}
              {STR.archiveOverlayCountSuffix}
            </span>
          </div>
        </header>

        <div className="archive-list" tabIndex={0}>
          {meta.error ? (
            <p className="archive-status archive-status-error">{meta.error}</p>
          ) : records.length === 0 ? (
            <p className="archive-status">{STR.archiveOverlayEmpty}</p>
          ) : filtered.length === 0 ? (
            <p className="archive-status">{STR.archiveOverlayNoMatch}</p>
          ) : (
            filtered.map(record => (
              <ArchiveRow key={record.id} record={record} onOpen={openBook} />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

interface RowProps {
  record: TaskRecord;
  onOpen: (record: TaskRecord) => void;
}

// One run-book row: a status-coloured spine, the prompt as the title, and a meta
// foot (status · mode · project · cost · time · duration · branch). Native hover
// highlight = pixel-exact hit-testing (the bug the canvas list had).
function ArchiveRow({ record, onOpen }: RowProps) {
  const statusLabel = TASK_STATUS_LABEL[record.status] ?? record.status;
  const modeLabel = record.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate;
  const dur = durationHint(record);

  const footParts = [statusLabel, modeLabel, projectLeaf(record.project), costLabel(record.costUsd)];
  footParts.push(new Date(record.createdAt).toLocaleString());
  if (dur) footParts.push(dur);
  if (record.branch) footParts.push(`${STR.archiveOverlayBranchPrefix}${record.branch}`);

  return (
    <button
      type="button"
      className="archive-row"
      onClick={() => onOpen(record)}
      title={STR.archiveOverlayOpenTip}
    >
      <span className={`archive-spine ${statusClass(record.status)}`} aria-hidden="true" />
      <span className="archive-row-body">
        <span className="archive-row-title">{record.prompt}</span>
        <span className="archive-row-foot">{footParts.join('  ·  ')}</span>
      </span>
    </button>
  );
}
