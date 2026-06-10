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
}

/** 下钻聚焦的放大倍率(相对 fit)。 */
const DIVE = 6;

/** 把整张办公室收进视口的基准缩放(留 0.4 下限)。 */
function computeFit(worldW: number, worldH: number): number {
  // 上限 1:**绝不放大世界**。world 用 transform:scale,放大(>1)会把整个 world 栅格化成纹理
  // 再 GPU 上采样 → 文字/像素发糊(控制条不在 world 里所以一直清晰)。世界比窗口小就 1:1 居中
  // 显示、周围留深色空白(沉浸),想看近景自己滚轮放大(用户主动,糊也认了)。
  return Math.max(0.4, Math.min(1, window.innerWidth / worldW, window.innerHeight / worldH));
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
  };
}
