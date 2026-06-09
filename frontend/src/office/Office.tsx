import { useMemo, type CSSProperties, type ReactNode } from 'react';

import { useWorkers } from '@/hooks/useWorkers';
import { buildScene } from '@/office/buildScene';
import type { SceneNode } from '@/office/primitives';
import { useFitScale } from '@/office/useFitScale';
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

/** 等距像素办公室。构建一次静态场景 + 初始工人，按窗口尺寸整体缩放居中。 */
export function Office() {
  const scene = useMemo(() => buildScene(), []);
  const workers = useWorkers(scene.layout);
  const scale = useFitScale(scene.layout.worldW, scene.layout.worldH);

  const worldStyle: CSSProperties = { width: scene.layout.worldW, height: scene.layout.worldH };
  (worldStyle as Record<string, string | number>)['--s'] = scale;

  return (
    <div className="scene-fit">
      <div className="world" style={worldStyle}>
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
  );
}
