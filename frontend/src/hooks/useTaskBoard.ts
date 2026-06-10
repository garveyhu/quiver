import { useCallback, useEffect, useState } from 'react';

import { cancelTask, listTasks, reorderTask } from '@/services/commands';
import { subscribe } from '@/services/ipc';
import type { TaskRecord } from '@/services/wire';

/** 看板上"活跃"的任务状态(排队/在跑/验收中) —— CEO 要掌控的就是这些。 */
const ACTIVE = new Set(['queued', 'running', 'verifying']);

export interface TaskBoardState {
  tasks: TaskRecord[];
  /** 上移一个排队任务(排到前一个之前)。 */
  moveUp: (index: number) => Promise<void>;
  /** 下移一个排队任务(排到后一个之后)。 */
  moveDown: (index: number) => Promise<void>;
  /** 取消一个任务(排队删除 / 在跑杀进程)。 */
  cancel: (id: string) => Promise<void>;
}

/**
 * 任务看板数据源:活跃任务按 position 排,实时随 task-updated 刷新。提供上移/下移(调
 * 优先级,经理/调度器按 position 认领)、取消。让 CEO 掌控自治公司的工作队列。
 */
export function useTaskBoard(open: boolean): TaskBoardState {
  const [tasks, setTasks] = useState<TaskRecord[]>([]);

  const refresh = useCallback(() => {
    void listTasks()
      .then(ts =>
        setTasks(ts.filter(t => ACTIVE.has(t.status)).sort((a, b) => a.position - b.position)),
      )
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!open) return;
    refresh();
    let alive = true;
    let unlisten: (() => void) | undefined;
    subscribe('task-updated', () => alive && refresh()).then(u => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [open, refresh]);

  const moveUp = useCallback(
    async (index: number) => {
      if (index <= 0) return;
      // 排到前一个之前:position 设为前一个 - 1(列表按 position 升序)。
      await reorderTask(tasks[index].id, tasks[index - 1].position - 1);
      refresh();
    },
    [tasks, refresh],
  );

  const moveDown = useCallback(
    async (index: number) => {
      if (index >= tasks.length - 1) return;
      await reorderTask(tasks[index].id, tasks[index + 1].position + 1);
      refresh();
    },
    [tasks, refresh],
  );

  const cancel = useCallback(
    async (id: string) => {
      await cancelTask(id);
      refresh();
    },
    [refresh],
  );

  return { tasks, moveUp, moveDown, cancel };
}
