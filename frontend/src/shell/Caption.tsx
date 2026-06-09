interface CaptionProps {
  text: string;
}

/** 底部状态条:呼吸灯 + 当前公司状态文案(由上层状态驱动)。 */
export function Caption({ text }: CaptionProps) {
  return (
    <div className="caption">
      <span className="dot" />
      <span>{text}</span>
    </div>
  );
}
