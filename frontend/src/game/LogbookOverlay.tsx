import { useCallback, useEffect, useMemo, useState } from 'react';
import { EventBus, BUS, type LogbookState } from '@/game/EventBus';
import { parseStoredEvents, durationLabel, projectLeaf } from '@/game/logbookState';
import { transcriptLine, clockStamp, formatCost } from '@/utils/eventFormat';
import type { AgentEvent } from '@/types/agentEvent.types';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import { play } from '@/utils/sound';

/**
 * The 委托卷轴 / Logbook as a real React DOM overlay (replaces the old Phaser
 * LogbookScene). The §6 two-layer rule is the whole point: a cozy parchment
 * SCROLL skin (CSS — wooden rollers,羊皮纸底, warm shadow, theme-aware) wraps a
 * HIGH-DENSITY, scan-readable transcript rendered in a native `<div overflow:auto>`.
 *
 * Why DOM, not hand-painted canvas + a GeometryMask: the browser's own scrolling /
 * clipping / wrapping NEVER overflows its container at any window size — the bug
 * the Phaser scroll hit on large windows (rows pressing onto the header, output
 * leaking under the bottom roller) is structurally impossible here.
 *
 * Data flow is unchanged: the GameBridge owns the IPC (useArchive.loadEvents, the
 * sole §11 caller), pushes the resolved {@link LogbookState} on `BUS.logbookState`,
 * and this component only renders it. "回放" hands the run's raw stored log back to
 * the workshop over the existing `BUS.replayStart` (→ useReplay.start) and closes.
 */
export function LogbookOverlay() {
  const [state, setState] = useState<LogbookState | null>(null);

  useEffect(() => {
    const onState = (next: LogbookState | null) => setState(next);
    EventBus.on(BUS.logbookState, onState);
    return () => {
      EventBus.off(BUS.logbookState, onState);
    };
  }, []);

  const close = useCallback(() => {
    play('close');
    EventBus.emit(BUS.closeLogbook);
  }, []);

  const replay = useCallback(() => {
    if (!state || state.raw.length === 0) return;
    play('open');
    // hand the stored log to useReplay (via the bridge); the workshop re-enacts it.
    EventBus.emit(BUS.replayStart, state.raw);
    EventBus.emit(BUS.closeLogbook);
  }, [state]);

  // Escape rolls the scroll up (parity with the rest of the world's overlays).
  useEffect(() => {
    if (!state) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [state, close]);

  // Parse once per loaded state (not per render) — the transcript can be long.
  const rows = useMemo(() => (state ? parseStoredEvents(state.raw) : []), [state]);

  if (!state) return null;

  const { meta, loading, error, raw } = state;
  const canReplay = !loading && !error && raw.length > 0;

  return (
    <div className="logbook-overlay" role="dialog" aria-modal="true" aria-label={STR.logbookTitle}>
      <div className="logbook-scrim" onClick={close} role="presentation" />
      <div className="logbook-scroll" role="document">
        <div className="logbook-roller logbook-roller-top" aria-hidden="true" />

        <LogbookHeader meta={meta} />

        <div className="logbook-transcript" tabIndex={0} aria-label={STR.logbookTimelineTitle}>
          {loading ? (
            <p className="logbook-status">{STR.logbookSceneLoading}</p>
          ) : error ? (
            <p className="logbook-status logbook-status-error">{STR.logbookSceneError}</p>
          ) : rows.length === 0 ? (
            <p className="logbook-status">{STR.logbookSceneEmpty}</p>
          ) : (
            rows.map(row => <TranscriptRow key={row.seq} tsMs={row.tsMs} event={row.event} />)
          )}
        </div>

        <div className="logbook-actions">
          <button type="button" className="logbook-btn logbook-btn-back" onClick={close}>
            {STR.logbookSceneBack}
          </button>
          <button
            type="button"
            className="logbook-btn logbook-btn-replay"
            onClick={replay}
            disabled={!canReplay}
            title={STR.logbookSceneReplayTip}
          >
            {STR.logbookSceneReplay}
          </button>
        </div>

        <div className="logbook-roller logbook-roller-bottom" aria-hidden="true" />
      </div>
    </div>
  );
}

interface HeaderProps {
  meta: LogbookState['meta'];
}

// The sticky commission head: 委托 #id · 标题, then a meta strip of chips
// (项目 / 模式 / 状态 / 分支 / 事件数 / 花费 / 历时 / 轮次).
function LogbookHeader({ meta }: HeaderProps) {
  const idShort = meta.taskId.length > 10 ? `${meta.taskId.slice(0, 10)}…` : meta.taskId;
  const prompt = meta.prompt ?? meta.taskId;

  const chips: Array<{ label: string; value: string }> = [];
  if (meta.project) chips.push({ label: STR.logbookProjectLabel, value: projectLeaf(meta.project) });
  if (meta.mode) {
    chips.push({
      label: STR.logbookModeLabel,
      value: meta.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate,
    });
  }
  if (meta.status) {
    chips.push({
      label: STR.logbookStatusLabel,
      value: TASK_STATUS_LABEL[meta.status] ?? meta.status,
    });
  }
  if (meta.branch) chips.push({ label: STR.logbookBranchLabel, value: meta.branch });
  chips.push({ label: STR.logbookTotalEvents, value: String(meta.count) });
  chips.push({ label: STR.logbookTotalCost, value: formatCost(meta.cost) });
  chips.push({ label: STR.logbookTotalDuration, value: durationLabel(meta.durationMs) });
  if (meta.turns != null) {
    chips.push({ label: STR.logbookSceneTurnsPrefix.trim(), value: String(meta.turns) });
  }

  return (
    <header className="logbook-header">
      <h2 className="logbook-title">
        <span className="logbook-id">委托 #{idShort}</span>
        <span className="logbook-prompt">{prompt}</span>
      </h2>
      <dl className="logbook-meta">
        {chips.map(chip => (
          <div className="logbook-chip" key={chip.label}>
            <dt className="logbook-chip-label">{chip.label}</dt>
            <dd className="logbook-chip-value">{chip.value}</dd>
          </div>
        ))}
      </dl>
    </header>
  );
}

interface RowProps {
  tsMs: number;
  event: AgentEvent;
}

// One transcript line: a clock stamp + per-kind color rail + glyph + head, with an
// optional verbatim output / error block that wraps gracefully (word-break) so a
// long line never widens or overflows the panel.
function TranscriptRow({ tsMs, event }: RowProps) {
  const line = transcriptLine(event);
  return (
    <div className={`logbook-row logbook-kind-${event.kind}`}>
      <time className="logbook-stamp">{clockStamp(tsMs)}</time>
      <div className="logbook-body">
        <div className="logbook-head">
          <span className="logbook-glyph" aria-hidden="true">
            {line.glyph}
          </span>
          <span className="logbook-head-text">{line.head}</span>
        </div>
        {line.block ? (
          <pre className={`logbook-block logbook-block-${line.kind}`}>{line.block}</pre>
        ) : null}
      </div>
    </div>
  );
}
