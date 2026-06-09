import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';

import type { CompletionFx } from '@/hooks/useCompletionFx';
import { useTaskEvents } from '@/hooks/useTaskEvents';
import { useWorkers } from '@/hooks/useWorkers';
import { buildScene } from '@/office/buildScene';
import type { SceneNode } from '@/office/primitives';
import { useCamera } from '@/office/useCamera';
import { Worker } from '@/office/Worker';
import { WorldFx } from '@/office/WorldFx';
import { Worksurf } from '@/office/Worksurf';
import type { PlacedWorker } from '@/office/workers';

/** 节点内部内容:复合结构(猫) > 可扩展"＋" > 标签文字 > 空。 */
function nodeChildren(node: SceneNode): ReactNode {
  if (node.composite === 'cat') {
    return (
      <>
        <div className="t" />
        <div className="b">
          <div className="e1" />
          <div className="e2" />
        </div>
      </>
    );
  }
  if (node.mark) return <b>＋</b>;
  return node.text;
}

interface OfficeProps {
  /** 交付时刻特效(验收通过/打回),在世界内质检台/发货口播放 */
  completionFx: CompletionFx;
}

/** 等距像素办公室。静态场景 + 实时工人 + 滚轮缩放 + 点小人下钻聚焦 + 交付特效。 */
export function Office({ completionFx }: OfficeProps) {
  const scene = useMemo(() => buildScene(), []);
  const workers = useWorkers(scene.layout);
  const camera = useCamera(scene.layout.worldW, scene.layout.worldH);
  const [focused, setFocused] = useState<PlacedWorker | null>(null);
  const events = useTaskEvents(focused?.taskId ?? null);

  const dive = useCallback(
    (w: PlacedWorker) => {
      setFocused(w);
      camera.diveTo(w.x, w.y);
    },
    [camera],
  );

  const closeDive = useCallback(() => {
    setFocused(null);
    camera.reset();
  }, [camera]);

  // Esc 退出下钻(相机由 useCamera 自身的 Esc 复位,这里清掉 worksurf)。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setFocused(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <>
      <div className="scene-fit">
        <div className="world" style={camera.worldStyle}>
          {scene.nodes.map(node => (
            <div key={node.key} className={node.className} style={node.style}>
              {nodeChildren(node)}
            </div>
          ))}
          {workers.map(w => (
            <Worker key={w.id} worker={w} onDive={dive} />
          ))}
          <WorldFx layout={scene.layout} fx={completionFx} />
        </div>
      </div>
      <div className={`zoomhint${camera.zoomed || focused ? ' on' : ''}`}>滚轮缩放 · Esc / 双击 复位</div>
      <Worksurf worker={focused} events={events} onClose={closeDive} />
    </>
  );
}
