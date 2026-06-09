import type { MetricsDto } from '@/services/wire';

interface TrustCardProps {
  open: boolean;
  metrics: MetricsDto | null;
  onClose: () => void;
}

const BARS = 20;

/** 按验收率把 20 格染成绿(验收)/红(失败);有失败至少留 1 格红、非全败至少留 1 格绿。 */
function bars(verifyRate: number) {
  let green = Math.round(verifyRate * BARS);
  if (verifyRate < 1 && green >= BARS) green = BARS - 1;
  if (verifyRate > 0 && green <= 0) green = 1;
  return Array.from({ length: BARS }, (_, i) => (i < green ? 'g' : 'r'));
}

/**
 * 信任卡(移植原型 openTrust):看公司自治战绩 + 锁死的破坏半径。
 * 战绩接 get_metrics 真值(验收率/时延/花费);护栏是产品硬保证(静态)。
 * 原型的"规划自由度"旋钮暂不接(无对应后端信任设置,不放死控件)。
 */
export function TrustCard({ open, metrics, onClose }: TrustCardProps) {
  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>
        经理 · 自治表现 <span className="h2-dim">· 破坏半径锁死</span>
      </h2>
      <div className="sub">信任 = 规划自由度,不是危险。下面的破坏半径任何时候都动不了。</div>
      <div className="body">
        {metrics ? (
          <>
            <div className="trust-head">战绩 · 近 {metrics.runs} 次运行</div>
            <div className="kv">
              <b className="kv-tight">干净验收 {metrics.verified} · 失败 {metrics.failed}</b>
              <div className="bars">
                {bars(metrics.verifyRate).map((c, i) => (
                  <i key={i} className={c} />
                ))}
              </div>
            </div>
            <div className="kv">
              <b className="kv-tight">验收率 {(metrics.verifyRate * 100).toFixed(1)}% · 时延 p50 {metrics.p50DurationMs}ms · p95 {metrics.p95DurationMs}ms</b>
            </div>
            <div className="kv">
              <b className="kv-tight">总花费 ${metrics.totalCostUsd.toFixed(2)} · 均 ${metrics.avgCostUsd.toFixed(2)}/次</b>
            </div>
          </>
        ) : (
          <div className="kv">
            <b className="kv-tight">读取中…</b>
          </div>
        )}
        <div className="lockline">破坏半径 · 锁死</div>
        <div className="lockrow">合并必须过独立审计</div>
        <div className="lockrow">不能碰 main</div>
        <div className="lockrow">不能写权威记忆</div>
        <div className="lockrow">不能直接联网</div>
      </div>
      <div className="foot">
        <button className="pbtn go" type="button" onClick={onClose}>
          好
        </button>
      </div>
    </div>
  );
}
