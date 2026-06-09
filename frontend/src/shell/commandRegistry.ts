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
  { id: 'report', label: '晨报 / 验收', hint: '看昨晚结果' },
  { id: 'trust', label: '信任设置', hint: '调某个经理的规划自由度' },
  { id: 'dispatch', label: '派活', hint: '让经理开一个任务' },
  { id: 'estop', label: '急停 · 全公司', hint: '冻结并停掉所有 AI', kbd: '⌃.' },
  { id: 'timeline', label: '看整夜时间线', hint: '回放过夜运行' },
];
