import { iso, zidx, type Layout } from '@/office/iso';

/** 工人角色:普通员工(戴耳机) / 经理(王冠袍子) / 独立审计(护目镜+夹板)。 */
export type WorkerRole = 'emp' | 'mgr' | 'aud';

/** 一个已算好像素位的工人(渲染层直接用)。 */
export interface PlacedWorker {
  id: string;
  role: WorkerRole;
  x: number;
  y: number;
  z: number;
  /** [连帽衫色, 上衣色]，派生出明暗三阶 */
  hood: readonly [string, string];
  /** 等待经理决策(头顶冒思考点) */
  awaiting?: boolean;
  /** 前倾姿态(审计/敲键) */
  lean?: boolean;
  /** 头顶气泡文字 */
  label?: string;
}

/** 连帽衫配色池(移植原型 HOODS)。 */
const HOODS: ReadonlyArray<readonly [string, string]> = [
  ['#5a86c0', '#3f5f8f'],
  ['#a566a8', '#7a3f7a'],
  ['#46a0a0', '#2f7070'],
  ['#6a6ad0', '#43439a'],
  ['#c07888', '#945565'],
  ['#5a8ab0', '#3f6a8a'],
];

/** 按通道加减亮度，派生明暗面色(移植原型 shade)。 */
export function shade(hex: string, d: number): string {
  const n = parseInt(hex.slice(1), 16);
  const cl = (v: number) => Math.max(0, Math.min(255, v));
  const r = cl((n >> 16) + d);
  const g = cl(((n >> 8) & 255) + d);
  const b = cl((n & 255) + d);
  return '#' + ((r << 16) | (g << 8) | b).toString(16).padStart(6, '0');
}

/** 休息室待命格(移植原型 BARR:休息室地板去掉左下角)。 */
function loungeCells(): Array<[number, number]> {
  const cells: Array<[number, number]> = [];
  for (let r = 1; r <= 8; r++) {
    for (let c = 0; c <= 3; c++) {
      if (!(c <= 1 && r >= 7)) cells.push([c, r]);
    }
  }
  return cells;
}

/**
 * 办公室开场人口(移植原型 build() 的初始三类工人):
 * 休息室 5 个待命员工(末一个等待决策)+ 领导区 1 个经理 + 质检台 1 个独立审计。
 * 派活走位 / agent-event 实时动画留给下一刀;本刀只摆静态初始位。
 * 注:5 个员工用错开的格位(原型的散列公式会让两个落到同一格,靠 wander 循环错开,
 * 本刀无 wander 故改用不重叠的格,语义仍是"5 个散在休息室")。
 */
export function initialWorkers(layout: Layout): PlacedWorker[] {
  const cells = loungeCells();
  const workers: PlacedWorker[] = [];

  for (let i = 0; i < 5; i++) {
    const [c, r] = cells[(i * 5 + 2) % cells.length];
    const p = iso(layout, c, r);
    workers.push({ id: `emp${i}`, role: 'emp', x: p.x, y: p.y, z: zidx(c, r) + 5, hood: HOODS[i % HOODS.length], awaiting: i === 4 });
  }

  const mp = iso(layout, 11.5, 3.3);
  workers.push({ id: 'mgr', role: 'mgr', x: mp.x, y: mp.y, z: zidx(11.5, 3.3) + 5, hood: ['#e0a050', '#b07a30'], label: '经理' });

  const ap = iso(layout, 11.3, 0.4);
  workers.push({ id: 'aud', role: 'aud', x: ap.x, y: ap.y, z: zidx(11.3, 0.4) + 5, hood: ['#46a0a0', '#2f7070'], lean: true, label: '审计' });

  return workers;
}
