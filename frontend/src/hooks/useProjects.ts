import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { InitialState, RecentProject } from '@/types/persistence.types';

// IPC contract with src-tauri/src/lib.rs — keep names in sync. The CRUD command
// vocabulary the door's 项目管理 overlay drives:
//   pick_project()          → add (native folder dialog, owned by useSupervisor)
//   select_recent_project() → set as current (owned by useSupervisor)
//   remove_recent_project() → drop from the recent list
//   set_project_alias()     → give/rename a display alias
//   get_initial_state()     → re-read the recent list + current project
const GET_INITIAL_STATE = 'get_initial_state';
const REMOVE_RECENT = 'remove_recent_project';
const SET_ALIAS = 'set_project_alias';

export interface ProjectsState {
  /** The recent projects, newest first (mirrors the durable §11 MRU list). */
  recent: RecentProject[];
  error: string | null;
  /** Re-read the recent list + current project from the store. */
  refresh: () => Promise<void>;
  /** Forget a project from the recent list (does not touch the current pick). */
  remove: (path: string) => Promise<void>;
  /** Set (or clear, with `null`) a project's display alias. */
  rename: (path: string, alias: string | null) => Promise<void>;
}

/**
 * The seam between the 项目管理 overlay and the durable §11 project list. Owns the
 * two CRUD commands that aren't already in {@link useSupervisor} — `remove` and
 * `rename` — plus a `refresh` that re-reads the recent list. Add (folder dialog)
 * and select-as-current stay in `useSupervisor`, which owns `projectPath` (the
 * world's project gate), so there is one source of truth for the current pick;
 * after a mutation the bridge calls both hooks' `refresh` to keep the list live.
 *
 * Every `invoke` lives here (the §11 rule): the overlay only renders the pushed
 * {@link ProjectsState} and emits command intents on the bus.
 */
export function useProjects(): ProjectsState {
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const state = await invoke<InitialState>(GET_INITIAL_STATE);
      setRecent(state.recentProjects);
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const remove = useCallback(
    async (path: string) => {
      setError(null);
      try {
        await invoke<void>(REMOVE_RECENT, { path });
        await refresh();
      } catch (e) {
        setError(typeof e === 'string' ? e : String(e));
      }
    },
    [refresh],
  );

  const rename = useCallback(
    async (path: string, alias: string | null) => {
      setError(null);
      try {
        await invoke<void>(SET_ALIAS, { path, alias });
        await refresh();
      } catch (e) {
        setError(typeof e === 'string' ? e : String(e));
      }
    },
    [refresh],
  );

  return { recent, error, refresh, remove, rename };
}
