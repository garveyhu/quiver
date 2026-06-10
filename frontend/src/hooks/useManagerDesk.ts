import { useCallback, useEffect, useState } from 'react';

import {
  addAuthoritativeFact,
  getBrief,
  getDecisions,
  getSettings,
  managerPreview,
  updateSettings,
} from '@/services/commands';
import { subscribe } from '@/services/ipc';
import type { Brief, ManagerDecision, ManagerPreview } from '@/services/wire';

/** 决策流最多留多少条(够看一段自治轨迹,不无限涨)。 */
const MAX_DECISIONS = 60;

export interface ManagerDeskState {
  /** 经理逐拍决策流(最新在前)。 */
  decisions: ManagerDecision[];
  /** 当前局面(在途/排队/预算),面板打开时拉。 */
  preview: ManagerPreview | null;
  /** 经理的记忆简报(注入决策上下文的内容,§6)——「经理在想什么」可见化。 */
  brief: Brief | null;
  /** 自治开关是否打开(经理控制循环驱动 vs 旧流水线)。 */
  autonomous: boolean;
  /** 翻自治开关(写回 settings;经理循环据此接管/退场)。 */
  toggleAutonomous: (on: boolean) => void;
  /** CEO 录入一条权威事实,成功后刷新简报让它立刻出现在「经理在想什么」。 */
  addFact: (text: string) => Promise<void>;
}

/**
 * 经理工作台数据源(DESIGN §5):
 * - **常驻订阅** `manager-decision` 事件,把经理每一拍的决策累积成决策流 —— 这样面板一打开
 *   就能看到经理刚才已经拍过的决策(不漏掉打开前发生的)。
 * - 面板打开时拉一次当前局面 + 自治开关状态。
 *
 * 决策流是"经理在自治、在基于决策调动员工"的可见证据,直击「看不到经理自治」的痛点。
 */
export function useManagerDesk(
  open: boolean,
  /** 经理升级给人(escalate)时回调 —— App 用状态条立刻喊人,「等你拍板」可感(§12)。 */
  onEscalate?: (reason: string) => void,
): ManagerDeskState {
  const [decisions, setDecisions] = useState<ManagerDecision[]>([]);
  const [preview, setPreview] = useState<ManagerPreview | null>(null);
  const [brief, setBrief] = useState<Brief | null>(null);
  const [autonomous, setAutonomous] = useState(false);

  // 常驻订阅决策流(挂载即订,不随面板开合 —— 关着时也累积,打开就有历史可看)。
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void subscribe<ManagerDecision>('manager-decision', d => {
      setDecisions(prev => [d, ...prev].slice(0, MAX_DECISIONS));
      if (d.action === 'escalate' && d.executed) onEscalate?.(d.reason ?? '(未给原因)');
    }).then(u => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
    // onEscalate 由 App 以 useCallback 稳定传入;不依赖它避免重订。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 打开面板时拉一次局面 + 自治开关状态。
  useEffect(() => {
    if (!open) return;
    let alive = true;
    void managerPreview()
      .then(p => alive && setPreview(p))
      .catch(() => {});
    void getSettings()
      .then(s => alive && setAutonomous(s.autonomous))
      .catch(() => {});
    void getBrief()
      .then(b => alive && setBrief(b))
      .catch(() => {}); // 记忆未初始化/未选项目 → 留 null,UI 显示空态
    // §10 决策流历史回填(decision_log):live 事件之前发生过什么,重启也不丢。
    // 与已累积的 live 决策按 (tsMs,seq) 去重合并,按时间降序,封顶 MAX_DECISIONS。
    void getDecisions(MAX_DECISIONS)
      .then(hist => {
        if (!alive) return;
        setDecisions(prev => {
          const seen = new Set(prev.map(d => `${d.tsMs}-${d.seq}`));
          const merged = [...prev, ...hist.filter(d => !seen.has(`${d.tsMs}-${d.seq}`))];
          merged.sort((a, b) => b.tsMs - a.tsMs || b.seq - a.seq);
          return merged.slice(0, MAX_DECISIONS);
        });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open]);

  const toggleAutonomous = useCallback((on: boolean) => {
    setAutonomous(on); // 乐观更新,失败再以后端为准
    void updateSettings({ autonomous: on })
      .then(s => setAutonomous(s.autonomous))
      .catch(() => setAutonomous(!on));
  }, []);

  const addFact = useCallback(async (text: string) => {
    await addAuthoritativeFact(text);
    const b = await getBrief(); // 录入后刷新,新事实立刻出现在简报里
    setBrief(b);
  }, []);

  return { decisions, preview, brief, autonomous, toggleAutonomous, addFact };
}
