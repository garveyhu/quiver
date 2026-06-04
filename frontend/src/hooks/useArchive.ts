import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { StoredEvent, TaskRecord } from '@/types/persistence.types';

// IPC contract with src-tauri/src/lib.rs — keep names in sync.
const TASK_UPDATED = 'task-updated';
const LIST_TASKS = 'list_tasks';
const GET_TASK_EVENTS = 'get_task_events';

export interface ArchiveState {
  /** Every persisted run across all projects, newest first. */
  records: TaskRecord[];
  error: string | null;
  refresh: () => Promise<void>;
  /** Pull the full §11 event log for one run (the Logbook source). */
  loadEvents: (taskId: string) => Promise<StoredEvent[]>;
}

/**
 * The seam between the 档案库 (run archive) + 卷轴 (Logbook) UI and the durable
 * §11 store. On mount it loads every persisted task (no project/status filter)
 * and subscribes to `task-updated` so the shelf reflects finished runs without
 * polling. `loadEvents` lazily fetches one run's ordered event log on demand —
 * the archive list stays cheap, the heavy I/O log loads only when a record is
 * opened. Records are returned newest-first (the archive sorts by recency,
 * unlike the board which sorts by queue position).
 */
export function useArchive(): ArchiveState {
  const [records, setRecords] = useState<TaskRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const all = await invoke<TaskRecord[]>(LIST_TASKS, { project: null, status: null });
      // The archive is a history surface: newest first by creation time.
      const sorted = [...all].sort((a, b) => b.createdAt - a.createdAt);
      setRecords(sorted);
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
  }, [refresh]);

  const loadEvents = useCallback(async (taskId: string): Promise<StoredEvent[]> => {
    return invoke<StoredEvent[]>(GET_TASK_EVENTS, { taskId });
  }, []);

  return { records, error, refresh, loadEvents };
}
