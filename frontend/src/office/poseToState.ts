import type { Pose } from '@/office/types';
import type { WorkerState, WorkerSkin } from '@/assets';
import { cssVar } from '@/assets';

/** poseMachine 的 Pose → 像素 Worker 组件的 6 状态。 */
export const poseToState: Record<Pose, WorkerState> = {
  arriving: 'idle',
  idle: 'idle',
  working: 'work',
  tense: 'sweat',
  celebrate: 'cel',
  sick: 'sick',
};

/** WorkerState → 工位灯色(光是状态反馈的母语:余光扫一眼就知全屋战况)。 */
export const STATE_LIGHT: Record<WorkerState, string> = {
  idle: cssVar('dim2'), // 暗——待命
  work: cssVar('amber'), // 暖黄——干活
  coffee: cssVar('amber2'), // 暖——歇着/排队
  sweat: cssVar('blue'), // 冷蓝——跑命令/紧张
  cel: cssVar('green'), // 绿——验证通过
  sick: cssVar('red'), // 红——失败
};

/** slot(0–3)→ 4 种工人皮肤,看板上区分任务。 */
export const SLOT_SKIN: WorkerSkin[] = [
  { body: cssVar('pink'), hair: '#5a3a4a', headphone: false },
  { body: cssVar('blue'), hair: '#3a3550', headphone: true },
  { body: cssVar('green'), hair: '#3a3550', headphone: true },
  { body: cssVar('amber2'), hair: '#caa', headphone: false },
];
