import { useMemo } from 'react';
import type { AgentEvent } from '@/types/agentEvent.types';
import type { WorkerView } from '@/office/types';
import { createWorker, reduceWorker } from '@/office/poseMachine';

/**
 * 把实时事件流折叠成每个任务的 WorkerView(姿势状态机)。
 * slot = 任务首次出现的顺序 % 4(对应 4 种皮肤)。
 */
export function useWorkers(events: AgentEvent[]): WorkerView[] {
  return useMemo(() => {
    const map = new Map<string, WorkerView>();
    let next = 0;
    for (const ev of events) {
      let w = map.get(ev.taskId);
      if (!w) {
        w = createWorker(ev.taskId, next % 4);
        next += 1;
      }
      map.set(ev.taskId, reduceWorker(w, ev));
    }
    return [...map.values()];
  }, [events]);
}
