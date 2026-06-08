import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

export type ToastKind = 'success' | 'error' | 'info';

const TOAST: Record<ToastKind, { color: string; glyph: string }> = {
  success: { color: cssVar('green'), glyph: '✓' },
  error: { color: cssVar('red'), glyph: '✕' },
  info: { color: cssVar('blue'), glyph: '◔' },
};

export interface ToastProps {
  kind?: ToastKind;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}

/** Toast 通知:合并成功 / 验证失败 / 提示。从右滑入。 */
export function Toast({ kind = 'info', children, className, style }: ToastProps) {
  const t = TOAST[kind];
  return (
    <div className={`qv-toast ${className ?? ''}`.trim()} style={{ borderLeft: `4px solid ${t.color}`, ...style }}>
      <span style={{ color: t.color }}>{t.glyph}</span>
      <span>{children}</span>
    </div>
  );
}

export interface TooltipProps {
  label: ReactNode;
  children: ReactNode;
}

/** 悬浮提示:hover 子元素时在上方显示标签。 */
export function Tooltip({ label, children }: TooltipProps) {
  return (
    <span className="qv-tip-wrap">
      {children}
      <span className="qv-tip">{label}</span>
    </span>
  );
}

/** 加载动画:三点跳动。 */
export function Spinner({ style }: { style?: CSSProperties }) {
  return (
    <span style={{ display: 'inline-flex', gap: 4, alignItems: 'flex-end', ...style }}>
      {[0, 1, 2].map((i) => (
        <span key={i} className="qv-dot" style={{ animationDelay: `${i * 0.15}s` }} />
      ))}
    </span>
  );
}
