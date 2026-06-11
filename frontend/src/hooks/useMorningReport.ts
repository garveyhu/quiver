import { useEffect, useState } from 'react';

import { getDecisions, getMemoryFacts, getStats, listTasks } from '@/services/commands';
import type { FactRecord, ManagerDecision, Stats, TaskRecord } from '@/services/wire';

/** 一个协作目标(§5):经理拆解的具体父目标(显式),或 CEO 的自治目标(经理主动 plan 推进)+ 子任务统计。 */
export interface CollabGoal {
  id: string;
  goalText: string;
  total: number;
  done: number;
  /** planned(推进中)/ done(协作完成)/ needs_rebase(部分失败)。 */
  status: string;
  /** true=主动自治目标(CEO 设的方向、经理主动 plan);false=经理拆解的具体目标。 */
  isAuto: boolean;
}

export interface MorningReportData {
  stats: Stats | null;
  /** 协作目标(经理拆解 → 多员工分工):带 N/M 完成统计,单列突出"公司协作成果"。 */
  collabGoals: CollabGoal[];
  /** 合进了 main —— 自治公司真正推进了项目的部分(§5.7 经理交付合并成功)。 */
  mergedToMain: TaskRecord[];
  /** 卡住、等你处理:冲突待 rebase / 失败 / 打回(留分支不动 main)。 */
  needsYou: TaskRecord[];
  /** 经理升级给你拍板的(§12 叫醒)——超出自治能力,等你决定。最新在前。 */
  escalations: ManagerDecision[];
  /** 其余近期已结束委托(verified/done 但未走合并的,如 simulate)。 */
  recent: TaskRecord[];
  /** §6 自学习成果:近期沉淀的知识(教训避坑 / 约定规则 / 有效做法)—— 公司从经验学到的,越用越聪明。 */
  lessons: FactRecord[];
}

const DONE = new Set(['verified', 'done']);
const NEEDS_YOU = new Set(['needs_rebase', 'failed', 'escalated']);
/** 协作父目标的可见状态:拆解中 / 协作完成 / 部分失败。 */
const COLLAB = new Set(['planned', 'done', 'needs_rebase']);

/**
 * 晨报数据源(§12 验收台,"第二天早上的产品本身"):把过夜结果按**结局**分三类 ——
 * 合进 main 的(公司真推进了)、卡住等你的、升级等你拍板的 —— 而不是一锅"已结束"清单。
 * 面板打开时拉 stats + 全部任务 + 最近决策(挑 escalate)。
 */
export function useMorningReport(open: boolean): MorningReportData {
  const [data, setData] = useState<MorningReportData>({
    stats: null,
    collabGoals: [],
    mergedToMain: [],
    needsYou: [],
    escalations: [],
    recent: [],
    lessons: [],
  });

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void (async () => {
      try {
        const [stats, all, decisions, facts] = await Promise.all([
          getStats(),
          listTasks(),
          getDecisions(60),
          getMemoryFacts().catch(() => [] as FactRecord[]), // 记忆是加性:读不到不挡晨报
        ]);
        if (!alive) return;
        const byUpdated = (a: TaskRecord, b: TaskRecord) => b.updatedAt - a.updatedAt;
        const subsOf = (goal: string) => all.filter(x => x.parentGoal === goal);
        const tally = (subs: TaskRecord[]) => subs.filter(x => DONE.has(x.status) || x.status === 'merged').length;
        // 协作父目标 = 有子任务指向它(parentGoal==它的 prompt)。单列出来,不混进普通分类。
        const isParent = (t: TaskRecord) => all.some(x => x.parentGoal === t.prompt);
        const explicitGoals: CollabGoal[] = all
          .filter(g => isParent(g) && COLLAB.has(g.status))
          .sort(byUpdated)
          .map(g => {
            const subs = subsOf(g.prompt);
            return { id: g.id, goalText: g.prompt, total: subs.length, done: tally(subs), status: g.status, isAuto: false };
          });
        // 主动自治目标:子任务的 parentGoal 指向一个**没有对应 task 行**的文本(= CEO 的自治目标,
        // 经理主动 plan 时挂上去的)。它没有 planned 父任务行,所以上面那条认不出 → 这里补:把这些
        // 孤儿 parentGoal 聚成虚拟目标行,让「绝对自治」的目标进展也进晨报。
        const taskPrompts = new Set(all.map(t => t.prompt));
        const autoTexts = [...new Set(all.map(t => t.parentGoal).filter((g): g is string => !!g && !taskPrompts.has(g)))];
        const autoGoals: CollabGoal[] = autoTexts.map(goal => {
          const subs = subsOf(goal);
          const done = tally(subs);
          const allTerminal = subs.every(x => DONE.has(x.status) || x.status === 'merged' || NEEDS_YOU.has(x.status));
          const status = done === subs.length ? 'done' : allTerminal ? 'needs_rebase' : 'planned';
          return { id: `auto-${goal}`, goalText: goal, total: subs.length, done, status, isAuto: true };
        });
        // 自治目标在前(CEO 最关心「公司为我的目标推进了多少」),再拆活目标;共上限 8。
        const collabGoals: CollabGoal[] = [...autoGoals, ...explicitGoals].slice(0, 8);
        setData({
          stats,
          collabGoals,
          mergedToMain: all.filter(t => t.status === 'merged' && !isParent(t)).sort(byUpdated).slice(0, 8),
          needsYou: all.filter(t => NEEDS_YOU.has(t.status) && !isParent(t)).sort(byUpdated).slice(0, 8),
          // 升级决策:去重同一任务只留最新一条(经理可能多拍 escalate)。
          escalations: dedupeLatest(decisions.filter(d => d.action === 'escalate' && d.executed)),
          recent: all.filter(t => DONE.has(t.status) && !isParent(t)).sort(byUpdated).slice(0, 8),
          // §6 自学习成果:公司从经验沉淀的知识 —— 教训(避坑)+ 约定(规则)+ 有效做法(成功模式),
          // 机械教训 + 记忆官提炼都算,最新在前。给 CEO 看"昨晚公司学到了什么、越用越聪明"。
          lessons: facts
            .filter(f => f.kind === '教训' || f.kind === '约定' || f.kind === '有效做法')
            .sort((a, b) => b.recordedAt - a.recordedAt)
            .slice(0, 6),
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
