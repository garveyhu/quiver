import { useEffect, useRef } from 'react';

interface AtmosphereProps {
  /** 今夜(滚动 24h)花费,驱动预算染色 */
  spentUsd: number;
  /** 预算上限;null = 未设(不做预算染色) */
  budgetCapUsd: number | null;
}

type Rgb = readonly [number, number, number];

const lerp3 = (a: Rgb, b: Rgb, k: number): Rgb => [
  Math.round(a[0] + (b[0] - a[0]) * k),
  Math.round(a[1] + (b[1] - a[1]) * k),
  Math.round(a[2] + (b[2] - a[2]) * k),
];

// 夜色三段:黄昏 → 深夜 → 黎明;地平线暖冷过渡(移植原型 nightTick)。
const DUSK: Rgb = [42, 34, 58];
const DEEP: Rgb = [8, 14, 34];
const DAWN: Rgb = [20, 34, 64];
const HOT: Rgb = [120, 72, 46];
const COOL: Rgb = [26, 40, 78];
const COLD: Rgb = [36, 58, 96];
const COOLTOP: Rgb = [10, 16, 40];

/**
 * 画面后期氛围层:整夜缓慢循环的夜空(「夜色呼吸」)+ 预算=能量染色(花费逼近上限→全场转冷转暗,
 * 临界亮红边)。忠实移植原型 nightTick / budgetTick,用 ref + interval 驱动,不触发整树重渲染。
 * 预算口径接 §8.3 的真实今夜花费。
 */
export function Atmosphere({ spentUsd, budgetCapUsd }: AtmosphereProps) {
  const skyRef = useRef<HTMLDivElement>(null);
  const tintRef = useRef<HTMLDivElement>(null);
  const edgeRef = useRef<HTMLDivElement>(null);
  // 最新花费放进 ref,让 budgetTick 间隔回调始终读到当前值而不重建定时器。
  const spentRef = useRef(spentUsd);
  spentRef.current = spentUsd;
  const capRef = useRef(budgetCapUsd);
  capRef.current = budgetCapUsd;

  useEffect(() => {
    let nightT = 0;
    const nightTick = () => {
      nightT = (nightT + 0.45) % 100;
      const p = nightT / 100;
      let c: Rgb;
      let op: number;
      let horiz: Rgb;
      let hk: number;
      if (p < 0.45) {
        const k = p / 0.45;
        c = lerp3(DUSK, DEEP, k);
        op = 0.1 + 0.34 * k;
        horiz = lerp3(HOT, COOL, k);
        hk = 0.34 * (1 - k) + 0.06;
      } else if (p < 0.82) {
        const k = (p - 0.45) / 0.37;
        c = lerp3(DEEP, DAWN, k);
        op = 0.44 - 0.18 * k;
        horiz = lerp3(COOL, COLD, k);
        hk = 0.06 + 0.05 * k;
      } else {
        const k = (p - 0.82) / 0.18;
        c = lerp3(DAWN, DUSK, k);
        op = 0.26 - 0.16 * k;
        horiz = lerp3(COLD, HOT, k);
        hk = 0.11 + 0.26 * k;
      }
      const top = lerp3(c, COOLTOP, 0.45);
      if (skyRef.current) {
        skyRef.current.style.background =
          `linear-gradient(180deg, rgba(${top[0]},${top[1]},${top[2]},${(op * 0.9).toFixed(3)}) 0%,` +
          ` rgba(${c[0]},${c[1]},${c[2]},${op.toFixed(3)}) 48%,` +
          ` rgba(${horiz[0]},${horiz[1]},${horiz[2]},${hk.toFixed(3)}) 100%)`;
      }
    };

    const budgetTick = () => {
      const cap = capRef.current;
      // 未设预算上限 → 不做"逼近上限"的转冷转暗。
      const f = cap ? Math.min(1, spentRef.current / cap) : 0;
      const e = Math.sqrt(f);
      if (tintRef.current) tintRef.current.style.opacity = (e * 0.55).toFixed(3);
      const edge = edgeRef.current;
      if (!edge) return;
      if (f > 0.78) {
        const re = ((f - 0.78) / 0.22) * 0.45 + 0.12;
        edge.style.setProperty('--re', re.toFixed(3));
        edge.style.opacity = re.toFixed(3);
        edge.classList.add('pulse');
      } else {
        edge.classList.remove('pulse');
        edge.style.opacity = '0';
      }
    };

    nightTick();
    budgetTick();
    const nightId = window.setInterval(nightTick, 650);
    const budgetId = window.setInterval(budgetTick, 800);
    return () => {
      window.clearInterval(nightId);
      window.clearInterval(budgetId);
    };
  }, []);

  return (
    <>
      <div id="sky" ref={skyRef} />
      <div id="budgetTint" ref={tintRef} />
      <div id="rededge" ref={edgeRef} />
    </>
  );
}
