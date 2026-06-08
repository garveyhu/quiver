import type { CSSProperties } from 'react';
import '../pixel.css';

const GLYPH = {
  gear: '⚙',
  play: '▶',
  pause: '⏸',
  stop: '■',
  check: '✓',
  close: '✕',
  plus: '＋',
  retry: '⟳',
  search: '⌕',
  doc: '▤',
  folder: '▰',
  book: '▥',
  coin: '◉',
  bell: '◔',
  star: '★',
} as const;

export type IconName = keyof typeof GLYPH;

export interface IconProps {
  name: IconName;
  size?: number;
  color?: string;
  className?: string;
  style?: CSSProperties;
}

/** 像素风图标(等宽字形)。 */
export function Icon({ name, size = 14, color, className, style }: IconProps) {
  return (
    <span className={`qv-icon ${className ?? ''}`.trim()} style={{ fontSize: size, color, ...style }}>
      {GLYPH[name]}
    </span>
  );
}
