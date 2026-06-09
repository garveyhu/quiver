/**
 * 办公室房间布局数据。忠实移植 redesign-iso-directions.html 的 `ROOMS`/`HALLS`/`DESKS`。
 * 这些坐标与色值是原型的布局真相，作为"美术/布局数据"原样保留。
 */

export interface Room {
  /** 起始列(含) */
  c0: number;
  /** 结束列(含) */
  c1: number;
  /** 起始行(含) */
  r0: number;
  /** 结束行(含) */
  r1: number;
  /** 房间标签(中文) */
  label: string;
  /** 地板染色 */
  tint: string;
  /** 墙体颜色；null = 无墙(开放/可扩展位) */
  wall: string | null;
  /** 可扩展空位(画斜线占位标记，不铺地板) */
  build?: boolean;
}

export type RoomKey = 'lounge' | 'work' | 'verify' | 'dispatch' | 'build' | 'ops' | 'ship';

export const ROOMS: Record<RoomKey, Room> = {
  lounge: { c0: 0, c1: 3, r0: 0, r1: 9, label: '休息室', tint: 'rgba(120,90,140,.16)', wall: 'rgba(86,62,104,1)' },
  work: { c0: 5, c1: 9, r0: 0, r1: 9, label: '工位区', tint: 'rgba(90,120,170,.13)', wall: 'rgba(64,86,128,1)' },
  verify: { c0: 11, c1: 13, r0: 0, r1: 1, label: '质检台', tint: 'rgba(80,150,130,.16)', wall: 'rgba(54,110,96,1)' },
  dispatch: { c0: 11, c1: 13, r0: 2, r1: 3, label: '领导区 · 经理', tint: 'rgba(200,160,90,.13)', wall: 'rgba(150,116,56,1)' },
  build: { c0: 11, c1: 13, r0: 4, r1: 5, label: '＋ 可扩展', tint: 'transparent', wall: null, build: true },
  ops: { c0: 11, c1: 13, r0: 6, r1: 7, label: '运维 · 预算', tint: 'rgba(160,130,80,.13)', wall: 'rgba(120,96,56,1)' },
  ship: { c0: 11, c1: 13, r0: 8, r1: 9, label: '发货口', tint: 'rgba(110,120,140,.12)', wall: 'rgba(74,84,104,1)' },
};

/** 走廊所在列(地板用更亮的底色提示通道) */
export const HALLS = [4, 10];

/** 工位坐标(12 个并发上限)——本轮先不铺桌子，留给后续家具切片。 */
export const DESKS: Array<[number, number]> = [
  [5, 1], [7, 1], [9, 1],
  [5, 3], [7, 3], [9, 3],
  [5, 5], [7, 5], [9, 5],
  [5, 7], [7, 7], [9, 7],
];

/** 某格属于哪个房间(命中第一个包含它的房间)；都不在则 null(走廊/外围)。 */
export function roomAt(c: number, r: number): RoomKey | null {
  for (const key of Object.keys(ROOMS) as RoomKey[]) {
    const room = ROOMS[key];
    if (c >= room.c0 && c <= room.c1 && r >= room.r0 && r <= room.r1) return key;
  }
  return null;
}
