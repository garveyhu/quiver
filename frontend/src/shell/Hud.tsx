import { BUDGET_CAP_USD } from '@/theme/palette';

/** 顶部 HUD：品牌 + 实时统计 + 预算条。本轮为静态占位，后续接 get_stats/get_metrics。 */
export function Hud() {
  return (
    <div className="hud">
      <span className="brand">
        QUIVER <small>· 自治 AI 公司</small>
      </span>
      <span className="stat">
        经理 <b className="num">1</b>
      </span>
      <span className="stat">
        运行 <b className="num">0</b>
      </span>
      <span className="stat">
        通过 <b className="num">0</b>
      </span>
      <span className="stat">
        $<b className="num">0.00</b>/{BUDGET_CAP_USD}
      </span>
      <span className="bar">
        <i style={{ width: 0 }} />
      </span>
    </div>
  );
}
