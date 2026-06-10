interface CtrlsProps {
  onCmdk: () => void;
  onReport: () => void;
  onGoal: () => void;
  /** 打开经理工作台(决策流 / 自治开关 / 记忆)。 */
  onManager: () => void;
  /** 打开人事部(经理/员工配置 / 专长 / 雇人裁员)。 */
  onPersonnel: () => void;
  /** 打开系统设置(模式 / 模型 / 并发 / 预算 / 验证命令)。 */
  onSettings: () => void;
  /** 打开追溯室(每个任务、每个员工干的每一步、结果)。 */
  onTrace: () => void;
  /** 打开记忆库(公司记住的事实,录入/作废)。 */
  onMemory: () => void;
  /** 当前运行模式,在「CEO 下目标」上标出来 —— 一眼知道派活是免费模拟还是真烧钱。 */
  mode: 'simulate' | 'real';
}

/** 右上角控件栏:常用面板直达入口 + CEO 下目标(标当前模式)。功能不再只藏在 ⌘K 里。 */
export function Ctrls({ onCmdk, onReport, onGoal, onManager, onPersonnel, onSettings, onTrace, onMemory, mode }: CtrlsProps) {
  return (
    <div className="ctrls">
      <button className="b-gh" type="button" onClick={onCmdk}>
        <span className="cmd">⌘K</span>
      </button>
      <button className="b-gh" type="button" onClick={onManager} title="经理工作台:决策流 / 自治 / 记忆">
        经理
      </button>
      <button className="b-gh" type="button" onClick={onPersonnel} title="人事部:配置 / 专长 / 雇人">
        人事部
      </button>
      <button className="b-gh" type="button" onClick={onTrace} title="追溯室:每个任务每一步、花费、结果">
        追溯
      </button>
      <button className="b-gh" type="button" onClick={onMemory} title="记忆库:公司记住的事实 / 录入 / 作废">
        记忆
      </button>
      <button className="b-gh" type="button" onClick={onReport} title="晨报:合进 main / 卡住 / 等你拍板的结果">
        晨报
      </button>
      <button className="b-gh" type="button" onClick={onSettings} title="系统设置:模式 / 模型 / 并发 / 预算">
        设置
      </button>
      <button className="b-go" type="button" onClick={onGoal}>
        CEO 下目标
        <span className="lbl" style={{ marginLeft: 6, opacity: 0.8 }}>
          {mode === 'real' ? '真实' : '模拟'}
        </span>
        <span className="arr" />
      </button>
    </div>
  );
}
