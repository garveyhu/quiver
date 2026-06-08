import type { ReactNode } from 'react';
import '../pixel.css';
import { Panel } from './Panel';

export interface DialogProps {
  open: boolean;
  title?: ReactNode;
  onClose?: () => void;
  width?: number;
  children?: ReactNode;
  /** 底部操作区(按钮等) */
  actions?: ReactNode;
}

/** 模态弹窗:暗背景 + 居中面板。点背景关闭。 */
export function Dialog({ open, title, onClose, width = 320, children, actions }: DialogProps) {
  if (!open) return null;
  return (
    <div className="qv-backdrop" onClick={onClose}>
      <div onClick={(e) => e.stopPropagation()}>
        <Panel title={title} onClose={onClose} width={width}>
          {children}
          {actions && (
            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end', marginTop: 12 }}>{actions}</div>
          )}
        </Panel>
      </div>
    </div>
  );
}
