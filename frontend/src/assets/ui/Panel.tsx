import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';

export interface PanelProps {
  /** 标题栏文字;不传则无标题栏 */
  title?: ReactNode;
  /** 右上角关闭回调(传了才显示 ✕) */
  onClose?: () => void;
  width?: number;
  bodyPadding?: number;
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

/** 像素面板:所有窗口/弹层的底(详情、设置账本、对话框)。双层硬描边 + 内高光。 */
export function Panel({ title, onClose, width, bodyPadding = 10, children, className, style }: PanelProps) {
  return (
    <div className={`qv-panel ${className ?? ''}`.trim()} style={{ width, ...style }}>
      {title !== undefined && (
        <div className="qv-titlebar">
          <span>{title}</span>
          {onClose && (
            <span role="button" aria-label="关闭" style={{ cursor: 'pointer' }} onClick={onClose}>
              ✕
            </span>
          )}
        </div>
      )}
      <div style={{ padding: bodyPadding }}>{children}</div>
    </div>
  );
}
