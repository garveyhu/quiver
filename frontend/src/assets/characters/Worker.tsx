import type { CSSProperties } from 'react';
import '../pixel.css';
import type { WorkerState, WorkerSkin } from './worker.types';

const STATE_CLASS: Record<WorkerState, string> = {
  idle: 'is-idle',
  work: 'is-work',
  coffee: 'is-coffee',
  sweat: 'is-sweat',
  cel: 'is-cel',
  sick: 'is-sick',
};

const CONFETTI_COLORS = ['#ffd27a', '#e2748a', '#7aa7e6', '#7bbf7e'];

export interface WorkerProps extends WorkerSkin {
  state?: WorkerState;
  scale?: number;
  className?: string;
  style?: CSSProperties;
}

/**
 * 像素小工 = 看板上的一个 agent。`state` 切表情/动作,`body`/`hair`/`headphone` 换皮。
 * 动画全在 pixel.css(随状态自动播),无需 JS 逐帧。
 */
export function Worker({
  state = 'idle',
  body,
  hair,
  headphone = false,
  scale = 1,
  className = '',
  style,
}: WorkerProps) {
  const skinVars = {
    '--qv-body': body,
    '--qv-hair': hair,
  } as CSSProperties;

  return (
    <div
      className={`qv-worker ${STATE_CLASS[state]} ${className}`.trim()}
      style={{
        transform: scale !== 1 ? `scale(${scale})` : undefined,
        transformOrigin: '50% 100%',
        ...skinVars,
        ...style,
      }}
    >
      <div className="qv-shadow" />
      <div className="qv-leg l" />
      <div className="qv-leg r" />
      <div className="qv-torso" />
      <div className="qv-arm l">
        <div className="qv-hand" />
      </div>
      <div className="qv-arm r">
        <div className="qv-hand" />
      </div>
      <div className="qv-head" />
      {headphone ? (
        <>
          <div className="qv-ear l" />
          <div className="qv-ear r" />
          <div className="qv-band" />
        </>
      ) : (
        <>
          <div className="qv-hair" />
          <div className="qv-tuft" />
        </>
      )}
      <div className="qv-cheek l" />
      <div className="qv-cheek r" />
      <div className="qv-eye l" />
      <div className="qv-eye r" />
      <div className="qv-mouth" />

      {state === 'coffee' && (
        <>
          <div className="qv-mug" />
          <div className="qv-mug-steam" />
        </>
      )}
      {state === 'sweat' && <div className="qv-drop" />}
      {state === 'sick' && <div className="qv-swirl" />}
      {state === 'cel' &&
        CONFETTI_COLORS.concat(CONFETTI_COLORS.slice(0, 2)).map((c, i) => (
          <div
            key={i}
            className="qv-conf"
            style={{
              left: 4 + i * 5,
              top: -6,
              background: c,
              animationDuration: `${1 + (i % 3) * 0.3}s`,
              animationDelay: `${i * 0.18}s`,
            }}
          />
        ))}
    </div>
  );
}
