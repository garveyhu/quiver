import { Px } from '@/assets/primitives';
import { cssVar } from '@/assets/palette';
import type { PaletteKey } from '@/assets/palette';
import type { PropProps } from './prop.types';

/** 砖墙(后墙质感)。大小由 style 决定。 */
export function BrickWall({ className, style }: PropProps) {
  return <div className={`qv-brick ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }} />;
}

/** 暖色暗角(覆盖全场,出氛围)。铺满父容器。 */
export function Vignette({ className, style }: PropProps) {
  return <div className={`qv-vignette ${className ?? ''}`.trim()} style={style} />;
}

/** 月光束 / 灯光柱(斜向半透明光)。大小由 style 决定。 */
export function MoonBeam({ className, style }: PropProps) {
  return <div className={`qv-beam ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }} />;
}

/** 暖光池(灯/炉投在墙地的柔光团,会呼吸)。大小由 style 决定。 */
export function LightPool({ className, style }: PropProps) {
  return <div className={`qv-pool ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }} />;
}

export interface FairyLightsProps extends PropProps {
  width?: number;
  count?: number;
}

/** 顶部串灯:一条线 + 一排会眨的暖灯泡。 */
export function FairyLights({ width = 320, count = 11, className, style }: FairyLightsProps) {
  return (
    <div className={className} style={{ position: 'absolute', width, height: 16, ...style }}>
      <Px left={0} top={2} w={width} h={2} color={cssVar('wall')} style={{ opacity: 0.6 }} />
      {Array.from({ length: count }).map((_, i) => (
        <div
          key={i}
          className="qv-fairy"
          style={{ left: Math.round((i + 0.5) * (width / count)), top: i % 2 ? 7 : 5, animationDelay: `${(i % 5) * 0.4}s` }}
        />
      ))}
    </div>
  );
}

const NOTES: [number, number, PaletteKey][] = [
  [8, 10, 'amber'], [8, 30, 'pink'], [8, 50, 'green'],
  [44, 10, 'blue'], [44, 32, 'amber'],
  [80, 12, 'pink'], [80, 34, 'blue'],
];

/** 看板白板:三栏 + 便签。Quiver 的任务隐喻——后墙挂一块正合适。盒子 110×72。 */
export function Kanban({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 110, height: 72, ...style }}>
      <Px left={0} top={0} w={110} h={72} color={cssVar('board')} style={{ boxShadow: 'inset 0 0 0 3px #b9b09a,2px 2px 0 #0005' }} />
      <Px left={37} top={6} w={2} h={60} color="#c9c0aa" />
      <Px left={73} top={6} w={2} h={60} color="#c9c0aa" />
      {NOTES.map(([x, y, c], i) => (
        <Px key={i} left={x} top={y} w={22} h={16} color={cssVar(c)} style={{ boxShadow: '1px 1px 0 #0004' }} />
      ))}
    </div>
  );
}

export interface NeonSignProps extends PropProps {
  label?: string;
}

/** 霓虹招牌(发光,会呼吸)。默认 `</>`,cozy-tech 点缀。 */
export function NeonSign({ label = '</>', className, style }: NeonSignProps) {
  return (
    <div className={`qv-neon-sign ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }}>
      {label}
    </div>
  );
}

/** 咖啡角:吧台柜 + 意式咖啡机(暖指示灯 + 蒸汽)+ 杯子 + 暖光。盒子 80×90。 */
export function CoffeeStation({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 80, height: 90, ...style }}>
      <Px left={6} bottom={0} w={68} h={34} color={cssVar('wood')} style={{ boxShadow: 'inset 0 -4px 0 var(--qv-wood-dk)' }} />
      <Px left={0} bottom={34} w={80} h={6} color={cssVar('wood2')} />
      <Px left={14} bottom={40} w={40} h={30} color="#3a3f55" style={{ boxShadow: 'inset 0 0 0 2px #4a5170' }} />
      <Px left={12} bottom={70} w={44} h={6} color="#2a2f44" />
      <Px left={18} bottom={58} w={5} h={5} color={cssVar('amber')} className="qv-a-pulse" style={{ borderRadius: '50%', boxShadow: '0 0 6px 2px #ffd27a99' }} />
      <Px left={30} bottom={44} w={10} h={8} color="#5a627e" />
      <Px left={32} bottom={40} w={6} h={5} color={cssVar('cream')} />
      <div className="qv-a-steam" style={{ position: 'absolute', left: 35, bottom: 52, width: 3, height: 5, background: '#cfe' }} />
      <Px left={60} bottom={40} w={8} h={7} color={cssVar('cream')} style={{ boxShadow: '1px 1px 0 #0005' }} />
      <div className="qv-glow" style={{ position: 'absolute', left: 6, bottom: 34, width: 70, height: 30, background: cssVar('amber') }} />
    </div>
  );
}

/** 垂吊绿植(从顶部/隔板挂下,藤蔓垂落)。盒子 40×60(顶端为挂点)。 */
export function HangingPlant({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 40, height: 60, ...style }}>
      <Px left={10} top={4} w={20} h={12} color={cssVar('wood2')} />
      <Px left={5} top={-4} w={30} h={12} color={cssVar('plant2')} style={{ borderRadius: '50%' }} />
      <Px left={11} top={14} w={3} h={40} color={cssVar('plant')} />
      <Px left={20} top={14} w={3} h={52} color={cssVar('plant2')} />
      <Px left={28} top={14} w={3} h={34} color={cssVar('plant')} />
      <Px left={8} top={36} w={6} h={6} color={cssVar('plant2')} style={{ borderRadius: '50%' }} />
      <Px left={26} top={30} w={6} h={6} color={cssVar('plant')} style={{ borderRadius: '50%' }} />
      <Px left={18} top={52} w={6} h={6} color={cssVar('plant2')} style={{ borderRadius: '50%' }} />
    </div>
  );
}
