import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { TaskRecord } from '@/types/persistence.types';
import type { RunMode } from '@/types/run.types';
import { play } from '@/utils/sound';

// IPC contract with src-tauri/src/lib.rs + scheduler.rs — keep names in sync.
const TASK_UPDATED = 'task-updated';
const LIST_TASKS = 'list_tasks';
const ENQUEUE_TASK = 'enqueue_task_cmd';
const REORDER_TASK = 'reorder_task';
const CANCEL_TASK = 'cancel_task_cmd';

export interface TaskBoardState {
  tasks: TaskRecord[];
  error: string | null;
  /** Ids that arrived since the last render — drives the "new commission" flash. */
  freshIds: Set<string>;
  enqueue: (prompt: string, mode: RunMode) => Promise<void>;
  reorder: (id: string, position: number) => Promise<void>;
  cancel: (id: string) => Promise<void>;
}

/**
 * The seam between the bulletin board UI and the durable §11 `task` queue.
 *
 * On mount it loads the board for `projectPath` and subscribes to the
 * `task-updated` channel — emitted by the scheduler on every lifecycle
 * transition (enqueue / claim / finish / cancel) so the board always reflects
 * live status, cost, and order without polling. `enqueue` pins a new commission
 * (the scheduler then runs it, up to maxWorkers concurrently); `reorder` and
 * `cancel` manage the board. Newly-seen ids are surfaced in `freshIds` for one
 * render so the card can flash.
 */
export function useTaskBoard(projectPath: string | null): TaskBoardState {
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [freshIds, setFreshIds] = useState<Set<string>>(new Set());

  // Ids we've already seen, to compute the fresh-arrival set on each refresh.
  const seenRef = useRef<Set<string>>(new Set());
  const projectRef = useRef<string | null>(projectPath);
  projectRef.current = projectPath;

  const refresh = useCallback(async () => {
    const project = projectRef.current;
    if (!project) {
      setTasks([]);
      seenRef.current = new Set();
      return;
    }
    try {
      const next = await invoke<TaskRecord[]>(LIST_TASKS, { project, status: null });
      // Newly-seen ids (not in the prior seen set) flash once.
      const fresh = new Set<string>();
      for (const t of next) if (!seenRef.current.has(t.id)) fresh.add(t.id);
      seenRef.current = new Set(next.map(t => t.id));
      setTasks(next);
      if (fresh.size > 0) {
        setFreshIds(fresh);
        // Clear the flash flag shortly after so it only plays once.
        setTimeout(() => setFreshIds(new Set()), 1200);
      }
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
    }
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    listen<unknown>(TASK_UPDATED, () => {
      if (active) void refresh();
    }).then(u => {
      if (active) unlisten = u;
      else u();
    });
    void refresh();
    return () => {
      active = false;
      unlisten?.();
    };
  }, [refresh, projectPath]);

  const enqueue = useCallback(
    async (prompt: string, mode: RunMode) => {
      setError(null);
      try {
        await invoke<TaskRecord>(ENQUEUE_TASK, { prompt, mode });
        play('pin'); // cozy "委托钉上板子" cue (no-op until audio ships)
        await refresh();
      } catch (e) {
        setError(typeof e === 'string' ? e : String(e));
      }
    },
    [refresh],
  );

  const reorder = useCallback(
    async (id: string, position: number) => {
      try {
        await invoke(REORDER_TASK, { id, position });
        await refresh();
      } catch (e) {
        setError(typeof e === 'string' ? e : String(e));
      }
    },
    [refresh],
  );

  const cancel = useCallback(
    async (id: string) => {
      setError(null);
      try {
        await invoke(CANCEL_TASK, { id });
        await refresh();
      } catch (e) {
        setError(typeof e === 'string' ? e : String(e));
      }
    },
    [refresh],
  );

  return { tasks, error, freshIds, enqueue, reorder, cancel };
}
