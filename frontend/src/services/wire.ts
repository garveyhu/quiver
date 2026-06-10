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
export type TaskStatus =
  | 'queued'
  | 'running'
  | 'verifying'
  | 'verified'
  | 'done'
  | 'merged'
  | 'failed'
  | 'needs_rebase';

/** 运行模式(§4.2):simulate=免费 fake-claude 默认,real=真 claude。Rust 侧小写序列化。 */
export type RunMode = 'simulate' | 'real';

/** 应用设置(get_settings,camelCase)。 */
export interface Settings {
  defaultMode: string;
  model: string;
  maxWorkers: number;
  monthlyCreditCapUsd: number | null;
  /** 今夜硬暂停的美元预算(§10)。null = 未设(不因预算挡)。 */
  nightlyBudgetUsd: number | null;
  agentBinOverride: string | null;
  fakeDelayMs: number;
  theme: string;
  uiScale: number;
  verifyCommand: string;
  /** 自治开关(§5):true=经理控制循环驱动调度(基于决策派活),false=旧 scheduler 流水线。 */
  autonomous: boolean;
}

/** 设置增量补丁(update_settings):字段都可选,只改给到的。 */
export interface SettingsPatch {
  maxWorkers?: number;
  nightlyBudgetUsd?: number | null;
  verifyCommand?: string;
  defaultMode?: string;
  model?: string;
  autonomous?: boolean;
}

/**
 * 启动包(get_initial_state)。本刀只用 `lastProject`(调它的副作用是让后端设好当前项目,
 * 派活才有目标仓库);recentProjects / history 的完整类型待项目选择器切片再补。
 */
export interface InitialState {
  lastProject: string | null;
  recentProjects: unknown[];
  history: unknown[];
}

/**
 * agent-event 实时事件(每个带 taskId,按 kind 标签的判别联合)。
 * 以 quiver-core/src/event.rs 为准:顶层 taskId/seq/tsMs/runner + flatten 的 payload(kind 区分)。
 * finished 由 run.rs 单独发,亦走同一通道(kind="finished")。
 */
interface AgentEventBase {
  taskId: string;
  seq: number;
  tsMs: number;
  runner: string;
}
export type AgentEvent =
  | (AgentEventBase & { kind: 'worker_started'; sessionId: string | null; model: string | null; authMode: string })
  | (AgentEventBase & { kind: 'tool_use'; tool: string; summary: string })
  | (AgentEventBase & { kind: 'output_chunk'; text: string })
  | (AgentEventBase & { kind: 'result'; ok: boolean; costUsd: number | null; numTurns: number; tokens: number | null; durationMs: number | null })
  | (AgentEventBase & { kind: 'error'; code: string; message: string })
  | (AgentEventBase & { kind: 'finished'; status: string; costUsd: number | null; branch: string | null; verifyOutput: string });

/** 一条持久化的 agent 事件(get_task_events,§11 日志)。payloadJson 是 payload 的 JSON 串。 */
export interface StoredEvent {
  taskId: string;
  seq: number;
  tsMs: number;
  runner: string;
  /** worker_started | tool_use | output_chunk | result | error | finished */
  kind: string;
  payloadJson: string;
}

/** 一条过夜交付记录(get_episodes,§6.2):机械绑 git 的 episode。 */
export interface EpisodeRecord {
  id: number;
  project: string;
  nodeId: string | null;
  taskId: string | null;
  commitSha: string | null;
  mergeSeq: number | null;
  /** "verified" | "failed" | … */
  verifyResult: string | null;
  diffStat: string | null;
  summary: string | null;
  createdAt: number;
}

/** 观测指标(get_metrics,§10-12):所有项目已结束运行的聚合。 */
export interface MetricsDto {
  runs: number;
  verified: number;
  failed: number;
  totalCostUsd: number;
  totalTokens: number;
  p50DurationMs: number;
  p95DurationMs: number;
  /** 验收率(自治度核心)0..1 */
  verifyRate: number;
  avgCostUsd: number;
}

/** 经理一拍的决策(§21,action 标签)。本前端主要用 action;字段按需。 */
export interface Decision {
  action: 'spawn' | 'plan' | 'continue' | 'deliver' | 'block' | 'escalate' | 'refresh_memory' | 'noop';
  prompt?: string;
  reason?: string;
}

/** AI 经理在此刻真实局面下的决策预览(manager_preview,只看不动)。 */
export interface ManagerPreview {
  inflight: number;
  queued: number;
  maxInflight: number;
  budgetRemainingUsd: number;
  decision: Decision;
}

/** 记忆简报里的一条当前事实(get_brief,§6;只取展示需要的字段)。 */
export interface BriefFact {
  id: number;
  text: string;
  /** 可信度档(权威/已验证·机械/员工汇报/不可信…)。 */
  trust: string;
  importance: number;
}

/** 经理的记忆简报(get_brief,§6):注入经理上下文的就是这份内容的文本化。 */
export interface Brief {
  project: string;
  facts: BriefFact[];
  recentEpisodes: EpisodeRecord[];
}

/** 人事部:一个角色(人物)的完整配置(agent_role,§14)。 */
export interface AgentRole {
  id: string;
  name: string;
  kind: 'manager' | 'worker';
  /** 经理大脑:rule(免费规则) | claude(真 claude 想,走 headless 额度)。仅经理生效。 */
  brain: 'rule' | 'claude';
  model: string;
  systemPrompt: string;
  budgetUsd: number | null;
  maxTurns: number | null;
  /** 专长标签(§14):测试/前端/安全/通用…任务按专长派给对的人。仅员工有意义。 */
  specialty: string;
  /** 配置版本,改一次 +1(§14)。 */
  version: number;
  updatedAt: number;
}

/** 人事部增量改动:只写给到的字段;budgetUsd/maxTurns 传 null 表示显式清除。 */
export interface RolePatch {
  name?: string;
  brain?: 'rule' | 'claude';
  model?: string;
  systemPrompt?: string;
  budgetUsd?: number | null;
  maxTurns?: number | null;
  specialty?: string;
}

/** 经理控制循环每一拍 emit 的决策(manager-decision 事件,§5)。前端工作台累积成决策流。 */
export interface ManagerDecision {
  project: string;
  /** 决策序号(去重钥匙,也是时间线单调序)。 */
  seq: number;
  action: Decision['action'];
  reason: string | null;
  /** 这拍作用的抽象节点(有则)。 */
  nodeId: string | null;
  /** 抽象节点翻译成的真任务(有则)——经理因此调动了哪个活。 */
  taskId: string | null;
  /** effect 是否真落地(被满载/未知节点等校验拒掉则 false)。 */
  executed: boolean;
  /** spawn 时派的具体任务文本(派的是什么活,让"派活"不再抽象)。 */
  taskPrompt: string | null;
  inflight: number;
  queued: number;
  /** 决策依据:并发上限(设置)+ 经理这拍看到的预算剩余。 */
  maxInflight: number;
  budgetRemainingUsd: number;
  tsMs: number;
}

/** 看板任务行(list_tasks)。 */
/** 一条记忆事实(§6,当前未失效)。记忆库浏览用。 */
export interface FactRecord {
  id: number;
  project: string;
  scope: string;
  kind: string;
  text: string;
  entities: string | null;
  /** 主题/模块(§6.4),矛盾消解的实体键。 */
  entity: string | null;
  importance: number;
  validAt: number | null;
  invalidAt: number | null;
  recordedAt: number;
  /** 可信度档:权威/已验证·机械/已验证·印证/员工汇报/不可信。 */
  trust: string;
}

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
  /** 派给哪个员工(角色名,§14 按人追溯);null = 没记录(旧任务)。 */
  workerRole: string | null;
  /** 父目标(§5 协作):子任务属于哪个被拆的目标;null=不是子任务。 */
  parentGoal: string | null;
}
