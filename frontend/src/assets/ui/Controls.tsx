import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

export interface SegmentedControlProps {
  options: string[];
  value: number;
  onChange?: (index: number) => void;
}

/** 分段控件(如 模拟运行 / 真实 Claude)。 */
export function SegmentedControl({ options, value, onChange }: SegmentedControlProps) {
  return (
    <div className="qv-seg">
      {options.map((o, i) => (
        <button key={i} className={i === value ? 'active' : ''} onClick={() => onChange?.(i)}>
          {o}
        </button>
      ))}
    </div>
  );
}

/** 快捷键芯片。 */
export function Kbd({ children }: { children: ReactNode }) {
  return <span className="qv-kbd">{children}</span>;
}

/** 分隔线。 */
export function Divider({ style }: { style?: CSSProperties }) {
  return <hr className="qv-divider" style={style} />;
}

export type BannerKind = 'info' | 'warn' | 'danger';

const BANNER: Record<BannerKind, string> = {
  info: cssVar('blue'),
  warn: cssVar('amber'),
  danger: cssVar('red'),
};

export interface BannerProps {
  kind?: BannerKind;
  children: ReactNode;
  action?: ReactNode;
  style?: CSSProperties;
}

/** 顶部横幅(如预算到顶:闭店—队列已暂停)。 */
export function Banner({ kind = 'warn', children, action, style }: BannerProps) {
  return (
    <div className="qv-banner" style={{ background: BANNER[kind], ...style }}>
      <span style={{ flex: 1 }}>{children}</span>
      {action}
    </div>
  );
}

export interface StatTileProps {
  value: ReactNode;
  label: ReactNode;
  glyph?: string;
  color?: string;
  style?: CSSProperties;
}

/** HUD 统计块(任务数 / 花费 / XP 等)。 */
export function StatTile({ value, label, glyph, color, style }: StatTileProps) {
  return (
    <div className="qv-stat" style={style}>
      <div style={{ font: 'bold 18px ui-monospace,monospace', color: color ?? cssVar('amber') }}>
        {glyph && <span style={{ marginRight: 4 }}>{glyph}</span>}
        {value}
      </div>
      <div style={{ font: '10px ui-monospace,monospace', color: cssVar('dim'), marginTop: 2 }}>{label}</div>
    </div>
  );
}
