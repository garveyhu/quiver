import { useCallback, useEffect, useRef, useState } from 'react';
import type { StoredEvent } from '@/types/persistence.types';
import type { AgentEvent } from '@/types/agentEvent.types';
import { parseStoredPayload } from '@/utils/eventFormat';

// Speed up the re-enactment vs. wall-clock: a run that took 30s replays in a few
// seconds. Gaps are also clamped so a long idle pause doesn't stall the replay.
const SPEED = 6;
const MAX_GAP_MS = 1600;
const MIN_GAP_MS = 250;

export interface ReplayState {
  /** The events emitted so far in the current replay (empty when idle). */
  events: AgentEvent[];
  /** True while a re-enactment is playing. */
  active: boolean;
  /** Start re-playing a run's stored event log with its original (scaled) timing. */
  start: (stored: StoredEvent[]) => void;
  /** Abort any in-flight replay and clear. */
  stop: () => void;
}

/**
 * Drives the optional "回放" juice: re-emits a finished run's §11 event log into
 * the workshop with the original (time-scaled) pacing, so the archer re-enacts
 * the run. Purely visual — it reuses the stored events and never invokes the
 * backend or any real claude. The growing `events` array is fed to PixelOffice
 * exactly like the live stream; clearing it (length drop to 0) makes the scene
 * reset, matching PixelOffice's existing shrink-to-reset contract.
 */
export function useReplay(): ReplayState {
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [active, setActive] = useState(false);
  const timers = useRef<number[]>([]);

  const clearTimers = useCallback(() => {
    for (const t of timers.current) window.clearTimeout(t);
    timers.current = [];
  }, []);

  const stop = useCallback(() => {
    clearTimers();
    setActive(false);
    setEvents([]);
  }, [clearTimers]);

  const start = useCallback(
    (stored: StoredEvent[]) => {
      clearTimers();
      const parsed = stored
        .map(s => ({ tsMs: s.tsMs, event: parseStoredPayload(s.payloadJson) }))
        .filter((p): p is { tsMs: number; event: AgentEvent } => p.event !== null);
      if (parsed.length === 0) return;

      setActive(true);
      setEvents([]);

      const base = parsed[0].tsMs;
      let elapsed = 0;
      let prevTs = base;
      parsed.forEach((p, i) => {
        const gap = i === 0 ? 0 : Math.min(MAX_GAP_MS, Math.max(MIN_GAP_MS, (p.tsMs - prevTs) / SPEED));
        prevTs = p.tsMs;
        elapsed += gap;
        const isLast = i === parsed.length - 1;
        const id = window.setTimeout(() => {
          setEvents(prev => [...prev, p.event]);
          if (isLast) setActive(false);
        }, elapsed);
        timers.current.push(id);
      });
    },
    [clearTimers],
  );

  useEffect(() => clearTimers, [clearTimers]);

  return { events, active, start, stop };
}
