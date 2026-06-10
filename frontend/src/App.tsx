import '@/styles/global.css';

import { useCallback, useEffect, useRef, useState } from 'react';

import { useCompletionFx } from '@/hooks/useCompletionFx';
import { useEpisodes } from '@/hooks/useEpisodes';
import { useHud } from '@/hooks/useHud';
import { useManagerDesk } from '@/hooks/useManagerDesk';
import { useMetrics } from '@/hooks/useMetrics';
import { useMorningReport } from '@/hooks/useMorningReport';
import { useRoles } from '@/hooks/useRoles';
import { useSettings } from '@/hooks/useSettings';
import { Office } from '@/office/Office';
import { cancelActiveTasks, enqueueTask, getInitialState, requeueTask } from '@/services/commands';
import { Atmosphere } from '@/shell/Atmosphere';
import { BriefCard } from '@/shell/BriefCard';
import { Caption } from '@/shell/Caption';
import { CommandPalette } from '@/shell/CommandPalette';
import type { QuiverCommand } from '@/shell/commandRegistry';
import { Ctrls } from '@/shell/Ctrls';
import { Hud } from '@/shell/Hud';
import { ManagerDesk } from '@/shell/ManagerDesk';
import { MorningReport } from '@/shell/MorningReport';
import { PersonnelDesk } from '@/shell/PersonnelDesk';
import { Timeline } from '@/shell/Timeline';
import { TrustCard } from '@/shell/TrustCard';

const DEFAULT_CAPTION = '你是 CEO。经理在领导区待命 —— 点「CEO 下目标」把一件事交给公司，它自己跑。';

/** 派给公司的示例目标(轮流取),移植原型 GOALS。 */
const GOALS = ['做转写的导出功能', '修登录态丢失', '给看板加暗色模式', '把召回准确率提上去', '补端到端测试', '清掉废弃依赖'];

/** 当前打开的叠层(同一时刻至多一个)。 */
type Overlay = 'none' | 'cmdk' | 'report' | 'brief' | 'trust' | 'timeline' | 'manager' | 'personnel';

/** 应用根:等距办公室舞台 + 叠层 + 画面后期。叠层开合、派活握手、状态条文案在此编排。 */
export function App() {
  const hud = useHud();
  const settings = useSettings();
  const budgetCap = settings?.nightlyBudgetUsd ?? null;
  const [overlay, setOverlay] = useState<Overlay>('none');
  const [caption, setCaption] = useState(DEFAULT_CAPTION);
  const [frozen, setFrozen] = useState(false);
  const [briefGoal, setBriefGoal] = useState(GOALS[0]);
  const report = useMorningReport(overlay === 'report');
  const metrics = useMetrics(overlay === 'trust');
  const episodes = useEpisodes(overlay === 'timeline');
  // 经理升级给人 → 状态条立刻喊人(§12「等你拍板」可感);M 打开工作台看详情。
  const onEscalate = useCallback((reason: string) => {
    setCaption(`⚠ 经理升级等你拍板:${reason} —— 按 M 打开经理工作台处理。`);
  }, []);
  const managerDesk = useManagerDesk(overlay === 'manager', onEscalate);
  const personnel = useRoles(overlay === 'personnel');
  const completionFx = useCompletionFx();

  // §12 一站式打回:晨报/工作台共用 —— 把卡住/升级的任务作为新任务重派(经理带记忆)。
  const onRequeue = useCallback((taskId: string) => {
    void requeueTask(taskId)
      .then(t => setCaption(`已打回重做 —— 新任务 ${t.id} 入队,经理会带着上次的记忆再派。`))
      .catch(e => setCaption(`打回失败:${e instanceof Error ? e.message : String(e)}`));
  }, []);
  const goalIdx = useRef(0);

  // 交付完成 → 状态条同步报喜/报忧(脉冲叠层在下方渲染)。
  useEffect(() => {
    if (completionFx === 'ok') setCaption('审计通过 → 发货 · main 是绿的。');
    else if (completionFx === 'bad') setCaption('审计打回 — 留分支等你决定，绝不自动改 main。');
  }, [completionFx]);

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
      } else if (e.key.toLowerCase() === 'm' && !e.metaKey && !e.ctrlKey && !e.altKey) {
        // 仅在没有叠层打开时切「经理工作台」—— 避免和 Brief 输入框里打 m 冲突。
        setOverlay(o => (o === 'none' ? 'manager' : o === 'manager' ? 'none' : o));
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  const openBrief = useCallback(() => {
    setBriefGoal(GOALS[goalIdx.current++ % GOALS.length]);
    setOverlay('brief');
  }, []);

  // 支持一次下一批:每行一个目标 → 全部入队,公司按并发持续消化("一直跑下去")。单行兼容。
  const enqueueGoal = useCallback(async (goalText: string) => {
    const goals = goalText
      .split('\n')
      .map(g => g.trim())
      .filter(Boolean);
    if (goals.length === 0) return;
    let ok = 0;
    let lastErr = '';
    for (const g of goals) {
      try {
        await enqueueTask(g, 'simulate');
        ok += 1;
      } catch (e) {
        lastErr = e instanceof Error ? e.message : String(e); // 不静默吞
      }
    }
    if (ok === goals.length) {
      setCaption(
        goals.length === 1
          ? `CEO → 经理：「${goals[0]}」—— 已派给公司（simulate，免费跑）。`
          : `CEO → 经理：一批 ${ok} 件事已派 —— 公司会按并发持续消化。`,
      );
    } else {
      setCaption(`派活:${ok}/${goals.length} 成功${lastErr ? ` —— ${lastErr}` : ''}`);
    }
  }, []);

  // 一次下一批目标(§3 协作):经理按并发上限(人事部 maxWorkers)并行调动多个员工。
  // 上限=1 时仍串行 —— 把人事部并发调高才看得到多个工位同时敲键。
  const enqueueBatch = useCallback(async () => {
    const goals = Array.from({ length: 3 }, () => GOALS[goalIdx.current++ % GOALS.length]);
    let ok = 0;
    let lastErr = '';
    for (const g of goals) {
      try {
        await enqueueTask(g, 'simulate');
        ok += 1;
      } catch (e) {
        lastErr = e instanceof Error ? e.message : String(e); // 不静默吞:报真成败,绝不假报"已派"
      }
    }
    setCaption(
      ok === goals.length
        ? `CEO → 经理：一批 ${ok} 件事已派 —— 经理按并发上限并行调度(上限=1 则串行)。`
        : `派活失败:${ok}/${goals.length} 成功${lastErr ? ` —— ${lastErr}` : ''}`,
    );
  }, []);

  const confirmBrief = useCallback(
    (goal: string) => {
      setOverlay('none');
      void enqueueGoal(goal);
    },
    [enqueueGoal],
  );

  const estop = useCallback(async () => {
    setOverlay('none');
    setFrozen(true);
    window.setTimeout(() => setFrozen(false), 2200);
    try {
      const n = await cancelActiveTasks();
      setCaption(`急停 — 全公司冻结，main 安全。已收回 ${n} 个在途委托的工具权。`);
    } catch (e) {
      setCaption(`急停遇到问题:${e instanceof Error ? e.message : String(e)}`);
    }
  }, []);

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
      if (cmd.id === 'manager') {
        setOverlay('manager');
        return;
      }
      if (cmd.id === 'personnel') {
        setOverlay('personnel');
        return;
      }
      if (cmd.id === 'estop') {
        void estop();
        return;
      }
      setOverlay('none');
      if (cmd.id === 'dispatch') {
        // 「派活」走快通道:不开 Brief,直接让经理开一个示例任务。
        void enqueueGoal(GOALS[goalIdx.current++ % GOALS.length]);
        return;
      }
      if (cmd.id === 'dispatch-batch') {
        void enqueueBatch();
        return;
      }
      // 其余命令的真实动作随对应面板/流程建好再接;先给状态条反馈,不留死按钮。
      setCaption(`「${cmd.label}」即将接入 —— 对应面板 / 流程建设中。`);
    },
    [openBrief, enqueueGoal, enqueueBatch, estop],
  );

  return (
    <>
      <div id="stage" className={`stage${frozen ? ' frozen' : ''}`}>
        <Office completionFx={completionFx} />
      </div>
      <Hud data={hud} budgetCap={budgetCap} />
      <Ctrls
        onCmdk={() => setOverlay('cmdk')}
        onReport={() => setOverlay('report')}
        onGoal={openBrief}
        onManager={() => setOverlay('manager')}
        onPersonnel={() => setOverlay('personnel')}
      />
      <Caption text={caption} />
      <div className={`scrim${overlay !== 'none' ? ' on' : ''}`} onClick={() => setOverlay('none')} />
      <CommandPalette open={overlay === 'cmdk'} onRun={runCommand} />
      <MorningReport open={overlay === 'report'} data={report} onRequeue={onRequeue} onClose={() => setOverlay('none')} />
      <BriefCard open={overlay === 'brief'} defaultGoal={briefGoal} onClose={() => setOverlay('none')} onConfirm={confirmBrief} />
      <TrustCard open={overlay === 'trust'} metrics={metrics} onClose={() => setOverlay('none')} />
      <Timeline open={overlay === 'timeline'} episodes={episodes} onClose={() => setOverlay('none')} />
      <ManagerDesk
        open={overlay === 'manager'}
        decisions={managerDesk.decisions}
        preview={managerDesk.preview}
        brief={managerDesk.brief}
        autonomous={managerDesk.autonomous}
        onToggleAutonomous={managerDesk.toggleAutonomous}
        onAddFact={(text, topic) =>
          void managerDesk
            .addFact(text, topic)
            .then(() => setCaption(`已录入权威事实:「${text}」${topic ? `(主题:${topic},同主题旧事实已作废)` : ''} —— 进了经理简报。`))
            .catch(e => setCaption(`录入失败:${e instanceof Error ? e.message : String(e)}`))
        }
        onRequeue={onRequeue}
        onClose={() => setOverlay('none')}
      />
      <PersonnelDesk
        open={overlay === 'personnel'}
        roles={personnel.roles}
        onPatch={(id, patch) => void personnel.patchRole(id, patch).catch(() => {})}
        onHire={() => void personnel.hire().catch(e => setCaption(`雇人失败:${e instanceof Error ? e.message : String(e)}`))}
        onFire={id => void personnel.fire(id).catch(e => setCaption(`裁员失败:${e instanceof Error ? e.message : String(e)}`))}
        onClose={() => setOverlay('none')}
      />
      <Atmosphere spentUsd={hud.spentUsd} budgetCapUsd={budgetCap} />
      <div id="flashfx" className={completionFx ?? ''} />
      <div id="vignette" />
      <div id="grain" />
    </>
  );
}
