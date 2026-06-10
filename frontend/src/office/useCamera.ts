import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';

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
  /** 飞入聚焦到世界某点(点小人下钻用) */
  diveTo: (x: number, y: number) => void;
  /** 复位到 fit */
  reset: () => void;
  /** world 局部坐标 → 屏幕(视口)坐标。屏幕空间浮层(房间标签)据此定位 —— 跟着相机走但
   * 文字本身不被 transform:scale 缩放,所以 DPR=1 也清晰。 */
  project: (wx: number, wy: number) => { x: number; y: number };
}

/** 下钻聚焦的放大倍率(相对 fit)。 */
const DIVE = 6;

/** 把整张办公室收进视口的基准缩放(留 0.4 下限)。 */
function computeFit(worldW: number, worldH: number): number {
  // 放大填满窗口。去掉了 .world 的 will-change:transform 后,静态缩放会按目标尺寸重新栅格化
  // (放大也清晰),所以可以放心放大填满,不再有纹理上采样模糊。
  return Math.max(0.4, Math.min(window.innerWidth / worldW, window.innerHeight / worldH));
}

/**
 * 连续缩放相机(移植原型 cam/applyCam/fit/wheel/dive):
 * 滚轮朝光标缩放(每帧一次,clamp [fit, fit*10]),点小人 diveTo 飞入聚焦(fit*6),
 * Esc / 双击复位。叠层(命令栏/面板/worksurf)打开时不抢滚轮。
 */
export function useCamera(worldW: number, worldH: number): Camera {
  const fitRef = useRef(computeFit(worldW, worldH));
  const lockRef = useRef(false);
  const [cam, setCam] = useState<CamState>(() => ({ s: fitRef.current, fx: worldW / 2, fy: worldH / 2 }));
  const [anim, setAnim] = useState<Anim>('none');

  const reset = useCallback(() => {
    setAnim('smooth');
    setCam({ s: fitRef.current, fx: worldW / 2, fy: worldH / 2 });
  }, [worldW, worldH]);

  const diveTo = useCallback((x: number, y: number) => {
    setAnim('smooth');
    setCam({ s: fitRef.current * DIVE, fx: x, fy: y });
  }, []);

  // 窗口尺寸变化:重算 fit 并复位。
  useEffect(() => {
    const onResize = () => {
      fitRef.current = computeFit(worldW, worldH);
      reset();
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [worldW, worldH, reset]);

  // 滚轮朝光标缩放(每帧至多一次)。
  useEffect(() => {
    const onWheel = (e: WheelEvent) => {
      if (document.querySelector('.panel.on, .cmdk.on, .worksurf.on')) return;
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
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') reset();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('dblclick', reset);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('dblclick', reset);
    };
  }, [reset]);

  // 整数像素对齐:DPR=1(非 retina)屏上,小数 translate 让内容子像素渲染 → 文字/像素发糊。
  // round 到整数物理像素能让平移不引入额外模糊。
  const tx = Math.round(-(cam.fx - worldW / 2) * cam.s);
  const ty = Math.round(-(cam.fy - worldH / 2) * cam.s);
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
    diveTo,
    reset,
    // .scene-fit 充满视口、world 居中: 屏幕坐标 = 视口中心 + (world点 - world中心)×缩放 + 平移。
    project: (wx: number, wy: number) => ({
      x: Math.round(window.innerWidth / 2 + (wx - worldW / 2) * cam.s + tx),
      y: Math.round(window.innerHeight / 2 + (wy - worldH / 2) * cam.s + ty),
    }),
  };
}
