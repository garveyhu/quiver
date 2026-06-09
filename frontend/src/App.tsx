import '@/styles/global.css';

import { useHud } from '@/hooks/useHud';
import { Office } from '@/office/Office';
import { Atmosphere } from '@/shell/Atmosphere';
import { Caption } from '@/shell/Caption';
import { Ctrls } from '@/shell/Ctrls';
import { Hud } from '@/shell/Hud';

/** 应用根:等距办公室舞台 + 叠层 HUD/控件/状态条 + 画面后期(夜空/预算染色/暗角/颗粒)。 */
export function App() {
  const hud = useHud();

  return (
    <>
      <div id="stage" className="stage">
        <Office />
      </div>
      <Hud data={hud} />
      <Ctrls />
      <Caption />
      <Atmosphere spentUsd={hud.spentUsd} budgetCapUsd={hud.budgetCapUsd} />
      <div id="vignette" />
      <div id="grain" />
    </>
  );
}
