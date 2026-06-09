import {
  ceilDuct,
  deskLamp,
  deskUnit,
  plant,
  pottedShelf,
  serverRack,
  wallPoster,
  whiteboard,
  windowWall,
} from '@/office/furniture';
import { computeLayout, zidx, type Layout, C, R, WALLH } from '@/office/iso';
import { SceneBuilder, type SceneNode } from '@/office/primitives';
import { DESKS, ROOMS, HALLS, roomAt, type RoomKey } from '@/office/rooms';

export interface Scene {
  layout: Layout;
  nodes: SceneNode[];
}

const HALL_TINT = 'rgba(255,255,255,.045)';
const FLOOR_TINT = 'rgba(255,255,255,.02)';

/**
 * 构建静态等距办公室场景：地板瓦片网格(按房间染色 + 走廊提亮 + 可扩展位斜线) +
 * 房间墙体(北/西两面，西墙更暗模拟受光) + 房间标签。
 * 家具、工人、灯光留给后续切片往同一棵 SceneNode[] 上叠。
 */
export function buildScene(): Scene {
  const layout = computeLayout();
  const b = new SceneBuilder(layout);

  // 地板：逐格按房间 tint 上色；可扩展位画斜线占位标记;走廊/外围用通用底色。
  for (let r = 0; r <= R; r++) {
    for (let c = 0; c <= C; c++) {
      const key = roomAt(c, r);
      if (key === 'build') {
        b.buildmark(c, r, c === 12 && r === 4);
        continue;
      }
      const base = key ? ROOMS[key].tint : HALLS.includes(c) ? HALL_TINT : FLOOR_TINT;
      b.tile(c, r, base, zidx(c, r));
    }
  }

  // 墙体：每个房间画北墙(顶边)与西墙(左边)，西墙压暗模拟侧光。
  for (const key of Object.keys(ROOMS) as RoomKey[]) {
    const room = ROOMS[key];
    if (!room.wall) continue;
    const z = zidx(room.c0, room.r0) - 3;
    b.wallEdge(room.c0, room.r0, room.c1 + 1, room.r0, WALLH, room.wall, z, 'brightness(.82)');
    b.wallEdge(room.c0, room.r0, room.c0, room.r1 + 1, WALLH, room.wall, z, 'brightness(.6)');
  }

  // 房间标签
  for (const key of Object.keys(ROOMS) as RoomKey[]) {
    const room = ROOMS[key];
    b.label((room.c0 + room.c1) / 2, (room.r0 + room.r1) / 2, room.label);
  }

  furnishWorkArea(b);
  ceilingLights(b);

  return { layout, nodes: b.nodes };
}

/** 工位区:12 套工位 + 服务器架 ×2 + 白板 + 绿植 + 落地窗 + 海报 + 风管 + 台灯 + 盆栽。 */
function furnishWorkArea(b: SceneBuilder): void {
  DESKS.forEach(([c, r]) => deskUnit(b, c, r));
  serverRack(b, 5.3, 8.6);
  serverRack(b, 6.6, 8.6);
  whiteboard(b, 7.6, 0, 1.6);
  plant(b, 8.9, 8.6);
  windowWall(b, 5.6, 7.2, 0);
  wallPoster(b, 8.5, 0.0, 1.1, ['#243042', '#46a0a0', '#5ce0ff']);
  ceilDuct(b, 5.4, 7.2, 0.0, 50, '#283044');
  ceilDuct(b, 5.4, 7.2, 4.0, 50, '#283044');
  deskLamp(b, 5.4, 0.65);
  deskLamp(b, 7.4, 0.65);
  deskLamp(b, 9.4, 0.65);
  pottedShelf(b, 8.95, 0.2);
}

/** 顶光:暖区(休息室/运维)与冷区(工位/质检)分色。 */
function ceilingLights(b: SceneBuilder): void {
  b.ceilLamp(1.8, 4, 'rgba(255,200,150,.5)');
  b.ceilLamp(7, 3, 'rgba(150,190,255,.46)');
  b.ceilLamp(7, 7, 'rgba(150,190,255,.42)');
  b.ceilLamp(12, 1, 'rgba(150,230,200,.46)');
  b.ceilLamp(12, 3, 'rgba(255,210,140,.4)');
  b.ceilLamp(12, 7, 'rgba(255,210,140,.4)');
}
