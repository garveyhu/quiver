import { useEffect, useMemo, useState } from 'react';

import type { Layout } from '@/office/iso';
import { placeWorkers, type PlacedWorker } from '@/office/workers';
import { listTasks } from '@/services/commands';
import { subscribe } from '@/services/ipc';
import type { AgentEvent, TaskRecord } from '@/services/wire';

const ACTIVE = new Set(['running', 'verifying']);

/** 短气泡:截断工具摘要。 */
function short(text: string): string {
  const t = text.trim();
  return t.length > 14 ? t.slice(0, 14) + '…' : t;
}

/**
 * 工人布局数据源:
 * - 订阅 `task-updated`,按真实在途(running/verifying)任务把员工落到工位。
 * - 订阅 `agent-event`,把每个任务最新的工具活动喂进对应工位工人的气泡(实时"在敲啥")。
 * 任务进出 → 重算布局,员工 id 稳定故由 CSS 过渡平滑滑行。
 */
export function useWorkers(layout: Layout): PlacedWorker[] {
  const [active, setActive] = useState<TaskRecord[]>([]);
  const [bubbles, setBubbles] = useState<Record<string, string>>({});

  useEffect(() => {
    let alive = true;
    const unlisteners: Array<() => void> = [];
    const track = (p: Promise<() => void>) => {
      p.then(u => (alive ? unlisteners.push(u) : u()));
    };

    const refresh = async () => {
      try {
        const tasks = await listTasks();
        if (!alive) return;
        const act = tasks.filter(t => ACTIVE.has(t.status)).sort((a, b) => a.createdAt - b.createdAt);
        setActive(act);
        // 丢弃已不在途任务的气泡,避免无限增长。
        const liveIds = new Set(act.map(t => t.id));
        setBubbles(prev => {
          const next: Record<string, string> = {};
          for (const id of Object.keys(prev)) if (liveIds.has(id)) next[id] = prev[id];
          return next;
        });
      } catch {
        // 后端不可用时保持空(全员待命)。
      }
    };

    void refresh();
    track(subscribe('task-updated', refresh));
    track(
      subscribe<AgentEvent>('agent-event', ev => {
        if (!alive) return;
        if (ev.kind === 'tool_use') setBubbles(prev => ({ ...prev, [ev.taskId]: short(ev.summary || ev.tool) }));
        else if (ev.kind === 'worker_started') setBubbles(prev => ({ ...prev, [ev.taskId]: '开工…' }));
      }),
    );

    return () => {
      alive = false;
      unlisteners.forEach(u => u());
    };
  }, []);

  return useMemo(() => placeWorkers(layout, active, bubbles), [layout, active, bubbles]);
}
