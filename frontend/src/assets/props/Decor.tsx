import { Px } from '@/assets/primitives';
import { cssVar } from '@/assets/palette';
import type { PropProps } from './prop.types';

const BOOKS = [
  [0, 22, '#e2748a'], [9, 26, '#7aa7e6'], [18, 20, '#7bbf7e'], [27, 24, '#ffd27a'],
  [36, 22, '#e2604f'], [52, 24, '#b07a8e'], [61, 20, '#7aa7e6'], [70, 26, '#4f8a5a'],
];

/** 挂架书堆:木板 + 一排彩色书脊。盒子 90×32。 */
export function Bookshelf({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 90, height: 32, ...style }}>
      {BOOKS.map(([x, h, c], i) => (
        <Px key={i} left={x as number} bottom={6} w={8} h={h as number} color={c as string} style={{ boxShadow: '1px 0 0 #0005' }} />
      ))}
      <Px left={-4} bottom={0} w={98} h={6} color={cssVar('wood')} style={{ boxShadow: '0 3px 0 #0004' }} />
    </div>
  );
}

/** 地毯(俯视一条):同心边框。盒子由 style 的 width/height 决定。 */
export function Rug({ className, style }: PropProps) {
  return <div className={`qv-rug-el ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }} />;
}

/** 落地灯:灯杆 + 灯罩 + 暖光晕。盒子 36×100。 */
export function FloorLamp({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 36, height: 100, ...style }}>
      <Px left={16} bottom={0} w={4} h={88} color={cssVar('woodDk')} />
      <Px left={6} bottom={88} w={24} h={14} color={cssVar('amber')} style={{ clipPath: 'polygon(20% 0,80% 0,100% 100%,0 100%)' }} />
      <Px left={0} bottom={72} w={36} h={30} color={cssVar('amber')} className="qv-glow" />
    </div>
  );
}

/** 挂钟:表盘 + 分/时针(缓慢转)。盒子 36×36。 */
export function WallClock({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 36, height: 36, ...style }}>
      <Px left={0} top={0} w={36} h={36} color="#1a1f33" style={{ borderRadius: '50%', boxShadow: 'inset 0 0 0 3px #cbb89a' }} />
      <Px left={17} top={6} w={2} h={12} color="#cbb89a" className="qv-a-spin-slow" style={{ transformOrigin: 'bottom' }} />
      <Px left={17} top={10} w={2} h={8} color="#cbb89a" style={{ transformOrigin: 'bottom', animation: 'qv-spin 120s linear infinite' }} />
    </div>
  );
}

export interface PendantLightProps extends PropProps {
  /** 吊线长度 px */
  drop?: number;
}

/** 吊灯:吊线 + 暖光灯泡。 */
export function PendantLight({ drop = 30, className, style }: PendantLightProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 8, height: drop + 8, ...style }}>
      <Px left={3} top={0} w={2} h={drop} color={cssVar('wall')} />
      <Px left={0} top={drop} w={8} h={8} color={cssVar('amber')} className="qv-a-pulse" style={{ borderRadius: '50%', boxShadow: '0 0 10px 5px #ffd27a55' }} />
    </div>
  );
}
