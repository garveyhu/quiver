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
  officeHint: '悬停查看动作 · 点击展开事件流 · 拖拽可挪动工位',

  workerDetailTitle: '工位详情',
  workerDetailClose: '收起',
  workerDetailCost: '花费',
  workerDetailLogEmpty: '暂无事件。',
  workerDetailCostUnknown: '未统计',

  recentHeader: '最近项目',
  recentEmpty: '还没有最近项目。选择一个 git 仓库后会记录在这里。',
  recentReselectTitle: '点击重新选择该项目',

  historyHeader: '运行历史',
  historyEmpty: '暂无历史。运行一个任务后会记录在这里，重启也不会丢失。',
  historyCostPrefix: '花费 $',
  historyCostUnknown: '花费未知',
  historyBranchPrefix: '分支：',

  // --- 设置账本（Phase B）---
  settingsOpen: '设置',
  settingsOpenTitle: '打开设置账本',
  settingsTitle: '设置账本',
  settingsSubtitle: '工坊的规章与度量都记在这本账本里',
  settingsClose: '合上账本',
  settingsCloseAria: '关闭设置',
  settingsSaving: '记录中…',
  settingsSaved: '已记录',
  settingsError: '记录失败，请稍后重试',

  // 分组
  settingsSectionRun: '运行',
  settingsSectionRunHint: '智能体如何开工',
  settingsSectionBudget: '预算',
  settingsSectionBudgetHint: '别让账本透支',
  settingsSectionAgent: 'Agent',
  settingsSectionAgentHint: 'claude 在哪',
  settingsSectionAppearance: '外观',
  settingsSectionAppearanceHint: '工坊的样子',

  // 运行
  settingDefaultMode: '默认模式',
  settingDefaultModeTip: '新任务默认用哪种模式开工。模拟免费、不动真额度；真实会调用官方 claude 消耗订阅额度。',
  settingDefaultModeSimulate: '模拟',
  settingDefaultModeReal: '真实',
  settingModel: '模型',
  settingModelTip: '真实模式下请求的 Claude 模型。sonnet 更快更省，opus 更强但更贵。',
  settingMaxWorkers: '并行工人数',
  settingMaxWorkersTip: '同时干活的弓箭手上限（任务并行度）。当前单任务运行，先记下，多任务队列上线后生效。',
  settingFakeDelay: '模拟节奏',
  settingFakeDelayTip: '模拟模式下每行输出之间的停顿（毫秒）。调小更急促，调大更从容。立即生效。',

  // 预算
  settingMonthlyCap: '月度额度上限',
  settingMonthlyCapTip: '本月愿意花的额度上限（美元）。留空表示不设上限。用于将来的额度守门。',
  settingNightlyBudget: '单晚预算',
  settingNightlyBudgetTip: '一整夜自动运行愿意花的上限（美元）。留空表示不设上限。用于将来的整夜守门。',
  settingBudgetUnset: '未设上限',
  settingBudgetClear: '清空',

  // Agent
  settingAgentBin: 'claude 二进制路径',
  settingAgentBinTip: 'claude 可执行文件的绝对路径。留空则自动解析（PATH 与常见安装位置）。填错会在运行时报错。',
  settingAgentBinPlaceholder: '留空 = 自动解析',

  // --- 任务公告板（Phase C）---
  boardTitle: '任务公告板',
  boardSubtitle: '把委托钉上板子，弓箭手们会并行接单开工',
  boardEmpty: '板上还没有委托。写一条任务钉上去，弓箭手就会接单。',
  boardConcurrencyPrefix: '同时开工上限：',
  boardConcurrencyUnit: ' 名弓箭手',
  boardRunningPrefix: '进行中 ',
  boardQueuedPrefix: '排队 ',
  enqueueTask: '加入公告板',
  enqueueHint: '钉上去后自动排队开工',
  taskCancel: '撤回',
  taskCancelTitle: '撤回这条尚未开工的委托',
  taskDragTitle: '拖拽调整接单顺序',
  taskBranchPrefix: '分支：',
  taskCostPrefix: '花费 $',
  taskCostUnknown: '未统计',
  taskModeSimulate: '模拟',
  taskModeReal: '真实',

  // 外观
  settingTheme: '主题',
  settingThemeTip: '工坊配色。cozy 是温暖羊皮纸主题；midnight 是暖色夜间主题。立即生效。',
  settingThemeCozy: '暖阳',
  settingThemeMidnight: '夜幕',
  settingUiScale: '界面缩放',
  settingUiScaleTip: '整体界面缩放比例。觉得元素太小就调大，想看更多内容就调小。立即生效。',
} as const;

// 运行历史里 status 字段（与 Rust FinishStatus 标签对齐）的中文展示。
export const RUN_STATUS_LABEL: Record<string, string> = {
  verified: '已验证',
  verify_failed: '验证未过',
  failed: '失败',
  needs_rebase: '待变基',
};

// 公告板任务卡的状态徽章中文展示（与 Rust task.status 词表对齐）。
// queued | running | verifying | done(=verified) | failed | needs_rebase
export const TASK_STATUS_LABEL: Record<string, string> = {
  queued: '待接',
  running: '进行中',
  verifying: '验证中',
  verified: '完成',
  done: '完成',
  verify_failed: '验证未过',
  failed: '失败',
  needs_rebase: '待整合',
};

// 各事件类型的中文展示标签（保留底层 kind 作为 key，仅展示文案中文化）。
export const EVENT_KIND_LABEL: Record<AgentEvent['kind'], string> = {
  worker_started: '工人就位',
  tool_use: '调用工具',
  output_chunk: '输出',
  result: '结果',
  error: '错误',
  finished: '完成',
};
