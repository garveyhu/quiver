import type { CompletionFx } from '@/hooks/useCompletionFx';
import { iso, type Layout } from '@/office/iso';

interface WorldFxProps {
  layout: Layout;
  fx: CompletionFx;
}

/**
 * 世界内的交付特效(随相机一起缩放):验收那刻在质检台亮一下(绿=通过/红=打回),
 * 通过时再从发货口飞出一只货箱(原型 shipout)。fx 出现即挂载播一次,1.2s 后随 fx 清空卸载。
 */
export function WorldFx({ layout, fx }: WorldFxProps) {
  if (!fx) return null;
  const qa = iso(layout, 11.7, 0.6, 30);
  const ship = iso(layout, 12, 8.3, 10);
  const color = fx === 'ok' ? 'rgba(108,196,122,.9)' : 'rgba(226,96,79,.9)';

  return (
    <>
      <div
        className="worldfx-glow"
        style={{ left: qa.x, top: qa.y, zIndex: 9000, background: `radial-gradient(closest-side, ${color}, transparent 75%)` }}
      />
      {fx === 'ok' && <div className="crate worldfx-ship" style={{ left: ship.x, top: ship.y, zIndex: 9400 }} />}
    </>
  );
}
