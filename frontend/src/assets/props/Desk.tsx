import { Px } from '@/assets/primitives';
import { cssVar } from '@/assets/palette';
import type { PropProps } from './prop.types';

export interface DeskProps extends PropProps {
  /** 屏幕辉光色:工作=screen(蓝),紧张=red */
  screen?: 'screen' | 'red';
  /** 屏幕辉光动画相位错开,多桌不同步 */
  glowDelay?: string;
}

/** 工位:桌面 + 桌腿 + 显示器(带辉光)+ 键盘。盒子 104×60(自底对齐摆放)。 */
export function Desk({ screen = 'screen', glowDelay, className, style }: DeskProps) {
  const glow = screen === 'red' ? '#e2604f44' : '#8fd0ff44';
  return (
    <div className={className} style={{ position: 'absolute', width: 104, height: 60, ...style }}>
      <Px left={6} bottom={0} w={5} h={18} color={cssVar('woodDk')} />
      <Px left={94} bottom={0} w={5} h={18} color={cssVar('woodDk')} />
      <Px left={0} bottom={14} w={104} h={14} color={cssVar('wood')} />
      <Px left={16} bottom={28} w={36} h={26} color="#1a1f33" style={{ boxShadow: '0 0 0 2px #0006' }} />
      <Px
        left={20}
        bottom={32}
        w={28}
        h={18}
        color={cssVar(screen)}
        className="qv-screen-glow"
        style={{ boxShadow: `0 0 12px 4px ${glow}`, animationDelay: glowDelay }}
      />
      <Px left={22} bottom={24} w={26} h={4} color="#3a3550" />
    </div>
  );
}
