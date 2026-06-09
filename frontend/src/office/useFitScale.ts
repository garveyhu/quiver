import { useEffect, useState } from 'react';

/** 让整张办公室缩放到刚好收进窗口(留最小 0.4 倍下限)，复刻原型 fit()。 */
function compute(worldW: number, worldH: number): number {
  return Math.max(0.4, Math.min(window.innerWidth / worldW, window.innerHeight / worldH));
}

/** 监听窗口尺寸，返回把 world 收进视口的缩放系数。 */
export function useFitScale(worldW: number, worldH: number): number {
  const [scale, setScale] = useState(() => compute(worldW, worldH));
  useEffect(() => {
    const onResize = () => setScale(compute(worldW, worldH));
    onResize();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [worldW, worldH]);
  return scale;
}
