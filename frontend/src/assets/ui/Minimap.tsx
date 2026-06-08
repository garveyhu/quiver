import type { CSSProperties } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';
import type { WorkerState } from '@/assets/characters';

const DOT: Record<WorkerState, string> = {
  idle: cssVar('cream'),
  work: cssVar('blue'),
  coffee: cssVar('amber'),
  sweat: cssVar('amber2'),
  cel: cssVar('green'),
  sick: cssVar('red'),
};

export interface MinimapDot {
  /** 百分比坐标 0–100 */
  x: number;
  y: number;
  state: WorkerState;
}

export interface MinimapProps {
  dots: MinimapDot[];
  width?: number;
  height?: number;
  style?: CSSProperties;
}

/** 迷你地图:工坊俯视小图,小工按状态着色发光。 */
export function Minimap({ dots, width = 120, height = 70, style }: MinimapProps) {
  return (
    <div className="qv-mini" style={{ width, height, ...style }}>
      {dots.map((d, i) => (
        <div
          key={i}
          className="qv-mini-dot"
          style={{ left: `${d.x}%`, top: `${d.y}%`, background: DOT[d.state], color: DOT[d.state] }}
        />
      ))}
    </div>
  );
}
