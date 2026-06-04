import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AgentEvent } from '@/types/agentEvent.types';
import type { RunMode } from '@/types/run.types';

// The "agent-event" channel name and the command names are the IPC contract
// with src-tauri/src/lib.rs — keep them in sync.
const AGENT_EVENT = 'agent-event';
const RUN_TASK_CMD = 'run_task_cmd';
const PICK_PROJECT = 'pick_project';

export interface SupervisorState {
  events: AgentEvent[];
  running: boolean;
  error: string | null;
  projectPath: string | null;
  pickProject: () => Promise<void>;
  runTask: (prompt: string, mode: RunMode) => Promise<void>;
  clear: () => void;
}

/**
 * The single seam between the React UI and the Tauri core. All `invoke` / `listen`
 * IPC lives here so display components only ever touch plain props.
 *
 * Subscribes to the `agent-event` stream on mount and exposes `pickProject`
 * (folder dialog → validated git repo path) and `runTask` (invokes
 * `run_task_cmd` with the prompt + mode). Each emitted AgentEvent and the final
 * synthesized `finished` event is appended to `events` live.
 */
export function useSupervisor(): SupervisorState {
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [projectPath, setProjectPath] = useState<string | null>(null);
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

  const pickProject = useCallback(async () => {
    setError(null);
    try {
      const picked = await invoke<string | null>(PICK_PROJECT);
      // `null` = the user cancelled the dialog; keep the current selection.
      if (picked) setProjectPath(picked);
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
    }
  }, []);

  const runTask = useCallback(async (prompt: string, mode: RunMode) => {
    setRunning(true);
    setError(null);
    setEvents([]);
    try {
      await invoke(RUN_TASK_CMD, { prompt, mode });
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

  return { events, running, error, projectPath, pickProject, runTask, clear };
}
