/**
 * 等距(isometric)投影的纯数学层。忠实移植 redesign-iso-directions.html 的 `iso`/`zidx`/
 * `layout`。这里不碰 DOM，只把"网格坐标(列 c、行 r、高度 z)"换算成"屏幕像素 + 层序"。
 */

/** 瓦片宽(菱形横对角线) */
export const TW = 64;
/** 瓦片高(菱形纵对角线) */
export const TH = 32;
/** 墙高(像素) */
export const WALLH = 42;

/** 办公室网格列数(0..C) */
export const C = 13;
/** 办公室网格行数(0..R) */
export const R = 9;

export interface Point {
  x: number;
  y: number;
}

export interface Layout {
  /** 把世界坐标系平移到正值区间的 X 偏移 */
  ox: number;
  /** 同上的 Y 偏移 */
  oy: number;
  /** world 容器像素宽 */
  worldW: number;
  /** world 容器像素高 */
  worldH: number;
}

/** 网格 (c,r,z) → 屏幕像素。z 为离地高度，向上为负 y。 */
export function iso(layout: Pick<Layout, 'ox' | 'oy'>, c: number, r: number, z = 0): Point {
  return {
    x: ((c - r) * TW) / 2 + layout.ox,
    y: ((c + r) * TH) / 2 + layout.oy - z,
  };
}

/** 画家算法层序：越靠前(c+r 越大)越压在上面，z 微调同格高低。 */
export function zidx(c: number, r: number, z = 0): number {
  return Math.round((c + r) * 10 + z * 0.5) + 50;
}

/**
 * 计算把整张办公室(含墙、含上方留白)收进正坐标所需的偏移与 world 尺寸。
 * 复刻原型 layout()：取四角投影包围盒，留出墙高 + 顶部光晕余量与四周边距。
 */
export function computeLayout(): Layout {
  const zero = { ox: 0, oy: 0 };
  const corners: Array<[number, number]> = [
    [0, 0],
    [C + 1, 0],
    [0, R + 1],
    [C + 1, R + 1],
  ];
  const xs = corners.map(([c, r]) => iso(zero, c, r).x);
  const ys = corners.map(([c, r]) => iso(zero, c, r).y);
  const minx = Math.min(...xs);
  const maxx = Math.max(...xs);
  const miny = Math.min(...ys) - WALLH - 50;
  const maxy = Math.max(...ys);
  const mX = 60;
  const mT = 44;
  const mB = 30;
  return {
    ox: mX - minx,
    oy: mT - miny,
    worldW: maxx - minx + 2 * mX,
    worldH: maxy - miny + mT + mB,
  };
}
