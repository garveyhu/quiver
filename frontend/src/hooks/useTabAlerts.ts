import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { TaskRecord } from '@/types/persistence.types';
import { play } from '@/utils/sound';

// The diegetic surfaces a finished-run flash can target. Was imported from the
// (now removed) SceneTabs; inlined here so the hook is self-contained. The
// game-first redesign relocates this journal-flash into the UIScene's hotspot
// glint (DESIGN §4), but the IPC behaviour below is unchanged.
export type Scene = 'workshop' | 'board' | 'archive';

// IPC contract with src-tauri/src/lib.rs — keep names in sync.
const TASK_UPDATED = 'task-updated';
const LIST_TASKS = 'list_tasks';

// The terminal statuses that count as "a run finished" for the journal-flash.
const TERMINAL = new Set(['verified', 'done', 'failed', 'verify_failed', 'needs_rebase']);

// A finished run surfaces on both the board (its card flips to done) and the
// archive (it gains a new bound record), so both tabs deserve the flash.
const NOTIFY_TABS: Scene[] = ['board', 'archive'];

export interface TabAlertsState {
  /** Tabs with an unseen "a run finished" update. */
  alerts: Set<Scene>;
  /** Clear the alert for one tab — call when the user views it. */
  clear: (scene: Scene) => void;
}

/**
 * Drives the cross-tab "有更新" journal-flash (the spec's journal idea).
 *
 * Listens to the durable `task-updated` stream and remembers each task's last
 * status. When a task newly crosses into a terminal status, it flags the board
 * + archive tabs — unless the user is already looking at that tab, which is
 * cleared immediately. The active scene is tracked via a ref so a finish that
 * lands on the tab in view never raises a stale badge. Purely a notification
 * layer: it re-reads the same `list_tasks` the board/archive already use.
 */
export function useTabAlerts(activeScene: Scene): TabAlertsState {
  const [alerts, setAlerts] = useState<Set<Scene>>(new Set());

  // Last-seen status per task, to detect the transition INTO a terminal state.
  const statusRef = useRef<Map<string, string>>(new Map());
  // Skip the very first load so pre-existing finished runs don't all flash.
  const seededRef = useRef(false);
  const sceneRef = useRef<Scene>(activeScene);
  sceneRef.current = activeScene;

  const clear = useCallback((scene: Scene) => {
    setAlerts(prev => {
      if (!prev.has(scene)) return prev;
      const next = new Set(prev);
      next.delete(scene);
      return next;
    });
  }, []);

  const scan = useCallback(async () => {
    let all: TaskRecord[];
    try {
      all = await invoke<TaskRecord[]>(LIST_TASKS, { project: null, status: null });
    } catch {
      return;
    }

    let justFinished = false;
    for (const t of all) {
      const prev = statusRef.current.get(t.id);
      statusRef.current.set(t.id, t.status);
      if (seededRef.current && prev !== undefined && prev !== t.status && TERMINAL.has(t.status)) {
        justFinished = true;
      }
    }

    if (!seededRef.current) {
      seededRef.current = true;
      return; // first scan only seeds the baseline
    }

    if (justFinished) {
      play('notify');
      const active = sceneRef.current;
      setAlerts(prev => {
        const next = new Set(prev);
        for (const tab of NOTIFY_TABS) if (tab !== active) next.add(tab);
        return next;
      });
    }
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    listen<unknown>(TASK_UPDATED, () => {
      if (active) void scan();
    }).then(u => {
      if (active) unlisten = u;
      else u();
    });
    void scan();
    return () => {
      active = false;
      unlisten?.();
    };
  }, [scan]);

  // Looking at a tab clears its badge.
  useEffect(() => {
    clear(activeScene);
  }, [activeScene, clear]);

  return { alerts, clear };
}
