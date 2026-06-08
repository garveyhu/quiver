import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

export interface AchievementProps {
  /** 左侧字形(默认 ★) */
  glyph?: string;
  title: ReactNode;
  sub?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

/** 成就 / 升级弹窗(发光呼吸 + 弹出)。庆祝合并、升级时用。 */
export function Achievement({ glyph = '★', title, sub, className, style }: AchievementProps) {
  return (
    <div className={`qv-ach ${className ?? ''}`.trim()} style={style}>
      <span style={{ color: cssVar('amber'), fontSize: 20 }}>{glyph}</span>
      <span>
        <div style={{ font: 'bold 12px ui-monospace,monospace', color: cssVar('amber') }}>{title}</div>
        {sub && <div style={{ font: '10px ui-monospace,monospace', color: cssVar('dim'), marginTop: 2 }}>{sub}</div>}
      </span>
    </div>
  );
}

export interface XpFloatProps {
  /** 如 "+10 XP" 或 "+$0.42" */
  amount: string;
  color?: string;
  className?: string;
  style?: CSSProperties;
}

/** 飘字:+XP / +$ 从头顶升起淡出。 */
export function XpFloat({ amount, color, className, style }: XpFloatProps) {
  return (
    <span className={`qv-xpfloat ${className ?? ''}`.trim()} style={{ color, ...style }}>
      {amount}
    </span>
  );
}
