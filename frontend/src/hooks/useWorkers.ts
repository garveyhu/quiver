import { useEffect, useMemo, useState } from 'react';

import type { Layout } from '@/office/iso';
import { placeWorkers, type PlacedWorker } from '@/office/workers';
import { listTasks } from '@/services/commands';
import { subscribe } from '@/services/ipc';
import type { TaskRecord } from '@/services/wire';

const ACTIVE = new Set(['running', 'verifying']);

/**
 * 工人布局数据源:订阅 `task-updated`,按真实在途(running/verifying)任务把员工落到工位。
 * 任务进出 → 重算布局,员工 id 稳定故由 CSS 过渡平滑滑行(休息室 ⇄ 工位)。
 */
export function useWorkers(layout: Layout): PlacedWorker[] {
  const [active, setActive] = useState<TaskRecord[]>([]);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;

    const refresh = async () => {
      try {
        const tasks = await listTasks();
        if (!alive) return;
        setActive(tasks.filter(t => ACTIVE.has(t.status)).sort((a, b) => a.createdAt - b.createdAt));
      } catch {
        // 后端不可用时保持空(全员待命)。
      }
    };

    void refresh();
    subscribe('task-updated', refresh).then(u => {
      if (alive) unlisten = u;
      else u();
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  return useMemo(() => placeWorkers(layout, active), [layout, active]);
}
