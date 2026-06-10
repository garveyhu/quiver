import { call } from '@/services/ipc';
import type {
  AgentRole,
  Brief,
  EpisodeRecord,
  InitialState,
  ManagerDecision,
  ManagerPreview,
  MetricsDto,
  RolePatch,
  RunMode,
  Settings,
  SettingsPatch,
  Stats,
  StoredEvent,
  TaskRecord,
  TaskStatus,
} from '@/services/wire';

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

/** 打回重做(§12 回流):已终结任务作为新任务重新入队(老行留档),经理会带记忆再派。 */
export const requeueTask = (taskId: string): Promise<TaskRecord> =>
  call<TaskRecord>('requeue_task_cmd', { taskId });

/** 取消一个任务(queued 删除 / running·verifying 杀进程 / 已结束报错)。 */
export const cancelTask = (id: string): Promise<void> => call<void>('cancel_task_cmd', { id });

/** 取消所有在途/排队任务(急停用)。返回成功取消的条数。 */
export async function cancelActiveTasks(): Promise<number> {
  const tasks = await call<TaskRecord[]>('list_tasks', {});
  const active = tasks.filter(t => t.status === 'running' || t.status === 'verifying' || t.status === 'queued');
  const results = await Promise.allSettled(active.map(t => cancelTask(t.id)));
  return results.filter(r => r.status === 'fulfilled').length;
}

/** AI 经理在此刻真实局面下会做的决策(只看不动,不 spawn/不花钱)。 */
export const managerPreview = (): Promise<ManagerPreview> => call<ManagerPreview>('manager_preview');

/** 观测指标聚合(验收率/时延/花费,§10-12)。 */
export const getMetrics = (): Promise<MetricsDto> => call<MetricsDto>('get_metrics');

/** 当前项目近期 episode(过夜交付记录,§6.2),时间线用。 */
export const getEpisodes = (limit = 12): Promise<EpisodeRecord[]> => call<EpisodeRecord[]>('get_episodes', { limit });

/** 某任务的持久化事件流(§11 日志),worksurf 下钻看真实轨迹用。 */
export const getTaskEvents = (taskId: string): Promise<StoredEvent[]> => call<StoredEvent[]>('get_task_events', { taskId });

/** 读应用设置(默认模式/模型/并发上限/预算/verify 命令…)。 */
export const getSettings = (): Promise<Settings> => call<Settings>('get_settings');

/** 应用设置补丁,返回更新后的完整设置。 */
export const updateSettings = (patch: SettingsPatch): Promise<Settings> => call<Settings>('update_settings', { patch });

/** 当前项目最近的经理决策(decision_log,最新在前;§10)。工作台回填决策流历史。 */
export const getDecisions = (limit = 40): Promise<ManagerDecision[]> =>
  call<ManagerDecision[]>('get_decisions', { limit });

/** 经理的记忆简报(当前事实+近期 episode,§6)——注入经理决策上下文的内容。 */
export const getBrief = (): Promise<Brief> => call<Brief>('get_brief');

/** 人事部:全部角色配置(经理在前,§14)。 */
export const listRoles = (): Promise<AgentRole[]> => call<AgentRole[]>('list_roles');

/** 人事部:增量改一个角色(version+1)。改经理 brain 即切换经理大脑。 */
export const updateRole = (id: string, patch: RolePatch): Promise<AgentRole> =>
  call<AgentRole>('update_role', { id, patch });
