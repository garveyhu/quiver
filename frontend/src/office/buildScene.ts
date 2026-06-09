import { computeLayout, zidx, type Layout, C, R, WALLH } from '@/office/iso';
import { SceneBuilder, type SceneNode } from '@/office/primitives';
import { ROOMS, HALLS, roomAt, type RoomKey } from '@/office/rooms';

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

  return { layout, nodes: b.nodes };
}
