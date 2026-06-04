import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AgentEvent } from '@/types/agentEvent.types';

// The "agent-event" channel name and the "run_demo_task" command name are the
// IPC contract with src-tauri/src/lib.rs — keep them in sync.
const AGENT_EVENT = 'agent-event';
const RUN_DEMO_TASK = 'run_demo_task';

export interface SupervisorState {
  events: AgentEvent[];
  running: boolean;
  error: string | null;
  runTask: (prompt: string) => Promise<void>;
  clear: () => void;
}

/**
 * The single seam between the React UI and the Tauri core. All `invoke` / `listen`
 * IPC lives here so display components only ever touch plain props.
 *
 * Subscribes to the `agent-event` stream on mount and exposes `runTask`, which
 * invokes the `run_demo_task` command. Each emitted AgentEvent (and the final
 * synthesized `finished` event) is appended to `events` live.
 */
export function useSupervisor(): SupervisorState {
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Keep the latest unlisten across re-renders without re-subscribing.
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    let active = true;
    listen<AgentEvent>(AGENT_EVENT, event => {
      if (active) setEvents(prev => [...prev, event.payload]);
    }).then(unlisten => {
      if (active) unlistenRef.current = unlisten;
      else unlisten();
    });
    return () => {
      active = false;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

  const runTask = useCallback(async (prompt: string) => {
    setRunning(true);
    setError(null);
    setEvents([]);
    try {
      await invoke(RUN_DEMO_TASK, { prompt });
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
    } finally {
      setRunning(false);
    }
  }, []);

  const clear = useCallback(() => {
    setEvents([]);
    setError(null);
  }, []);

  return { events, running, error, runTask, clear };
}
