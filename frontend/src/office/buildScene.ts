import {
  bigPlant,
  board,
  bookcase,
  cableRun,
  ceilDuct,
  coffeeBar,
  crateStack,
  deskLamp,
  deskUnit,
  memoryBook,
  patternRug,
  plant,
  pottedShelf,
  qaBench,
  safeBox,
  serverRack,
  shipBay,
  sofa,
  wallClock,
  wallPoster,
  waterCooler,
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

  furnishLounge(b);
  furnishWorkArea(b);
  furnishRightColumn(b);
  ceilingLights(b);

  return { layout, nodes: b.nodes };
}

/** 右列四间:质检台 / 领导区(看板+记忆书) / 运维·预算(机架+保险柜) / 发货口(卷帘门+货箱)。 */
function furnishRightColumn(b: SceneBuilder): void {
  // 质检台
  qaBench(b, 11.7, 0.6);
  plant(b, 12.8, 0.3);
  wallClock(b, 11.1, 0.2, 30);
  deskLamp(b, 11.0, 0.95);
  // 领导区(经理 + 记忆书)
  board(b, 11.2, 2.4);
  memoryBook(b, 12.6, 2.6);
  plant(b, 12.9, 3.5);
  wallPoster(b, 11.05, 2.0, 1.0, ['#3a2e1a', '#e6bd6c', '#ffd27a']);
  deskLamp(b, 12.2, 2.2);
  // 运维 · 预算
  serverRack(b, 11.3, 6.3);
  safeBox(b, 12.6, 7.0);
  cableRun(b, 11.4, 6.0, 44);
  ceilDuct(b, 11.2, 13, 6.0, 50, '#2a2218');
  // 发货口
  shipBay(b, 11.6, 8.3);
  crateStack(b, 12.7, 9.0);
  crateStack(b, 11.2, 9.1);
  pottedShelf(b, 11.1, 8.2);
}

/** 休息室:花纹地毯 + 沙发 ×2 + 咖啡吧 + 书柜 + 大小绿植 + 猫 + 落地窗 + 海报 + 时钟 + 饮水机 + 风管 + 线缆。 */
function furnishLounge(b: SceneBuilder): void {
  patternRug(b, 0.4, 2.2, 2.8, 3.2, 'rgba(176,122,142,.4)', 'rgba(120,84,100,.5)');
  sofa(b, 0.5, 2.4, 1.7);
  sofa(b, 0.5, 4.8, 1.7);
  coffeeBar(b, 2.3, 6.4);
  bookcase(b, 0.15, 0.4, 2.0);
  bigPlant(b, 2.9, 1.0);
  plant(b, 0.5, 6.6);
  plant(b, 2.9, 8.4);
  b.cat(1.7, 3.6);
  windowWall(b, 0.6, 2.6, 0);
  wallPoster(b, 0.05, 2.0, 1.1, ['#2a3550', '#5a86c0', '#ffd27a']);
  wallClock(b, 0.05, 5.6, 34);
  waterCooler(b, 0.2, 8.3);
  pottedShelf(b, 2.95, 5.3);
  ceilDuct(b, 0.5, 2.6, 0.2, 48);
  cableRun(b, 2.4, 0.6, 46);
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
