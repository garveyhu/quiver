import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';
import { CostTag } from './Hud';

export interface ProjectCardProps {
  name: string;
  path: string;
  meta?: string;
  onClick?: () => void;
  style?: CSSProperties;
}

/** 项目卡片(最近项目 / 选择工坊)。 */
export function ProjectCard({ name, path, meta, onClick, style }: ProjectCardProps) {
  return (
    <div className="qv-card" style={{ width: 210, ...style }} onClick={onClick}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span style={{ color: cssVar('amber') }}>▰</span>
        <span style={{ font: 'bold 12px ui-monospace,monospace', color: cssVar('cream') }}>{name}</span>
      </div>
      <div
        style={{
          font: '10px ui-monospace,monospace',
          color: cssVar('dim2'),
          marginTop: 4,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}
      >
        {path}
      </div>
      {meta && <div style={{ font: '10px ui-monospace,monospace', color: cssVar('dim'), marginTop: 6 }}>{meta}</div>}
    </div>
  );
}

export interface EventRowProps {
  glyph?: string;
  color?: string;
  text: ReactNode;
  time?: string;
  cost?: number;
}

/** 事件流条目(智能体事件列表的一行):图标 + 文本 + 花费 + 时间。 */
export function EventRow({ glyph = '·', color, text, time, cost }: EventRowProps) {
  return (
    <div className="qv-eventrow">
      <span style={{ color: color ?? cssVar('amber'), width: 14, textAlign: 'center' }}>{glyph}</span>
      <span style={{ flex: 1 }}>{text}</span>
      {cost !== undefined && <CostTag usd={cost} />}
      {time && <span style={{ color: cssVar('dim2') }}>{time}</span>}
    </div>
  );
}
