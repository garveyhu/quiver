import type { CSSProperties } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

const MONO11: CSSProperties = { font: '10px ui-monospace,monospace' };

export interface BudgetBarProps {
  spent: number;
  total: number;
  width?: number;
  className?: string;
  style?: CSSProperties;
}

/** 美元预算条:绿→黄→红。Quiver 的预算治理是核心,这是它的可视面。 */
export function BudgetBar({ spent, total, width = 160, className, style }: BudgetBarProps) {
  const pct = total > 0 ? Math.max(0, Math.min(1, spent / total)) : 0;
  const color = pct < 0.6 ? cssVar('green') : pct < 0.85 ? cssVar('amber') : cssVar('red');
  return (
    <div className={className} style={style}>
      <div className="qv-gauge" style={{ width, height: 14 }}>
        <div className="qv-gauge-fill" style={{ width: `${pct * 100}%`, background: color }} />
      </div>
      <div style={{ ...MONO11, color: cssVar('cream'), marginTop: 3 }}>
        ${spent.toFixed(2)} / ${total.toFixed(2)}
      </div>
    </div>
  );
}

export interface XpBarProps {
  xp: number;
  max: number;
  level: number;
  width?: number;
  className?: string;
  style?: CSSProperties;
}

/** XP 进度条 + 等级牌(游戏化)。 */
export function XpBar({ xp, max, level, width = 150, className, style }: XpBarProps) {
  const pct = max > 0 ? Math.max(0, Math.min(1, xp / max)) : 0;
  return (
    <div className={className} style={{ display: 'flex', alignItems: 'center', gap: 6, ...style }}>
      <span
        style={{
          font: 'bold 10px ui-monospace,monospace',
          color: cssVar('ink'),
          background: cssVar('amber'),
          padding: '2px 6px',
          boxShadow: '1px 1px 0 #00000055',
        }}
      >
        Lv{level}
      </span>
      <div className="qv-gauge" style={{ width, height: 12 }}>
        <div className="qv-gauge-fill" style={{ width: `${pct * 100}%`, background: cssVar('amber') }} />
      </div>
    </div>
  );
}

export type TagKind = 'verified' | 'failed' | 'running' | 'paused';

const TAG: Record<TagKind, { color: string; text: string }> = {
  verified: { color: cssVar('green'), text: '已合并' },
  failed: { color: cssVar('red'), text: '失败' },
  running: { color: cssVar('blue'), text: '运行中' },
  paused: { color: cssVar('amber2'), text: '暂停' },
};

/** 状态标签 chip:任务列表/详情里的小色标。 */
export function StatusTag({ kind }: { kind: TagKind }) {
  const t = TAG[kind];
  return (
    <span className="qv-tag" style={{ background: t.color, color: cssVar('ink') }}>
      {t.text}
    </span>
  );
}

/** 花费标签 chip。 */
export function CostTag({ usd }: { usd: number }) {
  return (
    <span className="qv-tag" style={{ background: cssVar('wall'), color: cssVar('cream') }}>
      ${usd.toFixed(2)}
    </span>
  );
}
