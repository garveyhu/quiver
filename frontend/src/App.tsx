import '@/styles/global.css';

import { useCallback, useEffect, useRef, useState } from 'react';

import { useEpisodes } from '@/hooks/useEpisodes';
import { useHud } from '@/hooks/useHud';
import { useMetrics } from '@/hooks/useMetrics';
import { useMorningReport } from '@/hooks/useMorningReport';
import { useSettings } from '@/hooks/useSettings';
import { Office } from '@/office/Office';
import { enqueueTask, getInitialState } from '@/services/commands';
import { Atmosphere } from '@/shell/Atmosphere';
import { BriefCard } from '@/shell/BriefCard';
import { Caption } from '@/shell/Caption';
import { CommandPalette } from '@/shell/CommandPalette';
import type { QuiverCommand } from '@/shell/commandRegistry';
import { Ctrls } from '@/shell/Ctrls';
import { Hud } from '@/shell/Hud';
import { MorningReport } from '@/shell/MorningReport';
import { Timeline } from '@/shell/Timeline';
import { TrustCard } from '@/shell/TrustCard';

const DEFAULT_CAPTION = '你是 CEO。经理在领导区待命 —— 点「CEO 下目标」把一件事交给公司，它自己跑。';

/** 派给公司的示例目标(轮流取),移植原型 GOALS。 */
const GOALS = ['做转写的导出功能', '修登录态丢失', '给看板加暗色模式', '把召回准确率提上去', '补端到端测试', '清掉废弃依赖'];

/** 当前打开的叠层(同一时刻至多一个)。 */
type Overlay = 'none' | 'cmdk' | 'report' | 'brief' | 'trust' | 'timeline';

/** 应用根:等距办公室舞台 + 叠层 + 画面后期。叠层开合、派活握手、状态条文案在此编排。 */
export function App() {
  const hud = useHud();
  const settings = useSettings();
  const budgetCap = settings?.nightlyBudgetUsd ?? null;
  const [overlay, setOverlay] = useState<Overlay>('none');
  const [caption, setCaption] = useState(DEFAULT_CAPTION);
  const [briefGoal, setBriefGoal] = useState(GOALS[0]);
  const report = useMorningReport(overlay === 'report');
  const metrics = useMetrics(overlay === 'trust');
  const episodes = useEpisodes(overlay === 'timeline');
  const goalIdx = useRef(0);

  // 启动读启动包:副作用是让后端设好当前项目,派活才有目标仓库。
  useEffect(() => {
    void getInitialState().catch(() => {});
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setOverlay(o => (o === 'cmdk' ? 'none' : 'cmdk'));
      } else if (e.key === 'Escape') {
        setOverlay('none');
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  const openBrief = useCallback(() => {
    setBriefGoal(GOALS[goalIdx.current++ % GOALS.length]);
    setOverlay('brief');
  }, []);

  const enqueueGoal = useCallback(async (goal: string) => {
    setCaption(`CEO → 经理：「${goal}」—— 已派给公司（simulate，免费跑）。`);
    try {
      await enqueueTask(goal, 'simulate');
    } catch (e) {
      setCaption(`派活失败:${e instanceof Error ? e.message : String(e)}`);
    }
  }, []);

  const confirmBrief = useCallback(
    (goal: string) => {
      setOverlay('none');
      void enqueueGoal(goal);
    },
    [enqueueGoal],
  );

  const runCommand = useCallback(
    (cmd: QuiverCommand) => {
      if (cmd.id === 'report') {
        setOverlay('report');
        return;
      }
      if (cmd.id === 'new-task') {
        openBrief();
        return;
      }
      if (cmd.id === 'trust') {
        setOverlay('trust');
        return;
      }
      if (cmd.id === 'timeline') {
        setOverlay('timeline');
        return;
      }
      setOverlay('none');
      if (cmd.id === 'dispatch') {
        // 「派活」走快通道:不开 Brief,直接让经理开一个示例任务。
        void enqueueGoal(GOALS[goalIdx.current++ % GOALS.length]);
        return;
      }
      // 其余命令的真实动作随对应面板/流程建好再接;先给状态条反馈,不留死按钮。
      setCaption(`「${cmd.label}」即将接入 —— 对应面板 / 流程建设中。`);
    },
    [openBrief, enqueueGoal],
  );

  return (
    <>
      <div id="stage" className="stage">
        <Office />
      </div>
      <Hud data={hud} budgetCap={budgetCap} />
      <Ctrls onCmdk={() => setOverlay('cmdk')} onReport={() => setOverlay('report')} onGoal={openBrief} />
      <Caption text={caption} />
      <div className={`scrim${overlay !== 'none' ? ' on' : ''}`} onClick={() => setOverlay('none')} />
      <CommandPalette open={overlay === 'cmdk'} onRun={runCommand} />
      <MorningReport open={overlay === 'report'} data={report} onClose={() => setOverlay('none')} />
      <BriefCard open={overlay === 'brief'} defaultGoal={briefGoal} onClose={() => setOverlay('none')} onConfirm={confirmBrief} />
      <TrustCard open={overlay === 'trust'} metrics={metrics} onClose={() => setOverlay('none')} />
      <Timeline open={overlay === 'timeline'} episodes={episodes} onClose={() => setOverlay('none')} />
      <Atmosphere spentUsd={hud.spentUsd} budgetCapUsd={budgetCap} />
      <div id="vignette" />
      <div id="grain" />
    </>
  );
}
