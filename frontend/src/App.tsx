import '@/styles/global.css';

import { useCallback, useEffect, useState } from 'react';

import { useHud } from '@/hooks/useHud';
import { Office } from '@/office/Office';
import { Atmosphere } from '@/shell/Atmosphere';
import { Caption } from '@/shell/Caption';
import { CommandPalette } from '@/shell/CommandPalette';
import type { QuiverCommand } from '@/shell/commandRegistry';
import { Ctrls } from '@/shell/Ctrls';
import { Hud } from '@/shell/Hud';

const DEFAULT_CAPTION = '你是 CEO。经理在领导区待命 —— 点「CEO 下目标」把一件事交给公司，它自己跑。';

/** 应用根:等距办公室舞台 + 叠层 HUD/控件/命令栏/状态条 + 画面后期。叠层开合与状态条文案在此编排。 */
export function App() {
  const hud = useHud();
  const [cmdkOpen, setCmdkOpen] = useState(false);
  const [caption, setCaption] = useState(DEFAULT_CAPTION);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setCmdkOpen(o => !o);
      } else if (e.key === 'Escape') {
        setCmdkOpen(false);
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  const runCommand = useCallback((cmd: QuiverCommand) => {
    setCmdkOpen(false);
    // 各命令的真实动作随对应面板/派活流程建好再接;先给状态条反馈,不留死按钮。
    setCaption(`「${cmd.label}」即将接入 —— 对应面板 / 流程建设中。`);
  }, []);

  return (
    <>
      <div id="stage" className="stage">
        <Office />
      </div>
      <Hud data={hud} />
      <Ctrls onCmdk={() => setCmdkOpen(true)} />
      <Caption text={caption} />
      <div className={`scrim${cmdkOpen ? ' on' : ''}`} onClick={() => setCmdkOpen(false)} />
      <CommandPalette open={cmdkOpen} onRun={runCommand} />
      <Atmosphere spentUsd={hud.spentUsd} budgetCapUsd={hud.budgetCapUsd} />
      <div id="vignette" />
      <div id="grain" />
    </>
  );
}
