import type { CSSProperties } from 'react';
import { cssVar } from '@/assets/palette';
import { Worker } from './Worker';
import type { WorkerState, WorkerSkin } from './worker.types';

export interface WorkerAvatarProps extends WorkerSkin {
  size?: number;
  state?: WorkerState;
  className?: string;
  style?: CSSProperties;
}

/** 工人头像:框住的小工(列表/详情头像)。整只缩放进一个像素框里。 */
export function WorkerAvatar({ size = 48, state = 'idle', body, hair, headphone, className, style }: WorkerAvatarProps) {
  return (
    <div
      className={className}
      style={{
        position: 'relative',
        width: size,
        height: size,
        overflow: 'hidden',
        background: cssVar('floor'),
        boxShadow: '0 0 0 2px var(--qv-wall),0 0 0 4px var(--qv-ink)',
        ...style,
      }}
    >
      <div
        style={{
          position: 'absolute',
          left: '50%',
          bottom: 2,
          transform: `translateX(-50%) scale(${(size - 8) / 46})`,
          transformOrigin: 'bottom center',
        }}
      >
        <Worker state={state} body={body} hair={hair} headphone={headphone} />
      </div>
    </div>
  );
}
