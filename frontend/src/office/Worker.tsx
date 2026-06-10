import type { CSSProperties } from 'react';

import { shade, type PlacedWorker } from '@/office/workers';

interface WorkerProps {
  worker: PlacedWorker;
  /** 点小人 → 下钻聚焦 */
  onDive: (worker: PlacedWorker) => void;
}

/**
 * 一个像素工人精灵 —— 忠实移植原型 makeWorker 的 DOM 结构。各部件绝对定位，
 * 连帽衫/上衣三阶明暗由 CSS 变量驱动;角色(emp/mgr/aud)与状态(awaiting/lean/talk)走 class。
 * 本刀只渲染静态站姿;走位/敲键/庆祝等动画 class 已在 CSS 备好，待下一刀的事件驱动接上。
 */
export function Worker({ worker, onDive }: WorkerProps) {
  const { role, x, y, z, hood, awaiting, lean, working, label, thinking } = worker;
  const [hoodColor, torsoColor] = hood;

  const className = ['worker', role, awaiting && 'awaiting', lean && 'lean', working && 'working', thinking && 'thinking', label && 'talk']
    .filter(Boolean)
    .join(' ');

  const style: CSSProperties = { left: x, top: y, zIndex: z };
  const vars = style as Record<string, string | number>;
  vars['--hood'] = hoodColor;
  vars['--hoodL'] = shade(hoodColor, 26);
  vars['--hoodD'] = shade(hoodColor, -34);
  vars['--torso'] = torsoColor;
  vars['--torsoL'] = shade(torsoColor, 24);
  vars['--torsoD'] = shade(torsoColor, -30);

  return (
    <div className={className} style={style} onClick={() => onDive(worker)}>
      <div className="shadow" />
      <div className="puff" />
      <div className="body">
        <div className="stack">
          <div className="hair" />
          <div className="hood" />
          <div className="phones">
            <i />
          </div>
          <div className="visor" />
          <div className="face" />
          <div className="eye" />
          <div className="torso" />
          <div className="arms" />
          <div className="book" />
          <div className="clip" />
          <div className="legs" />
          {role === 'mgr' && (
            <>
              <div className="crown" />
              <div className="mantle" />
              <div className="robe" />
            </>
          )}
        </div>
      </div>
      <div className="bubble">{label}</div>
      <div className="pop">+100 XP</div>
      <div className="think" />
    </div>
  );
}
