import { useEffect, useState } from 'react';

import { getStats, listTasks } from '@/services/commands';
import { subscribe } from '@/services/ipc';
import type { Stats } from '@/services/wire';

/** HUD 要展示的实时口径(已从后端口径折算成展示口径)。 */
export interface HudData {
  /** 经理数(当前固定 1，AI 经理多实例化后再接) */
  managers: number;
  /** 在途运行中的任务数 */
  running: number;
  /** 累计验收通过数 */
  verified: number;
  /** 今夜(滚动 24h)花费,对齐预算闸 */
  spentUsd: number;
}

/**
 * HUD 数据源:挂载时拉 get_stats + 在途任务数，并订阅 `task-updated` 事件实时刷新。
 * 后端未就绪(无数据/dev mock)时静默退化为占位 0，不抛错破坏画面。
 */
export function useHud(): HudData {
  const [stats, setStats] = useState<Stats | null>(null);
  const [running, setRunning] = useState(0);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;

    const refresh = async () => {
      try {
        const [s, runningTasks] = await Promise.all([getStats(), listTasks(undefined, 'running')]);
        if (!alive) return;
        setStats(s);
        setRunning(runningTasks.length);
      } catch {
        // 后端不可用时保持占位，不打断渲染。
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

  return {
    managers: 1,
    running,
    verified: stats?.verified ?? 0,
    spentUsd: stats?.spentDay ?? 0,
  };
}
