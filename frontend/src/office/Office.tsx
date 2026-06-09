import { useMemo, type CSSProperties } from 'react';

import { buildScene } from '@/office/buildScene';
import { useFitScale } from '@/office/useFitScale';

/** 等距像素办公室。构建一次静态场景，按窗口尺寸整体缩放居中。 */
export function Office() {
  const scene = useMemo(() => buildScene(), []);
  const scale = useFitScale(scene.layout.worldW, scene.layout.worldH);

  const worldStyle: CSSProperties = { width: scene.layout.worldW, height: scene.layout.worldH };
  (worldStyle as Record<string, string | number>)['--s'] = scale;

  return (
    <div className="scene-fit">
      <div className="world" style={worldStyle}>
        {scene.nodes.map(node => (
          <div key={node.key} className={node.className} style={node.style}>
            {node.mark ? <b>＋</b> : node.text}
          </div>
        ))}
      </div>
    </div>
  );
}
