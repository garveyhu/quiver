interface CtrlsProps {
  onCmdk: () => void;
  onReport: () => void;
  onGoal: () => void;
}

/** 右上角控件栏:命令栏 / 晨报 / 自动运转 / CEO 下目标。⌘K、晨报、派活已接，自动运转后续切片接。 */
export function Ctrls({ onCmdk, onReport, onGoal }: CtrlsProps) {
  return (
    <div className="ctrls">
      <button className="b-gh" type="button" onClick={onCmdk}>
        <span className="cmd">⌘K</span>
      </button>
      <button className="b-gh" type="button" onClick={onReport}>
        晨报
      </button>
      <button className="b-gh" type="button">
        <span className="ico pause" />
        <span className="lbl">自动运转</span>
      </button>
      <button className="b-go" type="button" onClick={onGoal}>
        CEO 下目标
        <span className="arr" />
      </button>
    </div>
  );
}
