import type { CSSProperties } from 'react';
import { cssVar } from '@/assets/palette';
import { Panel } from './Panel';
import { Kbd } from './Controls';

export interface Shortcut {
  keys: string[];
  label: string;
}

export interface ShortcutsPanelProps {
  items: Shortcut[];
  onClose?: () => void;
  width?: number;
  style?: CSSProperties;
}

/** 快捷键总览面板。 */
export function ShortcutsPanel({ items, onClose, width = 280, style }: ShortcutsPanelProps) {
  return (
    <Panel title="快捷键 ⌨" onClose={onClose} width={width} style={style}>
      {items.map((s, i) => (
        <div key={i} style={{ display: 'flex', alignItems: 'center', padding: '5px 0' }}>
          <span style={{ flex: 1, font: '11px ui-monospace,monospace', color: cssVar('cream') }}>{s.label}</span>
          <span style={{ display: 'flex', gap: 4 }}>
            {s.keys.map((k, j) => (
              <Kbd key={j}>{k}</Kbd>
            ))}
          </span>
        </div>
      ))}
    </Panel>
  );
}
