import type { CSSProperties } from 'react';

import { iso, zidx, type Layout, type Point } from '@/office/iso';

/**
 * 一个可渲染的场景节点 —— 最终就是 world 容器里一个绝对定位的 `<div>`。
 * 把"场景描述(数据)"与"渲染(把 div 摆出来)"解耦：原型里是命令式 appendChild，
 * 这里收敛成一棵 SceneNode[]，由 Office 组件无脑 map 成 div。
 */
export interface SceneNode {
  key: string;
  className: string;
  style: CSSProperties;
  /** 房间标签文字 */
  text?: string;
  /** 可扩展位的"＋"标记 */
  mark?: boolean;
}

/** 一个三面体素的三阶配色(顶面 / 左侧 / 右侧)。 */
export interface Faces {
  top: string;
  l: string;
  r: string;
}

/** 木纹/深色等常用体素配色，移植自原型常量。 */
export const WOOD: Faces = { top: '#9a7350', l: '#5a4636', r: '#7a5a3c' };
export const WOOD2: Faces = { top: '#a87f57', l: '#5a4636', r: '#7a5a3c' };
export const DARK: Faces = { top: '#2a2f3e', l: '#15131a', r: '#20242f' };

const polygon = (pts: Point[]): string =>
  `polygon(${pts.map(p => `${p.x.toFixed(1)}px ${p.y.toFixed(1)}px`).join(',')})`;

/**
 * 等距场景构造器。持有 layout 与累积的节点数组，提供与原型同名的绘制原语：
 * 体素(isoBox/chip)、竖面(southPanel)、平面(rug/tile)、像素点(led/speck)、
 * 接触影、原始矩形(pxrect)、顶光(ceilLamp)、窗光(godray)。自动按序生成稳定 key。
 */
export class SceneBuilder {
  readonly nodes: SceneNode[] = [];

  constructor(private readonly layout: Layout) {}

  private add(node: Omit<SceneNode, 'key'>): SceneNode {
    const full: SceneNode = { ...node, key: `n${this.nodes.length}` };
    this.nodes.push(full);
    return full;
  }

  /** 网格 (c,r,z) → 屏幕像素。家具拼装时直接取点用。 */
  pt(c: number, r: number, z = 0): Point {
    return iso(this.layout, c, r, z);
  }

  /** 任意多边形面，用 clip-path 切形，铺满 world 后裁剪。 */
  poly(pts: Point[], color: string, z: number, extra?: CSSProperties): SceneNode {
    return this.add({ className: 'poly', style: { zIndex: z, background: color, clipPath: polygon(pts), ...extra } });
  }

  /** 一块菱形地板瓦片。 */
  tile(c: number, r: number, color: string, z: number): void {
    const p = this.pt(c, r);
    this.add({ className: 'tile', style: { left: p.x, top: p.y, zIndex: z, background: color } });
  }

  /** 一段竖直墙面(从地面拔高 h)。 */
  wallEdge(c1: number, r1: number, c2: number, r2: number, h: number, color: string, z: number, filter?: string): void {
    const a = this.pt(c1, r1);
    const b = this.pt(c2, r2);
    this.poly(
      [a, b, { x: b.x, y: b.y - h }, { x: a.x, y: a.y - h }],
      color,
      z,
      filter ? { filter } : undefined,
    );
  }

  /** 房间标签(漂在房间中心上方)。 */
  label(c: number, r: number, text: string): void {
    const p = this.pt(c, r);
    this.add({ className: 'roomlabel', style: { left: p.x, top: p.y - 6, zIndex: 8000 }, text });
  }

  /** 可扩展空位的斜线占位标记。 */
  buildmark(c: number, r: number, mark: boolean): void {
    const p = this.pt(c, r);
    this.add({ className: 'buildmark', style: { left: p.x, top: p.y, zIndex: zidx(c, r) }, mark });
  }

  /** 落地三面体素(桌、椅、机架、绿植盆…)。返回其层序 z，供叠在上面的细节用。 */
  isoBox(c: number, r: number, wc: number, dc: number, h: number, col: Faces): number {
    const z = zidx(c + wc, r + dc) + 2;
    this.poly([this.pt(c + wc, r), this.pt(c + wc, r + dc), this.pt(c + wc, r + dc, h), this.pt(c + wc, r, h)], col.r, z);
    this.poly([this.pt(c, r + dc), this.pt(c + wc, r + dc), this.pt(c + wc, r + dc, h), this.pt(c, r + dc, h)], col.l, z);
    this.poly([this.pt(c, r, h), this.pt(c + wc, r, h), this.pt(c + wc, r + dc, h), this.pt(c, r + dc, h)], col.top, z + 1);
    return z;
  }

  /** 浮空三面小盒(放在桌面/架上的道具，不画落地长边)。 */
  chip(c: number, r: number, wc: number, dc: number, z0: number, z1: number, col: Faces, zb?: number): number {
    const z = zb != null ? zb : zidx(c + wc, r + dc) + 3;
    this.poly([this.pt(c + wc, r, z0), this.pt(c + wc, r + dc, z0), this.pt(c + wc, r + dc, z1), this.pt(c + wc, r, z1)], col.r, z);
    this.poly([this.pt(c, r + dc, z0), this.pt(c + wc, r + dc, z0), this.pt(c + wc, r + dc, z1), this.pt(c, r + dc, z1)], col.l, z);
    this.poly([this.pt(c, r, z1), this.pt(c + wc, r, z1), this.pt(c + wc, r + dc, z1), this.pt(c, r + dc, z1)], col.top, z + 1);
    return z;
  }

  /** 朝南竖面(家具正面、屏幕、海报、白板等)。返回节点，供后续切片取句柄改色。 */
  southPanel(c: number, r: number, wc: number, h0: number, h1: number, color: string, z: number): SceneNode {
    return this.poly([this.pt(c, r, h0), this.pt(c + wc, r, h0), this.pt(c + wc, r, h1), this.pt(c, r, h1)], color, z);
  }

  /** 贴地平面四边形(地毯)。 */
  rug(c0: number, r0: number, wc: number, dc: number, col: string, z: number): void {
    this.poly([this.pt(c0, r0), this.pt(c0 + wc, r0), this.pt(c0 + wc, r0 + dc), this.pt(c0, r0 + dc)], col, z);
  }

  /** 一颗像素 LED / 高光点(blink=true 闪烁)。 */
  led(c: number, r: number, h: number, color: string, blink = false, z?: number): void {
    const p = this.pt(c, r, h);
    const style: CSSProperties = { left: p.x, top: p.y, background: color, zIndex: z != null ? z : zidx(c, r) + 9 };
    if (blink) style.animationDelay = `${(Math.random() * 1.2).toFixed(2)}s`;
    this.add({ className: `pled${blink ? ' b' : ''}`, style });
  }

  /** 绿植上的高光斑点(同 LED 但不闪、层序更高)。 */
  speck(c: number, r: number, h: number, color: string, z?: number): void {
    const p = this.pt(c, r, h);
    this.add({ className: 'pspeck', style: { left: p.x, top: p.y, background: color, zIndex: z != null ? z : zidx(c, r) + 12 } });
  }

  /** 道具落地接触影。 */
  contactShadow(c: number, r: number, wpx: number, hpx: number, z?: number): void {
    const p = this.pt(c, r, 0);
    this.add({ className: 'pcshadow', style: { left: p.x - wpx / 2, top: p.y - hpx / 2 + 1, width: wpx, height: hpx, zIndex: z != null ? z : zidx(c, r) + 1 } });
  }

  /** 屏幕坐标系里的原始矩形(扫描线叠层、月亮、旋钮…)。 */
  pxrect(x: number, y: number, w: number, h: number, color: string, z: number, cls?: string, extra?: CSSProperties): SceneNode {
    return this.add({ className: `rect${cls ? ' ' + cls : ''}`, style: { left: x, top: y, width: w, height: h, background: color, zIndex: z, ...extra } });
  }

  /** 顶光晕(暖/冷色)。本轮只渲染光斑，工人光池交互留后续切片。 */
  ceilLamp(c: number, r: number, color: string): void {
    const p = this.pt(c, r);
    const z = zidx(c, r) - 3;
    this.add({ className: 'glow', style: { left: p.x - 58, top: p.y - 36, width: 116, height: 70, background: `radial-gradient(closest-side, ${color}, transparent 78%)`, opacity: 0.62, zIndex: z } });
    this.add({ className: 'glow', style: { left: p.x - 22, top: p.y - 16, width: 44, height: 30, background: `radial-gradient(closest-side, ${color}, transparent 70%)`, opacity: 0.5, filter: 'blur(5px)', zIndex: z + 1 } });
  }

  /** 窗外射入的体积光束。 */
  godray(left: number, top: number, width: number, height: number, z: number, secondary: boolean): void {
    this.add({ className: `godray${secondary ? ' b' : ''}`, style: { left, top, width, height, zIndex: z } });
  }
}
