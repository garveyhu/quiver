import { useEffect, useState } from 'react';

import { getDecisions, getStats, listTasks } from '@/services/commands';
import type { ManagerDecision, Stats, TaskRecord } from '@/services/wire';

export interface MorningReportData {
  stats: Stats | null;
  /** 合进了 main —— 自治公司真正推进了项目的部分(§5.7 经理交付合并成功)。 */
  mergedToMain: TaskRecord[];
  /** 卡住、等你处理:冲突待 rebase / 失败 / 打回(留分支不动 main)。 */
  needsYou: TaskRecord[];
  /** 经理升级给你拍板的(§12 叫醒)——超出自治能力,等你决定。最新在前。 */
  escalations: ManagerDecision[];
  /** 其余近期已结束委托(verified/done 但未走合并的,如 simulate)。 */
  recent: TaskRecord[];
}

const DONE = new Set(['verified', 'done']);
const NEEDS_YOU = new Set(['needs_rebase', 'failed']);

/**
 * 晨报数据源(§12 验收台,"第二天早上的产品本身"):把过夜结果按**结局**分三类 ——
 * 合进 main 的(公司真推进了)、卡住等你的、升级等你拍板的 —— 而不是一锅"已结束"清单。
 * 面板打开时拉 stats + 全部任务 + 最近决策(挑 escalate)。
 */
export function useMorningReport(open: boolean): MorningReportData {
  const [data, setData] = useState<MorningReportData>({
    stats: null,
    mergedToMain: [],
    needsYou: [],
    escalations: [],
    recent: [],
  });

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void (async () => {
      try {
        const [stats, all, decisions] = await Promise.all([getStats(), listTasks(), getDecisions(60)]);
        if (!alive) return;
        const byUpdated = (a: TaskRecord, b: TaskRecord) => b.updatedAt - a.updatedAt;
        setData({
          stats,
          mergedToMain: all.filter(t => t.status === 'merged').sort(byUpdated).slice(0, 8),
          needsYou: all.filter(t => NEEDS_YOU.has(t.status)).sort(byUpdated).slice(0, 8),
          // 升级决策:去重同一任务只留最新一条(经理可能多拍 escalate)。
          escalations: dedupeLatest(decisions.filter(d => d.action === 'escalate' && d.executed)),
          recent: all.filter(t => DONE.has(t.status)).sort(byUpdated).slice(0, 8),
        });
      } catch {
        // 后端不可用 → 保持上次/空,不打断画面。
      }
    })();
    return () => {
      alive = false;
    };
  }, [open]);

  return data;
}

/** 同一 (taskId|reason) 只留最新一条升级,避免重复刷屏;无 taskId 的按 reason 去重。 */
function dedupeLatest(decisions: ManagerDecision[]): ManagerDecision[] {
  const seen = new Set<string>();
  const out: ManagerDecision[] = [];
  for (const d of [...decisions].sort((a, b) => b.tsMs - a.tsMs)) {
    const key = d.taskId ?? d.reason ?? String(d.tsMs);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(d);
  }
  return out.slice(0, 8);
}
