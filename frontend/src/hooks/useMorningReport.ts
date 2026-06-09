import { useEffect, useState } from 'react';

import { getStats, listTasks } from '@/services/commands';
import type { Stats, TaskRecord } from '@/services/wire';

export interface MorningReportData {
  stats: Stats | null;
  /** 近期已结束的委托(done/verified/failed),按更新时间倒序 */
  recent: TaskRecord[];
}

const TERMINAL = new Set(['done', 'verified', 'failed']);

/** 晨报数据源:面板打开时拉 get_stats + 近期已结束任务。关闭时不拉。 */
export function useMorningReport(open: boolean): MorningReportData {
  const [stats, setStats] = useState<Stats | null>(null);
  const [recent, setRecent] = useState<TaskRecord[]>([]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void (async () => {
      try {
        const [s, all] = await Promise.all([getStats(), listTasks()]);
        if (!alive) return;
        setStats(s);
        setRecent(
          all
            .filter(t => TERMINAL.has(t.status))
            .sort((a, b) => b.updatedAt - a.updatedAt)
            .slice(0, 8),
        );
      } catch {
        // 后端不可用时保持上次/空,不打断画面。
      }
    })();
    return () => {
      alive = false;
    };
  }, [open]);

  return { stats, recent };
}
