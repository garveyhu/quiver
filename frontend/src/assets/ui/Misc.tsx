import type { CSSProperties } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

/** 像素光标(装饰 / 自定义指针)。 */
export function Cursor({ style }: { style?: CSSProperties }) {
  return <span className="qv-cursor" style={style} />;
}

export interface SoundToggleProps {
  on: boolean;
  onChange?: (value: boolean) => void;
  style?: CSSProperties;
}

/** 音效开关。 */
export function SoundToggle({ on, onChange, style }: SoundToggleProps) {
  return (
    <span
      className="qv-navbtn"
      role="switch"
      aria-checked={on}
      title="音效"
      style={{ width: 30, height: 30, color: on ? cssVar('amber') : cssVar('dim2'), ...style }}
      onClick={() => onChange?.(!on)}
    >
      ♪
    </span>
  );
}
