import type { HudData } from '@/hooks/useHud';

interface HudProps {
  data: HudData;
}

/** 顶部 HUD：品牌 + 实时统计 + 预算条。纯展示，数据由 App 经 useHud 注入。 */
export function Hud({ data }: HudProps) {
  const { managers, running, verified, spentUsd, budgetCapUsd } = data;
  const barPct = Math.min(100, (spentUsd / budgetCapUsd) * 100);

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
        $<b className="num">{spentUsd.toFixed(2)}</b>/{budgetCapUsd}
      </span>
      <span className="bar">
        <i style={{ width: `${barPct}%` }} />
      </span>
    </div>
  );
}
