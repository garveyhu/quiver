/**
 * 后端 IPC 的 wire 类型。**以 src-tauri/src/lib.rs 与各 crate 为准**(Rust 侧 camelCase 序列化)。
 * 只在这里声明一次，hook/服务层引用，展示组件不直接碰后端形状。
 */

/** XP / 等级 / 花费聚合(get_stats，全项目只读)。 */
export interface Stats {
  total: number;
  verified: number;
  failed: number;
  costUsd: number;
  xp: number;
  level: number;
  /** 滚动 24h 花费，对齐 §10 预算闸 —— HUD「今夜花费」与预算条用它。 */
  spentDay: number;
  spentMonth: number;
  /** 滚动 24h 验收/失败数 —— 晨报「昨晚」用。 */
  verifiedDay: number;
  failedDay: number;
}

/**
 * 任务生命周期状态。以 src-tauri/src/run.rs 实际写入为准:成功终态是 `verified`(run.rs 验证通过写它),
 * `verifying` 是过渡态;`done` 亦在用。`queued | running | verifying | verified | done | failed | needs_rebase`。
 */
export type TaskStatus = 'queued' | 'running' | 'verifying' | 'verified' | 'done' | 'failed' | 'needs_rebase';

/** 运行模式(§4.2):simulate=免费 fake-claude 默认,real=真 claude。Rust 侧小写序列化。 */
export type RunMode = 'simulate' | 'real';

/**
 * 启动包(get_initial_state)。本刀只用 `lastProject`(调它的副作用是让后端设好当前项目,
 * 派活才有目标仓库);recentProjects / history 的完整类型待项目选择器切片再补。
 */
export interface InitialState {
  lastProject: string | null;
  recentProjects: unknown[];
  history: unknown[];
}

/** 看板任务行(list_tasks)。 */
export interface TaskRecord {
  id: string;
  project: string;
  prompt: string;
  /** "simulate" | "real" */
  mode: string;
  status: TaskStatus;
  costUsd: number | null;
  branch: string | null;
  /** 可手排的看板位置(越小越靠前)。 */
  position: number;
  createdAt: number;
  updatedAt: number;
}
