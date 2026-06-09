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

const polygon = (pts: Point[]): string =>
  `polygon(${pts.map(p => `${p.x.toFixed(1)}px ${p.y.toFixed(1)}px`).join(',')})`;

/**
 * 等距场景构造器。持有 layout 与累积的节点数组，提供与原型同名的绘制原语
 * (tile/poly/wallEdge/label/buildmark)，自动按序生成稳定 key。
 */
export class SceneBuilder {
  readonly nodes: SceneNode[] = [];

  constructor(private readonly layout: Layout) {}

  private add(node: Omit<SceneNode, 'key'>): void {
    this.nodes.push({ ...node, key: `n${this.nodes.length}` });
  }

  private at(c: number, r: number, z = 0): Point {
    return iso(this.layout, c, r, z);
  }

  /** 一块菱形地板瓦片。 */
  tile(c: number, r: number, color: string, z: number): void {
    const p = this.at(c, r);
    this.add({ className: 'tile', style: { left: p.x, top: p.y, zIndex: z, background: color } });
  }

  /** 任意多边形面(墙、家具侧面等)，用 clip-path 切出形状，铺满 world 后裁剪。 */
  poly(pts: Point[], color: string, z: number, extra?: CSSProperties): void {
    this.add({ className: 'poly', style: { zIndex: z, background: color, clipPath: polygon(pts), ...extra } });
  }

  /** 一段竖直墙面(从地面拔高 h)。 */
  wallEdge(c1: number, r1: number, c2: number, r2: number, h: number, color: string, z: number, filter?: string): void {
    const a = this.at(c1, r1);
    const b = this.at(c2, r2);
    this.poly(
      [
        { x: a.x, y: a.y },
        { x: b.x, y: b.y },
        { x: b.x, y: b.y - h },
        { x: a.x, y: a.y - h },
      ],
      color,
      z,
      filter ? { filter } : undefined,
    );
  }

  /** 房间标签(漂在房间中心上方)。 */
  label(c: number, r: number, text: string): void {
    const p = this.at(c, r);
    this.add({ className: 'roomlabel', style: { left: p.x, top: p.y - 6, zIndex: 8000 }, text });
  }

  /** 可扩展空位的斜线占位标记。 */
  buildmark(c: number, r: number, mark: boolean): void {
    const p = this.at(c, r);
    this.add({ className: 'buildmark', style: { left: p.x, top: p.y, zIndex: zidx(c, r) }, mark });
  }
}
