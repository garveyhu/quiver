import { call } from '@/services/ipc';
import type { InitialState, ManagerPreview, MetricsDto, RunMode, Stats, TaskRecord, TaskStatus } from '@/services/wire';

/**
 * 具名后端命令封装 —— 每条 IPC 命令一个有类型的函数。hook/服务层调这里，
 * 不让命令名字符串与返回类型散落在各处。命名与 lib.rs 的 #[tauri::command] 对齐。
 */

/** XP / 等级 / 花费聚合(全项目只读)。 */
export const getStats = (): Promise<Stats> => call<Stats>('get_stats');

/** 看板任务列表(可按项目 / 状态过滤)。 */
export const listTasks = (project?: string, status?: TaskStatus): Promise<TaskRecord[]> =>
  call<TaskRecord[]>('list_tasks', { project, status });

/** 启动包:读 last project / recents / history(副作用:后端据此设好当前项目)。 */
export const getInitialState = (): Promise<InitialState> => call<InitialState>('get_initial_state');

/** 派一件事给公司:入队任务并自动 kick 调度器跑(simulate 走 fake-claude,免费)。 */
export const enqueueTask = (prompt: string, mode: RunMode = 'simulate'): Promise<TaskRecord> =>
  call<TaskRecord>('enqueue_task_cmd', { prompt, mode });

/** AI 经理在此刻真实局面下会做的决策(只看不动,不 spawn/不花钱)。 */
export const managerPreview = (): Promise<ManagerPreview> => call<ManagerPreview>('manager_preview');

/** 观测指标聚合(验收率/时延/花费,§10-12)。 */
export const getMetrics = (): Promise<MetricsDto> => call<MetricsDto>('get_metrics');
