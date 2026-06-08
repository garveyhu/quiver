import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

const GET_STATS = 'get_stats';
const TASK_UPDATED = 'task-updated';

export interface Stats {
  total: number;
  verified: number;
  failed: number;
  costUsd: number;
  xp: number;
  level: number;
  /** 滚动窗口花费(全项目),与后端预算闸口径一致:近 24h / 近 30d。 */
  spentDay: number;
  spentMonth: number;
}

export interface StatsState {
  stats: Stats | null;
  refresh: () => Promise<void>;
}

/**
 * 工坊进度(XP / 等级)—— 读 `get_stats`(后端聚合 task 表),`task-updated` 时刷新。
 * 纯只读;XP = 验证·100 + 失败·20,等级在后端按 sqrt 曲线算。
 */
export function useStats(): StatsState {
  const [stats, setStats] = useState<Stats | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStats(await invoke<Stats>(GET_STATS));
    } catch {
      // 后端未就绪 / 命令缺失时静默,HUD 退化为不显示。
    }
  }, []);

  useEffect(() => {
    void refresh();
    let unlisten: (() => void) | null = null;
    void listen(TASK_UPDATED, () => void refresh()).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
  }, [refresh]);

  return { stats, refresh };
}
