import type { CSSProperties, ReactNode } from 'react';

export interface PxProps {
  left?: number;
  right?: number;
  top?: number;
  bottom?: number;
  w?: number;
  h?: number;
  color?: string;
  className?: string;
  style?: CSSProperties;
  children?: ReactNode;
}

/**
 * 一个绝对定位的像素块——所有像素道具/场景的最小积木。
 * 位置用数字(px),颜色用 `cssVar('amber')` 之类的 token,不写颜色字面量。
 */
export function Px({ left, right, top, bottom, w, h, color, className, style, children }: PxProps) {
  return (
    <div
      className={className}
      style={{
        position: 'absolute',
        left,
        right,
        top,
        bottom,
        width: w,
        height: h,
        background: color,
        ...style,
      }}
    >
      {children}
    </div>
  );
}
