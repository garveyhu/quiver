import type { HudData } from '@/hooks/useHud';

interface HudProps {
  data: HudData;
  /** 今夜预算上限;null = 未设(只显花费、不显进度条) */
  budgetCap: number | null;
  /** 自治是否开启(经理控制循环接管调度)。 */
  autonomous: boolean;
  /** CEO 给的自治目标(经理派完手头活就主动推进它);空=被动等派活。 */
  goal: string;
}

/** 顶部 HUD：品牌 + 自治状态 + 实时统计 + 预算条。纯展示,数据由 App 经 useHud / useSettings 注入。 */
export function Hud({ data, budgetCap, autonomous, goal }: HudProps) {
  const { managers, running, verified, spentUsd } = data;
  const barPct = budgetCap ? Math.min(100, (spentUsd / budgetCap) * 100) : 0;
  const g = goal.trim();

  return (
    <div className="hud">
      {/* 自治状态:打开 app 一眼看到公司在不在自治、为哪个目标 —— 「绝对自治」的顶层可见。 */}
      <span
        className={`hud-auto${autonomous ? ' on' : ''}`}
        title={autonomous ? (g ? `自治推进目标:${g}` : '自治中,等你派活') : '手动:有排队就跑,经理不主动决策'}
      >
        {autonomous ? (g ? `自治推进:${g.length > 14 ? `${g.slice(0, 14)}…` : g}` : '自治中') : '手动调度'}
      </span>
      <span className="stat">
        经理 <b className="num">{managers}</b>
      </span>
      <span className="stat">
        运行 <b className="num">{running}</b>
      </span>
      <span className="stat">
        通过 <b className="num">{verified}</b>
      </span>
      <span className="stat">
        $<b className="num">{spentUsd.toFixed(2)}</b>
        {budgetCap ? `/${budgetCap}` : ' 今夜'}
      </span>
      {budgetCap != null && (
        <span className="bar">
          <i style={{ width: `${barPct}%` }} />
        </span>
      )}
    </div>
  );
}
