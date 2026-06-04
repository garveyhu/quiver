import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AgentEvent } from '@/types/agentEvent.types';
import type { InitialState, RecentProject } from '@/types/persistence.types';

// The "agent-event" channel name and the command names are the IPC contract
// with src-tauri/src/lib.rs — keep them in sync.
const AGENT_EVENT = 'agent-event';
const PICK_PROJECT = 'pick_project';
const SELECT_RECENT = 'select_recent_project';
const GET_INITIAL_STATE = 'get_initial_state';

export interface SupervisorState {
  events: AgentEvent[];
  error: string | null;
  projectPath: string | null;
  recentProjects: RecentProject[];
  pickProject: () => Promise<void>;
  selectRecentProject: (path: string) => Promise<void>;
}

/**
 * The single seam between the React UI and the Tauri core. All `invoke` / `listen`
 * IPC lives here so display components only ever touch plain props.
 *
 * On mount it (1) subscribes to the `agent-event` stream and (2) hydrates the
 * durable §11 state via `get_initial_state` — restoring the last picked project
 * and exposing the recent-projects list so it survives a restart. Runs are driven
 * through the bulletin-board queue (`useTaskBoard`); this hook owns only the live
 * event stream + project selection.
 */
export function useSupervisor(): SupervisorState {
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  // Keep the latest unlisten across re-renders without re-subscribing.
  const unlistenRef = useRef<(() => void) | null>(null);

  // Pull the durable state (last project + recents) from the store.
  const refreshState = useCallback(async () => {
    try {
      const state = await invoke<InitialState>(GET_INITIAL_STATE);
      setRecentProjects(state.recentProjects);
      return state;
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
      return null;
    }
  }, []);

  useEffect(() => {
    let active = true;
    listen<AgentEvent>(AGENT_EVENT, event => {
      if (active) setEvents(prev => [...prev, event.payload]);
    }).then(unlisten => {
      if (active) unlistenRef.current = unlisten;
      else unlisten();
    });

    // Hydrate durable state and restore the last picked project.
    refreshState().then(state => {
      if (active && state?.lastProject) setProjectPath(state.lastProject);
    });

    return () => {
      active = false;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, [refreshState]);

  const pickProject = useCallback(async () => {
    setError(null);
    try {
      const picked = await invoke<string | null>(PICK_PROJECT);
      // `null` = the user cancelled the dialog; keep the current selection.
      if (picked) {
        setProjectPath(picked);
        await refreshState();
      }
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
    }
  }, [refreshState]);

  const selectRecentProject = useCallback(
    async (path: string) => {
      setError(null);
      try {
        const selected = await invoke<string>(SELECT_RECENT, { path });
        setProjectPath(selected);
        await refreshState();
      } catch (e) {
        setError(typeof e === 'string' ? e : String(e));
      }
    },
    [refreshState],
  );

  return {
    events,
    error,
    projectPath,
    recentProjects,
    pickProject,
    selectRecentProject,
  };
}
