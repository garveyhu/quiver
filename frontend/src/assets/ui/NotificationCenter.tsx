import type { CSSProperties } from 'react';
import { cssVar } from '@/assets/palette';
import { Panel } from './Panel';

export interface NotifItem {
  kind: 'success' | 'error' | 'info';
  text: string;
  time: string;
}

const GLYPH: Record<NotifItem['kind'], { color: string; glyph: string }> = {
  success: { color: cssVar('green'), glyph: '✓' },
  error: { color: cssVar('red'), glyph: '✕' },
  info: { color: cssVar('blue'), glyph: '◔' },
};

export interface NotificationCenterProps {
  items: NotifItem[];
  onClose?: () => void;
  width?: number;
  style?: CSSProperties;
}

/** 通知中心:可滚动的通知列表面板。 */
export function NotificationCenter({ items, onClose, width = 260, style }: NotificationCenterProps) {
  return (
    <Panel title="通知 ◔" onClose={onClose} width={width} bodyPadding={0} style={style}>
      <div className="qv-scroll" style={{ maxHeight: 200, overflowY: 'auto' }}>
        {items.map((n, i) => {
          const g = GLYPH[n.kind];
          return (
            <div key={i} className="qv-eventrow">
              <span style={{ color: g.color, width: 14, textAlign: 'center' }}>{g.glyph}</span>
              <span style={{ flex: 1 }}>{n.text}</span>
              <span style={{ color: cssVar('dim2') }}>{n.time}</span>
            </div>
          );
        })}
        {items.length === 0 && (
          <div style={{ padding: 14, color: cssVar('dim'), font: '11px ui-monospace,monospace' }}>暂无通知</div>
        )}
      </div>
    </Panel>
  );
}
