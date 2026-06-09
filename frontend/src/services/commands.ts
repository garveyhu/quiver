import { call } from '@/services/ipc';
import type { Stats, TaskRecord, TaskStatus } from '@/services/wire';

/**
 * 具名后端命令封装 —— 每条 IPC 命令一个有类型的函数。hook/服务层调这里，
 * 不让命令名字符串与返回类型散落在各处。命名与 lib.rs 的 #[tauri::command] 对齐。
 */

/** XP / 等级 / 花费聚合(全项目只读)。 */
export const getStats = (): Promise<Stats> => call<Stats>('get_stats');

/** 看板任务列表(可按项目 / 状态过滤)。 */
export const listTasks = (project?: string, status?: TaskStatus): Promise<TaskRecord[]> =>
  call<TaskRecord[]>('list_tasks', { project, status });
