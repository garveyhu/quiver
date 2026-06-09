import { useEffect, useState } from 'react';

import { getTaskEvents } from '@/services/commands';
import { subscribe } from '@/services/ipc';
import type { AgentEvent, StoredEvent } from '@/services/wire';

/**
 * 某任务的事件流:taskId 变化时拉一次 get_task_events,并订阅 agent-event ——
 * 该任务每来一条新事件就重拉,worksurf 随运行实时填充(开工→工具→结果→终态)。
 */
export function useTaskEvents(taskId: string | null): StoredEvent[] {
  const [events, setEvents] = useState<StoredEvent[]>([]);

  useEffect(() => {
    if (!taskId) {
      setEvents([]);
      return;
    }
    let alive = true;
    let unlisten: (() => void) | undefined;

    const refresh = () => {
      void getTaskEvents(taskId)
        .then(e => {
          if (alive) setEvents(e);
        })
        .catch(() => {});
    };

    refresh();
    subscribe<AgentEvent>('agent-event', ev => {
      if (alive && ev.taskId === taskId) refresh();
    }).then(u => {
      if (alive) unlisten = u;
      else u();
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, [taskId]);

  return events;
}
