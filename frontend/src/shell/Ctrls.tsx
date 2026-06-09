interface CtrlsProps {
  onCmdk: () => void;
}

/** 右上角控件栏:命令栏 / 晨报 / 自动运转 / CEO 下目标。⌘K 已接命令栏，其余交互后续切片接 IPC。 */
export function Ctrls({ onCmdk }: CtrlsProps) {
  return (
    <div className="ctrls">
      <button className="b-gh" type="button" onClick={onCmdk}>
        <span className="cmd">⌘K</span>
      </button>
      <button className="b-gh" type="button">
        晨报
      </button>
      <button className="b-gh" type="button">
        <span className="ico pause" />
        <span className="lbl">自动运转</span>
      </button>
      <button className="b-go" type="button">
        CEO 下目标
        <span className="arr" />
      </button>
    </div>
  );
}
