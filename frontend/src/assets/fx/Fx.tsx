import type { CSSProperties } from 'react';
import { Px } from '@/assets/primitives';
import { cssVar } from '@/assets/palette';

interface FxProps {
  className?: string;
  style?: CSSProperties;
}

/** 暖光晕(灯/炉/屏共用)。大小用 style 的 width/height。 */
export function Glow({ className, style }: FxProps) {
  return <div className={`qv-glow ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }} />;
}

/** 单颗闪烁星。 */
export function Star({ className, style }: FxProps) {
  return <div className={`qv-star ${className ?? ''}`.trim()} style={{ position: 'absolute', ...style }} />;
}

/** 漂浮尘埃光点。 */
export function Dust({ className, style }: FxProps) {
  return (
    <div
      className={`qv-a-floaty ${className ?? ''}`.trim()}
      style={{ position: 'absolute', width: 2, height: 2, borderRadius: '50%', background: cssVar('glow'), ...style }}
    />
  );
}

/** 上升热气(咖啡/茶)。 */
export function Steam({ className, style }: FxProps) {
  return (
    <div
      className={`qv-a-steam ${className ?? ''}`.trim()}
      style={{ position: 'absolute', width: 3, height: 5, background: '#cfe', ...style }}
    />
  );
}

/** "!" 气泡:等你处理(AwaitingPermission)。盒子 36×22。 */
export function Bubble({ className, style }: FxProps) {
  return (
    <div className={className} style={{ position: 'absolute', width: 36, height: 22, ...style }}>
      <Px left={0} top={0} w={36} h={22} color={cssVar('cream')} style={{ boxShadow: '1px 1px 0 #0006' }} />
      <Px left={16} top={4} w={3} h={10} color={cssVar('ink')} />
      <Px left={16} top={16} w={3} h={3} color={cssVar('ink')} />
    </div>
  );
}
