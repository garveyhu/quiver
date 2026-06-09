import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

const GET_BRIEF = 'get_brief';
const TASK_UPDATED = 'task-updated';

/** 当前真相事实(invalid_at 为空),后端 memory_fact 行的 camelCase wire 形状。 */
export interface FactRecord {
  id: number;
  project: string;
  scope: string;
  kind: string;
  text: string;
  entities: string | null;
  importance: number;
  validAt: number | null;
  invalidAt: number | null;
  recordedAt: number;
  trust: string;
}

/** 一条 episode(§6.2 机械记录)的 camelCase wire 形状。 */
export interface EpisodeRecord {
  id: number;
  project: string;
  nodeId: string | null;
  taskId: string | null;
  commitSha: string | null;
  mergeSeq: number | null;
  verifyResult: string | null;
  diffStat: string | null;
  summary: string | null;
  createdAt: number;
}

/** 经理简报(§6):当前事实 + 近期 episode。与后端 quiver-memory::Brief 对齐。 */
export interface Brief {
  project: string;
  facts: FactRecord[];
  recentEpisodes: EpisodeRecord[];
}

export interface BriefState {
  brief: Brief | null;
  refresh: () => Promise<void>;
}

/**
 * 当前项目的记忆简报 —— 读 `get_brief`(后端 quiver-memory::brief),`task-updated` 时
 * 刷新(任务完成会记一条 episode)。未选项目 / 记忆未就绪时静默退化为 null。
 */
export function useBrief(): BriefState {
  const [brief, setBrief] = useState<Brief | null>(null);

  const refresh = useCallback(async () => {
    try {
      setBrief(await invoke<Brief>(GET_BRIEF));
    } catch {
      // 未选项目 / 记忆未初始化时静默,简报面退化为不显示。
      setBrief(null);
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

  return { brief, refresh };
}
