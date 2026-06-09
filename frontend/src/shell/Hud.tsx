import type { HudData } from '@/hooks/useHud';

interface HudProps {
  data: HudData;
  /** 今夜预算上限;null = 未设(只显花费、不显进度条) */
  budgetCap: number | null;
}

/** 顶部 HUD：品牌 + 实时统计 + 预算条。纯展示，数据由 App 经 useHud / useSettings 注入。 */
export function Hud({ data, budgetCap }: HudProps) {
  const { managers, running, verified, spentUsd } = data;
  const barPct = budgetCap ? Math.min(100, (spentUsd / budgetCap) * 100) : 0;

  return (
    <div className="hud">
      <span className="brand">
        QUIVER <small>· 自治 AI 公司</small>
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
