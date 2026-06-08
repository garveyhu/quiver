import { Px } from '@/assets/primitives';
import { cssVar } from '@/assets/palette';
import type { PropProps } from './prop.types';

const BUILDINGS = [
  { x: 0, w: 30, h: 60, lit: [[8, 40], [18, 24]] },
  { x: 30, w: 24, h: 84, lit: [[40, 60]] },
  { x: 58, w: 34, h: 48, lit: [[70, 30]] },
  { x: 96, w: 26, h: 70, lit: [[106, 50]] },
  { x: 126, w: 38, h: 38, lit: [] },
  { x: 166, w: 30, h: 64, lit: [[176, 42]] },
];

const STARS = [
  [30, 24, 0], [80, 40, 0.8], [120, 18, 1.4], [60, 60, 0.4], [150, 54, 1.1],
];

/** 夜窗:木框 + 夜空 + 月亮 + 星 + 城市天际线 + 十字窗棂 + 两侧窗帘。盒子 234×182。 */
export function StudioWindow({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 234, height: 182, ...style }}>
      <Px left={8} top={4} w={214} h={168} color={cssVar('woodDk')} style={{ boxShadow: '1px 1px 0 #0006' }} />
      <div
        style={{
          position: 'absolute',
          left: 16,
          top: 12,
          width: 198,
          height: 152,
          overflow: 'hidden',
          background: 'linear-gradient(180deg,#0a1130,#10204a)',
        }}
      >
        <Px right={24} top={16} w={26} h={26} color={cssVar('glow')} style={{ borderRadius: '50%', boxShadow: '0 0 12px 4px #ffe6ad55' }} />
        <Px right={18} top={12} w={26} h={26} color="#0c1838" style={{ borderRadius: '50%' }} />
        {STARS.map(([x, y, d], i) => (
          <Px key={i} left={x} top={y} w={2} h={2} color="#fff" className="qv-star" style={{ animationDelay: `${d}s` }} />
        ))}
        {BUILDINGS.map((b, i) => (
          <div key={i}>
            <Px left={b.x} bottom={0} w={b.w} h={b.h} color="#0c1430" />
            {b.lit.map(([lx, ly], j) => (
              <Px key={j} left={lx} bottom={ly} w={3} h={3} color={cssVar('amber')} className="qv-a-twk" style={{ opacity: 0.85 }} />
            ))}
          </div>
        ))}
      </div>
      {/* mullions */}
      <Px left={112} top={12} w={5} h={152} color={cssVar('woodDk')} />
      <Px left={16} top={84} w={198} h={5} color={cssVar('woodDk')} />
      {/* curtains */}
      <Px left={0} top={0} w={18} h={178} color={cssVar('cloth')} style={{ boxShadow: 'inset -3px 0 0 var(--qv-cloth2)' }} />
      <Px left={216} top={0} w={18} h={178} color={cssVar('cloth')} style={{ boxShadow: 'inset 3px 0 0 var(--qv-cloth2)' }} />
      <Px left={-2} top={-4} w={238} h={10} color={cssVar('cloth2')} />
    </div>
  );
}
