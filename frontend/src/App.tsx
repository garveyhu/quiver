import '@/styles/global.css';

import { Office } from '@/office/Office';
import { Caption } from '@/shell/Caption';
import { Ctrls } from '@/shell/Ctrls';
import { Hud } from '@/shell/Hud';

/** 应用根:等距办公室舞台 + 叠层 HUD/控件/状态条 + 静态画面后期(暗角/颗粒)。 */
export function App() {
  return (
    <>
      <div id="stage" className="stage">
        <Office />
      </div>
      <Hud />
      <Ctrls />
      <Caption />
      <div id="vignette" />
      <div id="grain" />
    </>
  );
}
