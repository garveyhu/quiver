import type { CSSProperties } from 'react';
import { cssVar } from '@/assets/palette';
import { Worker } from '@/assets/characters';
import { Desk, Plant } from '@/assets/props';

export interface EmptyStateProps {
  title?: string;
  hint?: string;
  className?: string;
  style?: CSSProperties;
}

/** 空状态:工坊静悄悄——一只小工趴桌打盹 + 提示。看板无任务时用。 */
export function EmptyState({
  title = '工坊静悄悄',
  hint = '钉一条委托,小工就会就位开工',
  className,
  style,
}: EmptyStateProps) {
  return (
    <div
      className={className}
      style={{ display: 'inline-flex', flexDirection: 'column', alignItems: 'center', gap: 12, ...style }}
    >
      <div
        style={{
          position: 'relative',
          width: 240,
          height: 140,
          overflow: 'hidden',
          background: 'linear-gradient(180deg,#1b2440,#141b33)',
          boxShadow: '0 0 0 2px var(--qv-wall),0 0 0 4px var(--qv-ink)',
        }}
      >
        <div style={{ position: 'absolute', left: 0, bottom: 0, width: '100%', height: 48, background: cssVar('floor') }} />
        <Plant style={{ left: 18, bottom: 46 }} />
        <div style={{ position: 'absolute', left: 96, bottom: 48 }}>
          <Worker state="idle" body={cssVar('blue')} hair="#3a3550" headphone />
        </div>
        <Desk style={{ left: 80, bottom: 40 }} />
        <span style={{ position: 'absolute', left: 132, top: 26, color: cssVar('dim'), font: '12px ui-monospace,monospace' }}>z</span>
        <span style={{ position: 'absolute', left: 142, top: 16, color: cssVar('dim2'), font: '10px ui-monospace,monospace' }}>z</span>
      </div>
      <div style={{ textAlign: 'center' }}>
        <div style={{ font: 'bold 13px ui-monospace,monospace', color: cssVar('cream') }}>{title}</div>
        <div style={{ font: '11px ui-monospace,monospace', color: cssVar('dim'), marginTop: 4 }}>{hint}</div>
      </div>
    </div>
  );
}
