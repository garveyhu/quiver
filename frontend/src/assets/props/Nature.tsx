import { Px } from '@/assets/primitives';
import { cssVar } from '@/assets/palette';
import type { PropProps } from './prop.types';

const LEAF = 'polygon(50% 0,100% 60%,60% 100%,0 70%)';

/** 龟背竹落地盆栽:陶盆 + 三片叶。盒子 36×56。 */
export function Plant({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 36, height: 56, ...style }}>
      <Px left={0} bottom={0} w={22} h={24} color={cssVar('wood2')} style={{ boxShadow: 'inset 0 -4px 0 var(--qv-wood)' }} />
      <Px left={-6} bottom={20} w={12} h={26} color={cssVar('plant')} style={{ clipPath: LEAF }} />
      <Px left={8} bottom={22} w={14} h={30} color={cssVar('plant2')} style={{ clipPath: LEAF }} />
      <Px left={18} bottom={18} w={12} h={24} color={cssVar('plant')} style={{ clipPath: LEAF }} />
    </div>
  );
}

/** 睡觉的猫(会呼吸):身体 + 头 + 耳 + 尾。盒子 34×16。 */
export function Cat({ className, style }: PropProps) {
  return (
    <div className={`qv-a-cat ${className ?? ''}`.trim()} style={{ position: 'absolute', width: 34, height: 16, ...style }}>
      <Px left={2} bottom={0} w={26} h={9} color="#39343f" style={{ borderRadius: 7 }} />
      <Px left={0} bottom={5} w={11} h={10} color="#39343f" style={{ borderRadius: 5 }} />
      <Px left={1} bottom={13} w={3} h={3} color="#39343f" />
      <Px left={7} bottom={13} w={3} h={3} color="#39343f" />
      <Px left={25} bottom={6} w={9} h={3} color="#39343f" style={{ borderRadius: 2 }} />
    </div>
  );
}

/** 扶手椅(可放猫)。盒子 60×60。 */
export function Armchair({ className, style }: PropProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 60, height: 60, ...style }}>
      <Px left={0} bottom={0} w={60} h={34} color="#6b4f7e" style={{ boxShadow: 'inset 0 0 0 3px #573f68' }} />
      <Px left={0} bottom={0} w={10} h={46} color="#573f68" />
      <Px left={50} bottom={0} w={10} h={46} color="#573f68" />
    </div>
  );
}
