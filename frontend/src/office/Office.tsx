import { useMemo, type ReactNode } from 'react';

import { useWorkers } from '@/hooks/useWorkers';
import { buildScene } from '@/office/buildScene';
import type { SceneNode } from '@/office/primitives';
import { useCamera } from '@/office/useCamera';
import { Worker } from '@/office/Worker';

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

/** 等距像素办公室。构建一次静态场景 + 实时工人，滚轮连续缩放、按窗口 fit 居中。 */
export function Office() {
  const scene = useMemo(() => buildScene(), []);
  const workers = useWorkers(scene.layout);
  const camera = useCamera(scene.layout.worldW, scene.layout.worldH);

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
            <Worker key={w.id} worker={w} />
          ))}
        </div>
      </div>
      <div className={`zoomhint${camera.zoomed ? ' on' : ''}`}>滚轮缩放 · Esc / 双击 复位</div>
    </>
  );
}
