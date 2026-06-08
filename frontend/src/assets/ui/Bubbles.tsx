import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

export interface SpeechBubbleProps {
  children: ReactNode;
  /** 深色气泡(智能体推理/系统),默认浅色(回答) */
  dark?: boolean;
  className?: string;
  style?: CSSProperties;
}

/** 输出气泡:浮在小工头顶,显示智能体最新一句(OutputChunk)。带小尾巴 + 弹出动画。 */
export function SpeechBubble({ children, dark, className, style }: SpeechBubbleProps) {
  return (
    <div className={`qv-bubble ${dark ? 'dark' : ''} ${className ?? ''}`.trim()} style={style}>
      {children}
    </div>
  );
}

/** 状态徽标种类 → 颜色 + 字形(映射 AgentEvent / sprite 状态机)。 */
export type StatusKind = 'await' | 'paused' | 'throttle' | 'verified' | 'failed' | 'retry' | 'budget';

const STATUS: Record<StatusKind, { color: string; glyph: string }> = {
  await: { color: cssVar('amber'), glyph: '!' },
  paused: { color: cssVar('blue'), glyph: '⏸' },
  throttle: { color: cssVar('blue'), glyph: '☕' },
  verified: { color: cssVar('green'), glyph: '✓' },
  failed: { color: cssVar('red'), glyph: '✕' },
  retry: { color: cssVar('amber2'), glyph: '⟳' },
  budget: { color: cssVar('amber2'), glyph: '$' },
};

export interface StatusBubbleProps {
  kind: StatusKind;
  /** 附加文字(如倒计时、金额) */
  label?: string;
  className?: string;
  style?: CSSProperties;
}

/** 状态气泡:浮在小工头顶的小徽标(等你处理 / 限流 / 已合并 / 失败 / 重试 / 预算)。 */
export function StatusBubble({ kind, label, className, style }: StatusBubbleProps) {
  const s = STATUS[kind];
  return (
    <div className={`qv-badge ${className ?? ''}`.trim()} style={{ background: s.color, ...style }}>
      <span>{s.glyph}</span>
      {label && <span style={{ marginLeft: 3 }}>{label}</span>}
    </div>
  );
}

export interface CountdownBubbleProps {
  /** 形如 "4:32" 的倒计时(RateLimited resetsAt) */
  time: string;
  className?: string;
  style?: CSSProperties;
}

/** 咖啡倒计时气泡:短时限流(可等过去的那种)专用。 */
export function CountdownBubble({ time, className, style }: CountdownBubbleProps) {
  return (
    <div className={`qv-badge ${className ?? ''}`.trim()} style={{ background: cssVar('blue'), ...style }}>
      ☕ {time}
    </div>
  );
}
