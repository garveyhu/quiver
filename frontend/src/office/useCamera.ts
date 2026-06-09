import { useEffect, useRef, useState, type CSSProperties } from 'react';

interface CamState {
  /** 缩放系数 */
  s: number;
  /** 焦点世界坐标 x */
  fx: number;
  /** 焦点世界坐标 y */
  fy: number;
}

type Anim = 'none' | 'fast' | 'smooth';

export interface Camera {
  worldStyle: CSSProperties;
  /** 是否已放大(显示缩放提示) */
  zoomed: boolean;
}

/** 把整张办公室收进视口的基准缩放(留 0.4 下限)。 */
function computeFit(worldW: number, worldH: number): number {
  return Math.max(0.4, Math.min(window.innerWidth / worldW, window.innerHeight / worldH));
}

/**
 * 连续缩放相机(移植原型 cam/applyCam/fit/wheel):
 * 滚轮朝光标缩放(clamp 到 [fit, fit*10]),Esc / 双击复位到 fit;窗口尺寸变化重算 fit。
 * 叠层(命令栏/面板)打开时不抢滚轮。语义层级(模糊→工作台 worksurf)、点小人 dive 留后续刀。
 */
export function useCamera(worldW: number, worldH: number): Camera {
  const fitRef = useRef(computeFit(worldW, worldH));
  const lockRef = useRef(false);
  const [cam, setCam] = useState<CamState>(() => ({ s: fitRef.current, fx: worldW / 2, fy: worldH / 2 }));
  const [anim, setAnim] = useState<Anim>('none');

  // 窗口尺寸变化:重算 fit 并复位。
  useEffect(() => {
    const onResize = () => {
      fitRef.current = computeFit(worldW, worldH);
      setAnim('none');
      setCam({ s: fitRef.current, fx: worldW / 2, fy: worldH / 2 });
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [worldW, worldH]);

  // 滚轮朝光标缩放(每帧至多一次)。
  useEffect(() => {
    const onWheel = (e: WheelEvent) => {
      if (document.querySelector('.panel.on, .cmdk.on')) return;
      e.preventDefault();
      if (lockRef.current) return;
      lockRef.current = true;
      requestAnimationFrame(() => {
        lockRef.current = false;
      });
      const vcx = window.innerWidth / 2;
      const vcy = window.innerHeight / 2;
      setCam(c => {
        const wx = c.fx + (e.clientX - vcx) / c.s;
        const wy = c.fy + (e.clientY - vcy) / c.s;
        const fit = fitRef.current;
        const ns = Math.max(fit, Math.min(fit * 10, c.s * (e.deltaY < 0 ? 1.16 : 1 / 1.16)));
        return { s: ns, fx: wx - (e.clientX - vcx) / ns, fy: wy - (e.clientY - vcy) / ns };
      });
      setAnim('fast');
    };
    window.addEventListener('wheel', onWheel, { passive: false });
    return () => window.removeEventListener('wheel', onWheel);
  }, []);

  // Esc / 双击复位到 fit。
  useEffect(() => {
    const reset = () => {
      setAnim('smooth');
      setCam({ s: fitRef.current, fx: worldW / 2, fy: worldH / 2 });
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') reset();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('dblclick', reset);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('dblclick', reset);
    };
  }, [worldW, worldH]);

  const tx = (-(cam.fx - worldW / 2) * cam.s).toFixed(1);
  const ty = (-(cam.fy - worldH / 2) * cam.s).toFixed(1);
  const transition = anim === 'none' ? 'none' : anim === 'fast' ? 'transform .13s ease-out' : 'transform .6s cubic-bezier(.45,.02,.2,1)';

  return {
    worldStyle: {
      width: worldW,
      height: worldH,
      transform: `translate(${tx}px,${ty}px) scale(${cam.s.toFixed(3)})`,
      transition,
      transformOrigin: 'center',
    },
    zoomed: cam.s > fitRef.current * 1.02,
  };
}
