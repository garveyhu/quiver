import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

export interface ProgressRingProps {
  /** 0–100 */
  pct: number;
  size?: number;
  color?: string;
  label?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

/** 环形进度(验证关进度 / 预算环)。 */
export function ProgressRing({ pct, size = 56, color, label, className, style }: ProgressRingProps) {
  const vars = { '--qv-ringp': Math.max(0, Math.min(100, pct)), '--qv-ringc': color } as CSSProperties;
  return (
    <div className={`qv-ring ${className ?? ''}`.trim()} style={{ width: size, height: size, ...vars, ...style }}>
      <div className="qv-ring-hole">{label ?? `${Math.round(pct)}%`}</div>
    </div>
  );
}

export interface ProgressBarProps {
  pct: number;
  width?: number;
  height?: number;
  color?: string;
  className?: string;
  style?: CSSProperties;
}

/** 线性进度条(通用)。 */
export function ProgressBar({ pct, width = 160, height = 12, color, className, style }: ProgressBarProps) {
  return (
    <div className={`qv-bar ${className ?? ''}`.trim()} style={{ width, height, ...style }}>
      <div className="qv-bar-fill" style={{ width: `${Math.max(0, Math.min(100, pct))}%`, background: color ?? cssVar('amber') }} />
    </div>
  );
}
