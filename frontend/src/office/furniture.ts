import { TW, zidx } from '@/office/iso';
import { SceneBuilder, WOOD2 } from '@/office/primitives';

/**
 * 静态家具 —— 忠实移植 redesign-iso-directions.html 各 draw 函数，逐件拼到 SceneBuilder。
 * 本轮先搬工位区(deskUnit/serverRack/whiteboard/deskLamp/plant/pottedShelf/windowWall/
 * wallPoster/ceilDuct)；屏幕高亮/工人光池等动态部分留给后续接事件的切片。
 * 这里的逐像素色值是原型的美术数据，原样保留。
 */

/** 一套工位:椅 + 木桌 + 显示器 + 键盘 + 咖啡 + 散纸 + 桌下走线。 */
export function deskUnit(b: SceneBuilder, c: number, r: number): void {
  // 工位椅(冷紫，带椅背高光)
  b.isoBox(c + 0.05, r - 0.55, 0.45, 0.45, 9, { top: '#4a3a5a', l: '#2d2138', r: '#3a2c4a' });
  b.chip(c + 0.07, r - 0.53, 0.4, 0.08, 9, 17, { top: '#5a4a6e', l: '#352846', r: '#473760' });
  // 桌面接触影 + 桌体(木纹三阶 + 顶面高光纹)
  b.contactShadow(c + 0.04, r + 0.16, 46, 22);
  const z = b.isoBox(c - 0.45, r - 0.15, 0.95, 0.6, 11, WOOD2);
  b.southPanel(c - 0.45, r - 0.15, 0.95, 0, 11, '#4a3628', z);
  b.southPanel(c - 0.44, r - 0.15, 0.93, 8, 11, '#b98a5e', z + 1);
  // 显示器底座 + 屏幕
  b.isoBox(c - 0.1, r - 0.05, 0.1, 0.1, 11, { top: '#2a2f3e', l: '#15131a', r: '#20242f' });
  b.chip(c - 0.16, r - 0.05, 0.32, 0.06, 11, 13, { top: '#3a3f50', l: '#1c2030', r: '#2a3040' });
  b.southPanel(c - 0.46, r - 0.06, 0.8, 17, 33, '#15131a', z + 2);
  b.southPanel(c - 0.42, r - 0.05, 0.72, 19, 31, '#2a3a55', z + 3);
  const sp = b.pt(c - 0.42, r - 0.05, 31);
  b.pxrect(sp.x, sp.y, 16, 12, '', z + 4, 'pscan');
  // 键盘 + 键帽高光
  b.chip(c - 0.34, r + 0.12, 0.42, 0.16, 11, 12.5, { top: '#cdd2dc', l: '#7a8090', r: '#9aa0ac' });
  for (let k = 0; k < 4; k++) b.led(c - 0.3 + k * 0.08, r + 0.16, 13, 'rgba(255,255,255,.5)', false, z + 6);
  // 咖啡杯 + 散落纸张
  b.contactShadow(c + 0.28, r + 0.16, 9, 5, z + 5);
  b.chip(c + 0.24, r + 0.1, 0.1, 0.1, 11, 15, { top: '#d9d3c4', l: '#8a8276', r: '#aaa294' });
  b.led(c + 0.29, r + 0.155, 15, '#3a2c20', false, z + 7);
  b.chip(c - 0.4, r + 0.2, 0.16, 0.12, 11, 11.6, { top: '#e7e1cf', l: '#b9b3a2', r: '#cfc9b6' });
  b.chip(c - 0.34, r + 0.24, 0.16, 0.12, 11, 11.4, { top: '#dcd6c4', l: '#aaa493', r: '#c4beac' });
  // 桌下走线
  const ca = b.pt(c - 0.4, r + 0.45, 11);
  const cb = b.pt(c - 0.4, r + 0.45, 0);
  b.poly([{ x: ca.x - 1, y: ca.y }, { x: ca.x + 1, y: ca.y }, { x: cb.x + 1, y: cb.y }, { x: cb.x - 1, y: cb.y }], '#15131a', z + 2);
}

/** 服务器机架:机箱 + 8 层抽屉 + 状态灯阵。 */
export function serverRack(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.35, r + 0.25, 40, 20);
  const z = b.isoBox(c, r, 0.7, 0.5, 42, { top: '#23283a', l: '#0f1118', r: '#181c28' });
  b.southPanel(c + 0.04, r, 0.62, 4, 40, '#0c0f16', z + 1);
  b.southPanel(c + 0.05, r, 0.6, 4, 5, '#33405a', z + 2);
  for (let i = 0; i < 8; i++) {
    const y = 6 + i * 4.4;
    b.southPanel(c + 0.06, r, 0.58, y, y + 3.4, '#1a1f2e', z + 2);
    b.southPanel(c + 0.06, r, 0.58, y + 3.0, y + 3.4, '#070a10', z + 3);
    for (let v = 0; v < 3; v++) b.southPanel(c + 0.4 + v * 0.05, r, 0.03, y + 0.6, y + 2.8, '#05070c', z + 3);
    b.led(c + 0.12, r, 11 + i * 4.4 * 0.95, i % 3 === 0 ? '#6cc47a' : i % 3 === 1 ? '#ffd27a' : '#5ce0ff', true, z + 5);
    if (i % 2 === 0) b.led(c + 0.17, r, 11 + i * 4.4 * 0.95, '#e2604f', true, z + 5);
  }
  b.led(c + 0.6, r, 40, '#6cc47a', true, z + 6);
}

/** 白板:框 + 板面 + 涂鸦笔迹 + 便签。 */
export function whiteboard(b: SceneBuilder, c: number, r: number, wc: number): void {
  const z = zidx(c, r) - 2;
  b.southPanel(c - 0.02, r, wc + 0.04, 16, 49, '#8a7a5a', z);
  b.southPanel(c, r, wc, 18, 46, '#eee8d6', z + 1);
  b.southPanel(c, r, wc, 18, 20, 'rgba(255,255,255,.5)', z + 2);
  b.southPanel(c, r, wc, 46, 49, '#9a8a6a', z + 2);
  const strokes: Array<[string, number, number, number]> = [
    ['#5a86c0', 0.1, 0.55, 26],
    ['#c45446', 0.2, 0.34, 33],
    ['#6caf70', 0.14, 0.46, 40],
    ['#3a4660', 0.3, 0.2, 47],
  ];
  strokes.forEach(([col, o, w2, y]) => b.southPanel(c + o, r, wc * w2, y, y + 2, col, z + 3));
  b.southPanel(c + 0.12, r, 0.04, 28, 38, '#3a4660', z + 3);
  b.southPanel(c + 0.12, r, 0.18, 28, 30, '#3a4660', z + 3);
  b.southPanel(c + wc * 0.62, r, 0.16, 30, 38, '#ffd27a', z + 4);
  b.southPanel(c + wc * 0.62, r, 0.16, 30, 31, '#e0b85a', z + 5);
  b.southPanel(c + wc * 0.8, r, 0.16, 24, 32, '#7cc4d0', z + 4);
  b.southPanel(c + wc * 0.8, r, 0.16, 24, 25, '#5aa6b4', z + 5);
}

/** 桌面台灯。 */
export function deskLamp(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.06, r + 0.06, 8, 4);
  b.isoBox(c, r, 0.08, 0.08, 4, { top: '#3a3f50', l: '#1c2030', r: '#2a3040' });
  const a = b.pt(c + 0.04, r + 0.04, 4);
  const d = b.pt(c + 0.04, r + 0.04, 16);
  b.poly([{ x: a.x - 0.8, y: a.y }, { x: a.x + 0.8, y: a.y }, { x: d.x + 0.8, y: d.y }, { x: d.x - 0.8, y: d.y }], '#2a3040', zidx(c, r) + 5);
  b.chip(c - 0.02, r, 0.12, 0.06, 15, 18, { top: '#e6bd6c', l: '#9a7c34', r: '#c9a043' });
  b.led(c + 0.04, r + 0.03, 15, '#ffe6ad', false, zidx(c, r) + 8);
}

/** 小绿植。 */
export function plant(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.16, r + 0.16, 18, 9);
  b.isoBox(c, r, 0.32, 0.32, 8, { top: '#9a6a48', l: '#4a3422', r: '#7a4e30' });
  b.southPanel(c, r, 0.32, 6, 8, 'rgba(210,150,110,.5)', zidx(c + 0.32, r) + 4);
  b.isoBox(c - 0.02, r - 0.02, 0.36, 0.36, 20, { top: '#4f8a5a', l: '#2f5a3a', r: '#418a4e' });
  b.isoBox(c + 0.02, r + 0.02, 0.28, 0.28, 27, { top: '#5b9a66', l: '#356a44', r: '#4f8a5a' });
  b.isoBox(c + 0.07, r + 0.07, 0.18, 0.18, 32, { top: '#6aab74', l: '#3c7a4e', r: '#58a064' });
  ['#8ec98e', '#7cbf86', '#9ad19a'].forEach((cl, i) => b.speck(c + 0.1 + i * 0.05, r + 0.06 + i * 0.04, 30 + i * 2, cl, zidx(c, r) + 12));
}

/** 架上盆栽。 */
export function pottedShelf(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.1, r + 0.1, 14, 7);
  b.isoBox(c, r, 0.18, 0.18, 16, { top: '#6a4e34', l: '#3a2820', r: '#523a28' });
  b.southPanel(c, r, 0.18, 14, 16, '#7a5a3c', zidx(c + 0.18, r) + 3);
  b.chip(c + 0.02, r + 0.02, 0.12, 0.12, 16, 20, { top: '#9a6a48', l: '#4a3422', r: '#7a4e30' });
  b.chip(c, r, 0.16, 0.16, 20, 30, { top: '#5b9a66', l: '#356a44', r: '#4f8a5a' });
  ['#8ec98e', '#7cbf86'].forEach((cl, i) => b.speck(c + 0.06 + i * 0.04, r + 0.05, 26 + i * 2, cl, zidx(c, r) + 10));
}

/** 落地窗:玻璃 + 月亮 + 竖框 + 体积光束。 */
export function windowWall(b: SceneBuilder, cA: number, cB: number, r: number): void {
  const glass = [b.pt(cA, r, 16), b.pt(cB, r, 16), b.pt(cB, r, 38), b.pt(cA, r, 38)];
  b.poly(glass, 'rgba(143,208,255,.20)', zidx(cA, r) - 2);
  const m = b.pt((cA + cB) / 2, r, 27);
  b.pxrect(m.x - 4, m.y - 5, 9, 9, '#ffe6ad', zidx(cA, r) - 1, undefined, {
    borderRadius: '50%',
    opacity: 0.85,
    boxShadow: '0 0 10px 3px rgba(255,230,173,.6)',
  });
  const v1 = b.pt((cA + cB) / 2, r, 16);
  const v2 = b.pt((cA + cB) / 2, r, 38);
  b.poly([{ x: v1.x - 1, y: v1.y }, { x: v1.x + 1, y: v1.y }, { x: v2.x + 1, y: v2.y }, { x: v2.x - 1, y: v2.y }], 'rgba(20,20,30,.6)', zidx(cA, r) - 1);
  const top = b.pt((cA + cB) / 2, r, 38);
  const w = ((cB - cA) * TW) / 2;
  b.godray(top.x - w * 0.6, top.y, Math.max(70, w * 1.2), 140, zidx(cA, r) + 4, false);
  b.godray(top.x - w * 0.6 + 12, top.y, Math.max(70, w * 1.2), 140, zidx(cA, r) + 4, true);
}

/** 墙面海报。 */
export function wallPoster(b: SceneBuilder, c: number, r: number, wc: number, scheme: [string, string, string]): void {
  const z = zidx(c, r) - 2;
  b.southPanel(c - 0.02, r, wc + 0.04, 24, 47, '#1a1f30', z);
  b.southPanel(c, r, wc, 26, 45, scheme[0], z + 1);
  b.southPanel(c + wc * 0.12, r, wc * 0.5, 30, 42, scheme[1], z + 2);
  b.southPanel(c + wc * 0.18, r, wc * 0.3, 33, 39, scheme[2], z + 3);
  b.southPanel(c, r, wc, 44, 45, 'rgba(255,255,255,.25)', z + 3);
}

/** 三层花纹地毯(外/内边 + 中心菱形)。 */
export function patternRug(b: SceneBuilder, c0: number, r0: number, wc: number, dc: number, base: string, trim: string): void {
  const z = zidx(c0 + wc, r0 + dc) - 1;
  b.rug(c0, r0, wc, dc, base, z);
  b.rug(c0 + 0.12, r0 + 0.12, wc - 0.24, dc - 0.24, trim, z);
  b.rug(c0 + 0.28, r0 + 0.28, wc - 0.56, dc - 0.56, base, z);
  const m = b.pt(c0 + wc / 2, r0 + dc / 2);
  b.poly([{ x: m.x, y: m.y - 6 }, { x: m.x + 10, y: m.y }, { x: m.x, y: m.y + 6 }, { x: m.x - 10, y: m.y }], trim, z);
}

/** 沙发:底座 + 靠背 + 两侧扶手 + 坐垫。 */
export function sofa(b: SceneBuilder, c: number, r: number, wc: number): void {
  b.contactShadow(c + wc / 2, r + 0.42, wc * 60, 26);
  b.isoBox(c, r, wc, 0.7, 8, { top: '#5a4a6e', l: '#322444', r: '#46395c' });
  b.isoBox(c, r - 0.05, wc, 0.18, 17, { top: '#6a5a80', l: '#3a2c4e', r: '#4e3f66' });
  b.southPanel(c, r - 0.05, wc, 15, 17, 'rgba(170,150,200,.5)', zidx(c + wc, r) + 5);
  b.isoBox(c, r, 0.16, 0.7, 13, { top: '#6a5a80', l: '#352848', r: '#4a3c60' });
  b.isoBox(c + wc - 0.16, r, 0.16, 0.7, 13, { top: '#6a5a80', l: '#352848', r: '#4a3c60' });
  const n = Math.max(2, Math.round(wc / 0.8));
  const cw = (wc - 0.32) / n;
  for (let i = 0; i < n; i++) {
    const cc = c + 0.16 + i * cw;
    b.chip(cc + 0.02, r + 0.06, cw - 0.06, 0.56, 8, 12, { top: '#6f5e88', l: '#3c2e52', r: '#54456c' });
    b.southPanel(cc + 0.02, r + 0.06, cw - 0.06, 10, 12, 'rgba(180,160,210,.4)', zidx(c + wc, r) + 6);
  }
}

/** 咖啡吧台:台体 + 咖啡机 + 杯具 + 升腾热气。 */
export function coffeeBar(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.5, r + 0.22, 56, 20);
  const z = b.isoBox(c, r, 1.0, 0.4, 16, { top: '#403a50', l: '#1f1c2c', r: '#2c2840' });
  b.southPanel(c, r, 1.0, 13, 16, '#544c66', z + 1);
  b.isoBox(c + 0.08, r, 0.26, 0.26, 20, { top: '#2a2f3e', l: '#15131a', r: '#20242f' });
  b.southPanel(c + 0.08, r, 0.26, 28, 30, 'rgba(150,170,210,.4)', z + 5);
  b.led(c + 0.3, r + 0.06, 22, '#ffd27a', true, z + 6);
  b.chip(c + 0.14, r + 0.2, 0.08, 0.06, 16, 18, { top: '#3a3f50', l: '#1c2030', r: '#2a3040' });
  b.chip(c + 0.5, r + 0.1, 0.1, 0.1, 16, 20, { top: '#d9d3c4', l: '#8a8276', r: '#aaa294' });
  b.led(c + 0.55, r + 0.15, 20, '#3a2c20', false, z + 7);
  b.chip(c + 0.66, r + 0.16, 0.09, 0.09, 16, 19, { top: '#c9a043', l: '#7a6028', r: '#9a7c34' });
  b.steam(c + 0.2, r, z + 8);
}

/** 书柜:柜体 + 三层书架 + 高矮错落的书脊。 */
export function bookcase(b: SceneBuilder, c: number, r: number, wc: number): void {
  const z = b.isoBox(c, r, wc, 0.3, 46, { top: '#6a4e34', l: '#3a2820', r: '#523a28' });
  b.southPanel(c, r, wc, 0, 46, '#2e2018', z + 1);
  b.southPanel(c + 0.02, r, wc - 0.04, 44, 46, '#7a5a3c', z + 2);
  const books = ['#c45446', '#6f97d6', '#6caf70', '#e6bd6c', '#9c6c80', '#4cc0d8', '#b8855e', '#7e6aa8'];
  for (let s = 0; s < 3; s++) {
    b.southPanel(c + 0.03, r, wc - 0.06, 6 + s * 13, 7 + s * 13, '#1c130d', z + 2);
    let i = 0;
    let cc = c + 0.06;
    const end = c + wc - 0.06;
    while (cc < end - 0.01) {
      const bw = ((wc - 0.12) / 5) * (0.7 + ((i * 7 + s * 3) % 5) * 0.07);
      const h = 8 + ((i * 5 + s * 2) % 4) * 3.5;
      const lean = (i + s) % 6 === 5 ? 2 : 0;
      b.southPanel(cc, r, Math.min(bw, end - cc) * 0.82, 9 + s * 13, 9 + s * 13 + 11 + h * 0.6 + lean, books[(i + s) % books.length], z + 3);
      b.southPanel(cc, r, Math.min(bw, end - cc) * 0.18, 9 + s * 13, 9 + s * 13 + 11 + h * 0.6 + lean, 'rgba(0,0,0,.22)', z + 4);
      cc += bw + 0.005;
      i++;
    }
  }
}

/** 大型绿植(四层叶冠)。 */
export function bigPlant(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.2, r + 0.2, 24, 12);
  b.isoBox(c, r, 0.4, 0.4, 10, { top: '#9a6a48', l: '#4a3422', r: '#7a4e30' });
  b.southPanel(c, r, 0.4, 7, 10, 'rgba(210,150,110,.5)', zidx(c + 0.4, r) + 4);
  b.isoBox(c - 0.05, r - 0.05, 0.5, 0.5, 30, { top: '#427a4c', l: '#2a5034', r: '#387844' });
  b.isoBox(c - 0.02, r - 0.02, 0.42, 0.42, 40, { top: '#4f8a5a', l: '#2f5a3a', r: '#418a4e' });
  b.isoBox(c + 0.02, r + 0.02, 0.34, 0.34, 48, { top: '#5b9a66', l: '#356a44', r: '#4f8a5a' });
  b.isoBox(c + 0.07, r + 0.07, 0.2, 0.2, 54, { top: '#6aab74', l: '#3c7a4e', r: '#58a064' });
  ['#8ec98e', '#7cbf86', '#9ad19a', '#86c886'].forEach((cl, i) => b.speck(c + 0.08 + i * 0.06, r + 0.05 + i * 0.05, 42 + i * 4, cl, zidx(c, r) + 14));
}

/** 挂墙时钟(表盘 + 转动的时针/分针)。 */
export function wallClock(b: SceneBuilder, c: number, r: number, h: number): void {
  const p = b.pt(c, r, h);
  const z = zidx(c, r) + 6;
  b.pxrect(p.x - 7, p.y - 7, 14, 14, '#e7e1cf', z, undefined, { borderRadius: '50%', boxShadow: '0 0 0 2px #3a3040' });
  const hand = (w: number, len: number, col: string, dur: number) =>
    b.pxrect(p.x - w / 2, p.y - len, w, len, col, z + 1, 'pclock-h', { animation: `spin ${dur}s linear infinite` });
  hand(2, 6, '#2a3040', 60);
  hand(1.5, 4, '#5a86c0', 720);
}

/** 饮水机(机身 + 水桶 + 出水面板 + 双指示灯)。 */
export function waterCooler(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.16, r + 0.16, 18, 9);
  b.isoBox(c, r, 0.3, 0.3, 22, { top: '#cdd4de', l: '#7a828e', r: '#9aa2ae' });
  b.isoBox(c + 0.03, r + 0.03, 0.24, 0.24, 40, { top: 'rgba(150,200,230,.55)', l: 'rgba(90,140,180,.55)', r: 'rgba(120,170,210,.55)' });
  b.southPanel(c + 0.06, r, 0.18, 8, 12, '#3a4660', zidx(c + 0.3, r) + 5);
  b.led(c + 0.1, r, 11, '#5ce0ff', true, zidx(c + 0.3, r) + 6);
  b.led(c + 0.2, r, 11, '#e2604f', false, zidx(c + 0.3, r) + 6);
}

/** 顺墙垂下的线缆束。 */
export function cableRun(b: SceneBuilder, c: number, r: number, h: number): void {
  const z = zidx(c, r) - 1;
  const top = b.pt(c, r, h);
  const mid = b.pt(c + 0.04, r, h * 0.6);
  const btm = b.pt(c + 0.02, r, h * 0.25);
  b.poly([{ x: top.x - 1, y: top.y }, { x: top.x + 1, y: top.y }, { x: mid.x + 1, y: mid.y }, { x: mid.x - 1, y: mid.y }], '#14181f', z);
  b.poly([{ x: mid.x - 1, y: mid.y }, { x: mid.x + 1, y: mid.y }, { x: btm.x + 1, y: btm.y }, { x: btm.x - 1, y: btm.y }], '#181c24', z);
  b.led(c + 0.02, r, h * 0.25, '#6cc47a', true, z + 2);
}

/** 吊顶风管。 */
export function ceilDuct(b: SceneBuilder, cA: number, cB: number, r: number, h: number, col?: string): void {
  const z = zidx(cA, r) - 2;
  const c0 = col || '#2c3346';
  const a = b.pt(cA, r, h);
  const bb = b.pt(cB, r, h);
  const d = b.pt(cB, r, h - 5);
  const e = b.pt(cA, r, h - 5);
  b.poly([a, bb, d, e], c0, z);
  b.poly([a, bb, { x: bb.x, y: bb.y - 1.5 }, { x: a.x, y: a.y - 1.5 }], 'rgba(150,170,210,.35)', z + 1);
  const n = Math.max(2, Math.round(Math.abs(cB - cA)));
  for (let i = 0; i <= n; i++) {
    const cc = cA + ((cB - cA) * i) / n;
    const t = b.pt(cc, r, h);
    const btm = b.pt(cc, r, h + 8);
    b.poly([{ x: t.x - 0.7, y: t.y }, { x: t.x + 0.7, y: t.y }, { x: btm.x + 0.7, y: btm.y }, { x: btm.x - 0.7, y: btm.y }], '#1a2030', z);
  }
}

/** 质检台:浅色工作台 + 立式审查屏(扫描线)+ 摄像头 + 手持设备。 */
export function qaBench(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c - 0.02, r + 0.2, 48, 22);
  const z = b.isoBox(c - 0.45, r - 0.1, 0.95, 0.6, 12, { top: '#aeb6c4', l: '#5a6272', r: '#828c9c' });
  b.southPanel(c - 0.45, r - 0.1, 0.95, 10, 12, '#cdd4de', z + 1);
  b.southPanel(c - 0.5, r, 0.9, 16, 44, '#0e1118', z + 2);
  b.southPanel(c - 0.46, r, 0.82, 18, 42, '#24405a', z + 3);
  const sp = b.pt(c - 0.46, r, 42);
  b.pxrect(sp.x, sp.y, 42, 14, '', z + 4, 'pscan');
  b.southPanel(c - 0.46, r, 0.82, 40, 42, 'rgba(140,180,220,.35)', z + 4);
  b.isoBox(c + 0.32, r - 0.05, 0.18, 0.18, 24, { top: '#9aa2b0', l: '#4a5260', r: '#727c8c' });
  b.led(c + 0.41, r + 0.04, 24, '#5ce0ff', true, z + 6);
  b.chip(c + 0.14, r + 0.22, 0.22, 0.14, 12, 13.5, { top: '#c4ccd6', l: '#7a828e', r: '#9aa2ae' });
}

/** 看板:窄立柱 + 板面 + 三列卡片(带高光)。 */
export function board(b: SceneBuilder, c: number, r: number): void {
  const z = zidx(c, r) + 2;
  b.isoBox(c, r, 0.12, 0.5, 40, { top: '#3a4060', l: '#191d30', r: '#2a3048' });
  b.southPanel(c - 0.12, r, 0.74, 14, 42, '#161f33', z + 3);
  b.southPanel(c - 0.12, r, 0.74, 40, 42, '#2a3454', z + 4);
  const cols: Array<[string, string]> = [
    ['#5a86c0', '#6caf70'],
    ['#c45446', '#7aa7e6'],
    ['#ffd27a', '#6caf70'],
  ];
  for (let col = 0; col < 3; col++) {
    const x = c - 0.08 + col * 0.22;
    b.southPanel(x, r, 0.02, 18, 40, '#0e1424', z + 4);
    for (let k = 0; k < 3; k++) {
      const y = 20 + k * 7 + ((col + k) % 2) * 1;
      const cardCol = cols[col][k % 2];
      b.southPanel(x + 0.02, r, 0.16, y, y + 5, cardCol, z + 5);
      b.southPanel(x + 0.02, r, 0.16, y, y + 1, 'rgba(255,255,255,.3)', z + 6);
    }
  }
}

/** 经理的记忆书(桌 + 两本立书 + 翻开的金页)。 */
export function memoryBook(b: SceneBuilder, c: number, r: number): void {
  b.isoBox(c - 0.2, r - 0.1, 0.5, 0.45, 10, WOOD2);
  b.southPanel(c - 0.2, r - 0.1, 0.5, 8, 10, '#b98a5e', zidx(c + 0.3, r) + 3);
  const z = zidx(c, r) + 4;
  b.chip(c - 0.2, r - 0.06, 0.18, 0.34, 10, 12, { top: '#efe6cf', l: '#b8ad92', r: '#d4cbae' });
  b.chip(c - 0.01, r - 0.06, 0.18, 0.34, 10, 12, { top: '#efe6cf', l: '#b8ad92', r: '#d4cbae' });
  b.southPanel(c - 0.16, r, 0.34, 12, 22, '#ffd27a', z);
  b.southPanel(c - 0.16, r, 0.34, 12, 13.5, '#5a4636', z + 1);
  b.led(c - 0.01, r - 0.02, 12, '#c45446', false, z + 3);
}

/** 预算保险柜(箱体 + 面板 + 角钉灯 + 把手 + 转盘)。 */
export function safeBox(b: SceneBuilder, c: number, r: number): void {
  b.contactShadow(c + 0.3, r + 0.25, 32, 17);
  const z = b.isoBox(c, r, 0.6, 0.5, 34, { top: '#2f3550', l: '#191d30', r: '#252a42' });
  b.southPanel(c + 0.06, r, 0.48, 5, 32, '#2a3048', z + 1);
  b.southPanel(c + 0.1, r, 0.4, 8, 28, '#3a4060', z + 2);
  b.southPanel(c + 0.1, r, 0.4, 26, 28, 'rgba(120,140,190,.35)', z + 3);
  const studs: Array<[number, number]> = [[0.13, 10], [0.45, 10], [0.13, 26], [0.45, 26]];
  studs.forEach(([cc, h]) => b.led(c + cc, r, h, '#5a6488', false, z + 4));
  b.southPanel(c + 0.5, r, 0.04, 11, 15, '#5a6488', z + 4);
  b.southPanel(c + 0.5, r, 0.04, 22, 26, '#5a6488', z + 4);
  const dial = b.pt(c + 0.3, r, 18);
  b.pxrect(dial.x - 4, dial.y - 4, 9, 9, '#ffd27a', z + 5, undefined, { borderRadius: '50%', boxShadow: '0 0 0 1px #b8860f inset' });
  b.southPanel(c + 0.38, r, 0.06, 16, 20, '#c9a043', z + 5);
}

/** 发货口:门框 ×2 + 卷帘门 + 警示条 + 地面胶带 + 出货指示灯。 */
export function shipBay(b: SceneBuilder, c: number, r: number): void {
  b.isoBox(c, r - 0.1, 0.2, 1.2, 46, { top: '#3a4060', l: '#1a1f30', r: '#2a3048' });
  b.isoBox(c + 1.0, r - 0.1, 0.2, 1.2, 46, { top: '#3a4060', l: '#1a1f30', r: '#2a3048' });
  const door = [b.pt(c + 0.2, r, 0), b.pt(c + 1.0, r, 0), b.pt(c + 1.0, r, 40), b.pt(c + 0.2, r, 40)];
  b.poly(door, 'repeating-linear-gradient(180deg,#323a54 0 3px,#222a40 3px 6px)', zidx(c + 1, r + 1) + 4);
  const warn = [b.pt(c + 0.2, r, 6), b.pt(c + 1.0, r, 6), b.pt(c + 1.0, r, 9), b.pt(c + 0.2, r, 9)];
  b.poly(warn, 'repeating-linear-gradient(45deg,#ffd27a 0 5px,#3a3020 5px 10px)', zidx(c + 1, r + 1) + 5);
  b.rug(c + 0.25, r + 0.05, 0.7, 0.06, 'rgba(255,210,122,.35)', zidx(c + 1, r) + 1);
  b.led(c + 0.6, r, 42, '#6cc47a', true, zidx(c + 1, r + 1) + 6);
}

/** 货箱堆(三只木箱 + 板条 + 贴纸)。 */
export function crateStack(b: SceneBuilder, c: number, r: number): void {
  const slat = (cc: number, rr: number, h0: number) => {
    b.southPanel(cc, rr, 0.4, h0 + 2, h0 + 3, '#3a2818', zidx(cc + 0.4, rr) + 5);
    b.southPanel(cc, rr, 0.4, h0 + 9, h0 + 10, '#3a2818', zidx(cc + 0.4, rr) + 5);
  };
  b.contactShadow(c + 0.45, r + 0.25, 40, 20);
  b.isoBox(c, r, 0.4, 0.4, 14, { top: '#9a7350', l: '#4a3628', r: '#7a5a3c' });
  slat(c, r, 1);
  b.southPanel(c + 0.08, r, 0.22, 5, 11, '#d9d3c4', zidx(c + 0.4, r) + 6);
  b.isoBox(c + 0.5, r + 0.1, 0.4, 0.4, 14, { top: '#8e6a48', l: '#42301f', r: '#6e5232' });
  slat(c + 0.5, r + 0.1, 1);
  b.isoBox(c + 0.2, r - 0.1, 0.4, 0.4, 30, { top: '#a87f57', l: '#4a3628', r: '#7a5a3c' });
  b.southPanel(c + 0.28, r - 0.1, 0.06, 18, 30, '#c9bfa6', zidx(c + 0.6, r) + 7);
}
