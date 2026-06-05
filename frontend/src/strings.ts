import type { AgentEvent } from '@/types/agentEvent.types';

// 集中存放所有界面文案（v1 仅中文）。品牌标题 "Quiver" 保持英文，其余全部中文。
// 没有引入完整 i18n 框架——v1 单语言，硬编码中文即可。

export const STR = {
  brand: 'Quiver',
  tagline: '一个任务 → 一个 worktree → 一个智能体 → 实时事件流',

  pickProject: '选择项目',
  projectPathPrefix: '项目：',
  noProjectSelected: '尚未选择 git 仓库',
  firstRunTitle: '先挑一个工坊',
  firstRunHint: '选一个 git 仓库当作你的工坊，弓箭手就能在里面接单干活。',

  modeAriaLabel: '运行模式',
  modeSimulate: '模拟运行（免费）',
  modeReal: '真实 Claude（消耗额度）',
  realWarning:
    '真实模式会用真 Claude 智能体在所选仓库上读写文件、执行命令。' +
    '暂无沙箱（Phase 6），请用你不介意的仓库。' +
    '会消耗你的月度额度。产物只留在分支、不并入 main。',

  taskPlaceholder: '读 README.md，用一句话总结这个项目',
  taskInputAriaLabel: '任务描述',

  taskFailedPrefix: '任务失败：',

  eventsHeader: '智能体事件',
  eventsEmpty: '暂无事件。运行一个任务，看智能体开工。',

  officeTitle: '工坊',
  officeEmptyTitle: '工坊静悄悄',
  officeEmpty: '炉火正暖，工位空着。写一条委托钉上公告板，弓箭手就会就位开工。',
  officeHint: '悬停查看动作 · 点击展开事件流 · 拖拽可挪动工位',

  workerDetailTitle: '工位详情',
  workerDetailClose: '收起',
  workerDetailCost: '花费',
  workerDetailLogEmpty: '暂无事件。',
  workerDetailCostUnknown: '未统计',

  recentHeader: '最近项目',
  recentEmpty: '还没有最近项目。选择一个 git 仓库后会记录在这里。',
  recentReselectTitle: '点击重新选择该项目',

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
  boardEmptyTitle: '公告板空着',
  boardEmpty: '还没有委托钉上来。写一条任务钉上去，弓箭手就会接单开工。',
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

  // --- 导航（Phase D）---
  navWorkshop: '工坊',
  navBoard: '公告板',
  navArchive: '档案',
  navWorkshopTip: '看弓箭手实时干活与当前任务队列',
  navBoardTip: '把委托钉上公告板、排队、调整顺序',
  navArchiveTip: '翻阅过往委托记录，点开看完整 I/O 卷轴',
  tabHasUpdate: '有更新 — 一次运行刚刚完成',

  // --- 档案库 / 书架（Phase D）---
  archiveTitle: '委托档案库',
  archiveSubtitle: '过往委托都装订在这书架上，可翻阅、搜索、回看完整记录',
  archiveEmpty: '书架空空。完成几次委托后，记录会装订在这里。',
  archiveNoMatch: '没有符合条件的委托记录。换个筛选或搜索词试试。',
  archiveSearchPlaceholder: '搜索委托内容…',
  archiveSearchAria: '按委托内容搜索',
  archiveFilterStatus: '状态',
  archiveFilterProject: '项目',
  archiveFilterAll: '全部',
  archiveCountPrefix: '共 ',
  archiveCountSuffix: ' 卷',
  archiveOpenTip: '点击翻开这卷委托记录',
  archiveCostPrefix: '花费 $',
  archiveCostUnknown: '花费未统计',
  archiveBranchPrefix: '分支：',

  // --- 卷轴 / Logbook（Phase D）---
  logbookTitle: '委托卷轴',
  logbookClose: '合上卷轴',
  logbookCloseAria: '关闭卷轴',
  logbookLoading: '正在展开卷轴…',
  logbookError: '卷轴读取失败，请稍后重试',
  logbookEmpty: '这卷委托没有留下任何事件记录。',
  logbookPromptLabel: '委托内容',
  logbookProjectLabel: '项目',
  logbookModeLabel: '模式',
  logbookStatusLabel: '状态',
  logbookBranchLabel: '分支',
  logbookTimelineTitle: '运行时间线',
  logbookTotalEvents: '事件',
  logbookTotalCost: '花费',
  logbookTotalDuration: '历时',
  logbookCostUnknown: '未统计',
  logbookReplay: '在工坊回放',
  logbookReplayTip: '让弓箭手按原始节奏重演这次运行（仅视觉，不消耗额度）',
  logbookReplaying: '回放中…',
  replayStop: '停止回放',

  // --- 游戏世界 / HUD（里程碑 1：game-first 翻转）---
  // 世界内可点击物件的标签（公告板 / 账本 / 书架 / 门）
  hotspotBoard: '公告板',
  hotspotSettings: '账本',
  hotspotArchive: '书架',
  hotspotProject: '门 · 选项目',
  // empty-state (no project) door hint — the one glowing call-to-action
  firstRunWake: '点亮工坊 · 选择项目',

  // 顶部 HUD
  hudBudgetPrefix: '余额',
  hudBudgetUnset: '未设预算上限',
  hudBusyPrefix: '工坊里有 ',
  hudBusySuffix: ' 位弓匠在忙',
  hudIdle: '工坊静悄悄',
  hudToastDone: '完成 ✓',

  // 临时覆盖层（点物件后打开的现有面板，P3-P4 会替换成世界内场景）
  overlayClose: '× 回到工坊',

  // --- 世界内公告板场景（里程碑 2 / P2）---
  boardSceneTitle: '任务公告板',
  boardSceneBack: '← 回到工坊',
  boardScenePin: '＋ 钉新委托',
  boardSceneEmpty: '公告板空着 — 钉一条委托上来吧',
  boardSceneNoProject: '先在门口选一个工坊（git 仓库），才能钉委托',
  boardCardCancel: '撤回',
  boardCardCancelTip: '撤回这条尚未开工的委托',
  boardCardDragTip: '拖动重新排序（仅待接委托）',

  // 文本浮层（中文 IME 安全，§0 option-b，全场景复用）
  textInputSubmit: '钉上去',
  textInputCancel: '取消',
  textInputNewCommission: '写一条委托',

  // 外观
  settingTheme: '主题',
  settingThemeTip: '工坊配色。cozy 是温暖羊皮纸主题；midnight 是暖色夜间主题。立即生效。',
  settingThemeCozy: '暖阳',
  settingThemeMidnight: '夜幕',
  settingUiScale: '界面缩放',
  settingUiScaleTip: '整体界面缩放比例。觉得元素太小就调大，想看更多内容就调小。立即生效。',

  // --- 世界内档案库场景（里程碑 4 / P4）---
  archiveSceneTitle: '委托档案库',
  archiveSceneBack: '← 回到工坊',
  archiveSceneEmpty: '书架空空 — 完成几次委托后，记录会装订上架',
  archiveSceneNoMatch: '没有符合条件的卷宗 — 换个筛选或搜索词',
  archiveSceneSearchHint: '点此搜索委托内容…',
  archiveSceneSearchClear: '清除搜索',
  archiveSceneFilterStatusAll: '全部状态',
  archiveSceneFilterProjectAll: '全部项目',
  archiveSceneSearchSubmit: '搜索',
  archiveSceneCountPrefix: '共 ',
  archiveSceneCountSuffix: ' 卷',
  archiveSceneOpenTip: '抽出这卷委托记录展开卷轴',

  // --- 世界内卷轴场景（里程碑 4 / P4）---
  logbookSceneBack: '← 卷起卷轴',
  logbookSceneReplay: '▶ 在工坊回放',
  logbookSceneReplayTip: '让弓箭手按原始节奏重演这次运行（仅视觉，不消耗额度）',
  logbookSceneLoading: '正在展开卷轴…',
  logbookSceneError: '卷轴读取失败，请稍后重试',
  logbookSceneEmpty: '这卷委托没有留下任何事件记录。',
  logbookSceneTotalEvents: '事件 ',
  logbookSceneTotalCost: '花费 ',
  logbookSceneTotalDuration: '历时 ',
  logbookSceneTurnsPrefix: '轮次 ',

  // --- 世界内设置账本场景（里程碑 3 / P3）---
  settingsSceneBack: '← 合上账本',
  settingsSceneTitle: '设置账本',
  // 点击数字 / 文本格时浮层的提交按钮文案
  settingsFieldSave: '记下',
  // 点空的文本格时的占位提示
  settingMaxWorkersUnit: ' 名',
  settingFakeDelayUnit: ' ms',
  settingUiScaleUnit: '%',
  settingModelTapHint: '点此填写模型名',
  settingBudgetTapHint: '点此填写上限',
} as const;

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
