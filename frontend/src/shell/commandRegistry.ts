/** 命令栏(⌘K)的命令注册表。移植原型 CMDS;动作随对应面板/流程建好再逐条接上。 */
export interface QuiverCommand {
  id: string;
  label: string;
  hint: string;
  /** 显示用快捷键提示(非真实绑定) */
  kbd?: string;
}

export const COMMANDS: QuiverCommand[] = [
  { id: 'new-task', label: '新任务', hint: '下一个目标交给公司', kbd: '⌘N' },
  { id: 'manager', label: '经理工作台', hint: '看经理逐拍决策 / 开自治', kbd: 'M' },
  { id: 'personnel', label: '人事部', hint: '经理/员工配置 · 大脑/模型/硬限' },
  { id: 'settings', label: '系统设置', hint: '模式 / 模型 / 并发 / 预算 / 验证命令' },
  { id: 'trace', label: '追溯室', hint: '每个任务每一步 · claude 思考/工具/花费/结果' },
  { id: 'memory', label: '记忆库', hint: '公司记住的事实 · 录入 / 作废' },
  { id: 'board', label: '调度台', hint: '工作队列 · 调优先级 / 取消' },
  { id: 'report', label: '晨报 / 验收', hint: '看昨晚结果' },
  { id: 'trust', label: '信任设置', hint: '调某个经理的规划自由度' },
  { id: 'dispatch', label: '派活', hint: '让经理开一个任务' },
  { id: 'dispatch-batch', label: '派一批活', hint: '一次下 3 个目标,看经理并行调度' },
  { id: 'estop', label: '急停 · 全公司', hint: '冻结并停掉所有 AI', kbd: '⌃.' },
  { id: 'timeline', label: '看整夜时间线', hint: '回放过夜运行' },
];
