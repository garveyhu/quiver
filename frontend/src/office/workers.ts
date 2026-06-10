import { iso, zidx, type Layout } from '@/office/iso';
import { DESKS } from '@/office/rooms';
import type { TaskRecord } from '@/services/wire';

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
  /** 在工位敲键(typebob 动画) */
  working?: boolean;
  /** 头顶气泡文字 */
  label?: string;
  /** 正在干的任务 id(working 时有),worksurf 下钻拉事件流用 */
  taskId?: string;
  /** 正在干/刚完工的任务状态(running/verified/…),驱动 worksurf 的任务链路条。 */
  taskStatus?: string;
  /** 真实员工身份(角色名,§14):这个小人是哪个员工在干 —— 让"专长分工"具体可见。 */
  workerRole?: string;
  /** 经理正在用 claude 思考决策(头顶冒思考点 + 轻浮动)——让 CEO 扫一眼就知道 AI 在工作。 */
  thinking?: boolean;
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

const EMP_COUNT = 5;

/** 员工气泡用的短标签:剥掉注入的"(公司约定:…)"记忆尾巴(那是给 worker 看的,不是气泡),
 * 只留任务名再截断 —— 气泡干净,记忆约定在追溯室/调度台看完整。 */
function shortLabel(prompt: string): string {
  const clean = prompt.split('(公司约定:')[0].trim();
  return clean.length > 10 ? `${clean.slice(0, 10)}…` : clean;
}

/** 刚完工、在工位驻留收尾的任务(D1:不瞬移,气泡报结果几秒再走)。 */
export interface LingeringTask {
  task: TaskRecord;
  /** 终态是否验收通过(/气泡)。 */
  ok: boolean;
}

/**
 * 按在途任务摆工人(移植原型 build() 的三类工人 + 派活落工位):
 * 休息室 EMP_COUNT 个员工,有几件在途任务就把前几个移到工位敲键(其余在休息室待命,末一个待命时冒思考点);
 * 刚完工的在工位**驻留收尾**(不敲键,气泡报/,几秒后才回休息室,D1);
 * 领导区 1 个经理 + 质检台 1 个独立审计常驻。
 * 员工 id 稳定(emp0..),React 复用同一 DOM → 位置变化由 .worker 的 left/top 过渡平滑滑行。
 * 在途优先占工位,驻留用剩余工位(不够就直接回休息室,绝不挤新活)。
 */
export function placeWorkers(
  layout: Layout,
  active: TaskRecord[],
  bubbles: Record<string, string> = {},
  lingering: LingeringTask[] = [],
  mgrThinking = false,
): PlacedWorker[] {
  const cells = loungeCells();
  const workers: PlacedWorker[] = [];
  const deskCap = Math.min(EMP_COUNT, DESKS.length);
  const busy = Math.min(active.length, deskCap);
  const lingerCount = Math.max(0, Math.min(lingering.length, deskCap - busy));

  for (let i = 0; i < EMP_COUNT; i++) {
    const hood = HOODS[i % HOODS.length];
    if (i < busy) {
      const task = active[i];
      const [dc, dr] = DESKS[i];
      const p = iso(layout, dc, dr);
      // 气泡优先用 agent-event 的实时工具摘要,缺时退回任务目标。
      const label = bubbles[task.id] ?? shortLabel(task.prompt);
      workers.push({ id: `emp${i}`, role: 'emp', x: p.x, y: p.y, z: zidx(dc, dr) + 5, hood, working: true, lean: true, label, taskId: task.id, taskStatus: task.status, workerRole: task.workerRole ?? undefined });
    } else if (i < busy + lingerCount) {
      // 完工驻留(D1):还在工位但不敲键,气泡报结果;到点(useWorkers 计时)回休息室。
      const linger = lingering[i - busy];
      const [dc, dr] = DESKS[i];
      const p = iso(layout, dc, dr);
      workers.push({
        id: `emp${i}`,
        role: 'emp',
        x: p.x,
        y: p.y,
        z: zidx(dc, dr) + 5,
        hood,
        label: linger.ok ? '验收通过' : '没过验收',
        taskId: linger.task.id,
        taskStatus: linger.task.status,
        workerRole: linger.task.workerRole ?? undefined,
      });
    } else {
      const [c, r] = cells[(i * 5 + 2) % cells.length];
      const p = iso(layout, c, r);
      workers.push({ id: `emp${i}`, role: 'emp', x: p.x, y: p.y, z: zidx(c, r) + 5, hood, awaiting: i === EMP_COUNT - 1 && busy === 0 });
    }
  }

  const mp = iso(layout, 11.5, 3.3);
  workers.push({
    id: 'mgr',
    role: 'mgr',
    x: mp.x,
    y: mp.y,
    z: zidx(11.5, 3.3) + 5,
    hood: ['#e0a050', '#b07a30'],
    thinking: mgrThinking,
    // 思考中改 label,头顶思考点 + 浮动一起把"经理在用 AI 想"亮出来。
    label: mgrThinking ? '思考中…' : '经理',
  });

  const ap = iso(layout, 11.3, 0.4);
  workers.push({ id: 'aud', role: 'aud', x: ap.x, y: ap.y, z: zidx(11.3, 0.4) + 5, hood: ['#46a0a0', '#2f7070'], lean: true, label: '审计' });

  return workers;
}
