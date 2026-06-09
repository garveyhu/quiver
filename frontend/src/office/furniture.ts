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
