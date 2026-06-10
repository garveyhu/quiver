interface CtrlsProps {
  onCmdk: () => void;
  onReport: () => void;
  onGoal: () => void;
  /** 打开经理工作台(决策流 / 自治开关 / 记忆)。 */
  onManager: () => void;
  /** 打开人事部(经理/员工配置 / 专长 / 雇人裁员)。 */
  onPersonnel: () => void;
}

/** 右上角控件栏:常用面板直达入口 + CEO 下目标。功能不再只藏在 ⌘K 里。 */
export function Ctrls({ onCmdk, onReport, onGoal, onManager, onPersonnel }: CtrlsProps) {
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
      <button className="b-gh" type="button" onClick={onReport}>
        晨报
      </button>
      <button className="b-go" type="button" onClick={onGoal}>
        CEO 下目标
        <span className="arr" />
      </button>
    </div>
  );
}
