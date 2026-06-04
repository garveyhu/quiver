import type { AgentEvent } from '@/types/agentEvent.types';

// 集中存放所有界面文案（v1 仅中文）。品牌标题 "Quiver" 保持英文，其余全部中文。
// 没有引入完整 i18n 框架——v1 单语言，硬编码中文即可。

export const STR = {
  brand: 'Quiver',
  tagline: '一个任务 → 一个 worktree → 一个智能体 → 实时事件流',

  pickProject: '选择项目',
  projectPathPrefix: '项目：',
  noProjectSelected: '尚未选择 git 仓库',

  modeAriaLabel: '运行模式',
  modeSimulate: '模拟运行（免费）',
  modeReal: '真实 Claude（消耗额度）',
  realWarning:
    '真实模式会用真 Claude 智能体在所选仓库上读写文件、执行命令。' +
    '暂无沙箱（Phase 6），请用你不介意的仓库。' +
    '会消耗你的月度额度。产物只留在分支、不并入 main。',

  runTask: '运行任务',
  running: '运行中…',
  taskPlaceholder: '读 README.md，用一句话总结这个项目',
  taskInputAriaLabel: '任务描述',
  noProjectHint: '请先选择一个项目',

  taskFailedPrefix: '任务失败：',

  eventsHeader: '智能体事件',
  eventsEmpty: '暂无事件。运行一个任务，看智能体开工。',

  officeTitle: '工坊',
  officeEmpty: '工坊空荡荡。运行一个任务，弓箭手就会就位开工。',
} as const;

// 各事件类型的中文展示标签（保留底层 kind 作为 key，仅展示文案中文化）。
export const EVENT_KIND_LABEL: Record<AgentEvent['kind'], string> = {
  worker_started: '工人就位',
  tool_use: '调用工具',
  output_chunk: '输出',
  result: '结果',
  error: '错误',
  finished: '完成',
};
