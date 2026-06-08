import { useEffect, useRef, useState } from 'react';
import './shell.css';
import {
  MenuBar,
  Panel,
  Button,
  BudgetBar,
  SoundToggle,
  SegmentedControl,
  TextField,
  ProjectCard,
  StudioRoom,
  Banner,
  StatTile,
  XpBar,
  Achievement,
  ShortcutsPanel,
  Dialog,
  cssVar,
  PALETTE,
} from '@/assets';
import type { RoomWorker } from '@/assets';
import { useSupervisor } from '@/hooks/useSupervisor';
import { useSettingsContext } from './SettingsContext';
import { useTaskBoard } from '@/hooks/useTaskBoard';
import { useReplay } from '@/hooks/useReplay';
import { useStats } from '@/hooks/useStats';
import { useEnvironmentCheck } from '@/hooks/useEnvironmentCheck';
import { usePersistedState } from '@/hooks/usePersistedState';
import { useWorkers } from '@/office/useWorkers';
import { poseToState, SLOT_SKIN, STATE_LIGHT } from '@/office/poseToState';
import type { RunMode } from '@/types/run.types';
import { Transcript } from './Transcript';
import { SettingsView } from './SettingsView';
import { ArchiveView } from './ArchiveView';
import { ProjectDialog } from './ProjectDialog';
import { CommandPalette } from './CommandPalette';
import type { PaletteCommand } from './CommandPalette';
import { TaskCard } from './TaskCard';

type View = 'board' | 'archive' | 'settings';

/** 新 React 像素 UI 外壳。全窗游戏式布局:场景铺满 + 悬浮 HUD/指令栏。 */
export function QuiverShell() {
  const sup = useSupervisor();
  const { settings } = useSettingsContext();
  const board = useTaskBoard(sup.projectPath);
  const replay = useReplay();
  const { stats } = useStats();
  const health = useEnvironmentCheck();

  const [view, setView] = useState<View>('board');
  const [projOpen, setProjOpen] = useState(false);
  const [mode, setMode] = useState<RunMode>('simulate');
  const [draft, setDraft] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [sound, setSound] = usePersistedState('qv.sound', true);
  const [pendingReal, setPendingReal] = useState<string | null>(null);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
  const [railOpen, setRailOpen] = usePersistedState('qv.railOpen', true);
  const [achv, setAchv] = useState<{ title: string; sub: string } | null>(null);
  const celebrated = useRef<Set<string>>(new Set());
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const queueScrollRef = useRef<HTMLDivElement | null>(null);
  const [queueCursor, setQueueCursor] = useState(-1);
  const dragId = useRef<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  // 委托队列:进行中的活在上(排队/运行/校验/待 rebase),最近完成的在下(压暗、只读),
  // 既不会被几十条"已合并"挤成网站式长列表,也不会空着。
  const TERMINAL = ['verified', 'done', 'failed', 'verify_failed'];
  const activeTasks = board.tasks.filter((t) => !TERMINAL.includes(t.status));
  const recentDone = board.tasks.filter((t) => TERMINAL.includes(t.status)).slice(-8).reverse();
  // 给键盘处理器读最新的 view/任务/光标/board(避免 [] effect 的 stale closure)。
  const navRef = useRef({ view: 'board' as View, tasks: activeTasks, cursor: queueCursor, board });
  navRef.current = { view, tasks: activeTasks, cursor: queueCursor, board };

  useEffect(() => {
    if (settings?.defaultMode) setMode(settings.defaultMode as RunMode);
  }, [settings?.defaultMode]);

  // 键盘快捷键:⌘/Ctrl+N 钉任务、1/2/3 切 tab、Esc 关、? 快捷键。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      const typing = !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT');
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setShowPalette((s) => !s);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        setView('board');
        setTimeout(() => (document.querySelector('.qv-input') as HTMLInputElement | null)?.focus(), 0);
        return;
      }
      if (e.key === 'Escape') setShowPalette(false);
      if (typing) return;
      // vim 式队列浏览(仅看板):j 下 / k 上 / Enter 看选中任务的对话。
      const nav = navRef.current;
      if (nav.view === 'board' && nav.tasks.length) {
        if (e.key === 'j') {
          setQueueCursor((c) => Math.min((c < 0 ? -1 : c) + 1, nav.tasks.length - 1));
          return;
        }
        if (e.key === 'k') {
          setQueueCursor((c) => Math.max((c < 0 ? nav.tasks.length : c) - 1, 0));
          return;
        }
        if (e.key === 'Enter' && nav.cursor >= 0 && nav.cursor < nav.tasks.length) {
          setSelected(nav.tasks[nav.cursor].id);
          return;
        }
        // triage:J/K 重排、x 撤回(仅 queued)。
        const cur = nav.cursor >= 0 ? nav.tasks[nav.cursor] : undefined;
        if (cur?.status === 'queued') {
          if (e.key === 'J') {
            void nav.board.reorder(cur.id, cur.position + 1);
            return;
          }
          if (e.key === 'K') {
            void nav.board.reorder(cur.id, Math.max(0, cur.position - 1));
            return;
          }
          if (e.key === 'x') {
            void nav.board.cancel(cur.id);
            return;
          }
        }
      }
      if (e.key === '1') setView('board');
      else if (e.key === '2') setView('archive');
      else if (e.key === '3') setView('settings');
      else if (e.key === '?') setShowShortcuts((s) => !s);
      else if (e.key === ']' || e.key === '[') setRailOpen((r) => !r);
      else if (e.key === 'Escape') {
        setSelected(null);
        setProjOpen(false);
        setShowShortcuts(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // 任务验证通过 → 弹成就 + XP 飘字。
  useEffect(() => {
    let latest: { title: string; sub: string } | null = null;
    for (const e of sup.events) {
      if (e.kind !== 'finished') continue;
      const key = `${e.taskId}:${e.seq}`;
      if (celebrated.current.has(key)) continue;
      celebrated.current.add(key);
      if (e.status.toLowerCase() === 'verified') {
        latest = { title: '任务完成 ✓', sub: `+100 XP${e.costUsd != null ? ` · $${e.costUsd.toFixed(2)}` : ''}` };
      }
    }
    if (latest) setAchv(latest);
  }, [sup.events]);

  useEffect(() => {
    if (!achv) return;
    const t = setTimeout(() => setAchv(null), 4200);
    return () => clearTimeout(t);
  }, [achv]);

  // 回放中用回放事件喂大厅,否则用实时流。
  const liveEvents = replay.active ? replay.events : sup.events;
  const workers = useWorkers(liveEvents);

  // 预算口径与后端 §10 闸完全一致:夜间预算 vs 近 24h、月度额度 vs 近 30d(滚动窗口)。
  const spentDay = stats?.spentDay ?? 0;
  const spentMonth = stats?.spentMonth ?? 0;
  const nightlyCap = settings?.nightlyBudgetUsd ?? 0;
  const monthlyCap = settings?.monthlyCreditCapUsd ?? 0;
  const budgetPaused = (nightlyCap > 0 && spentDay >= nightlyCap) || (monthlyCap > 0 && spentMonth >= monthlyCap);
  const barTotal = nightlyCap > 0 ? nightlyCap : monthlyCap;
  const barSpent = nightlyCap > 0 ? spentDay : spentMonth;
  const running = board.tasks.filter((t) => t.status === 'running' || t.status === 'verifying').length;
  const queued = board.tasks.filter((t) => t.status === 'queued').length;
  const projectName =
    sup.recentProjects.find((r) => r.path === sup.projectPath)?.alias ??
    sup.projectPath?.split('/').pop() ??
    '';
  const healthFail = health.checks.find((c) => c.status === 'fail');

  // 传最近 6 个工人视图;StudioRoom 按容器宽决定显示几个工位 + 溢出提示。
  const roomWorkers: RoomWorker[] = workers.slice(-6).map((w) => ({
    state: poseToState[w.pose],
    light: STATE_LIGHT[poseToState[w.pose]],
    taskId: w.taskId,
    bubble: w.bubble,
    ...SLOT_SKIN[w.slot],
  }));

  // 没手动选中时,默认跟随"当前正在干活"的工人。
  const activeWorker = [...workers].reverse().find((w) => !w.terminal);
  const displayTaskId = selected ?? activeWorker?.taskId ?? null;
  const selEvents = liveEvents.filter((e) => e.taskId === displayTaskId);
  const selTask = board.tasks.find((t) => t.id === displayTaskId);

  useEffect(() => {
    const el = transcriptRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [selEvents.length, displayTaskId]);

  // vim 光标移动时,把当前行滚入视野。
  useEffect(() => {
    if (queueCursor < 0) return;
    const row = queueScrollRef.current?.children[queueCursor] as HTMLElement | undefined;
    row?.scrollIntoView({ block: 'nearest' });
  }, [queueCursor]);

  const submit = () => {
    const p = draft.trim();
    if (!p) return;
    if (mode === 'real') {
      setPendingReal(p);
      return;
    }
    void board.enqueue(p, mode);
    setDraft('');
  };
  const confirmReal = () => {
    if (pendingReal) {
      void board.enqueue(pendingReal, 'real');
      setDraft('');
    }
    setPendingReal(null);
  };

  // ⌘K 命令面板的命令集 + 派活回调。
  const paletteCommands: PaletteCommand[] = [
    { id: 'nav-board', label: '前往 · 看板', keywords: 'board kanban', run: () => { setView('board'); setShowPalette(false); } },
    { id: 'nav-archive', label: '前往 · 档案', keywords: 'archive history', run: () => { setView('archive'); setShowPalette(false); } },
    { id: 'nav-settings', label: '前往 · 设置', keywords: 'settings config', run: () => { setView('settings'); setShowPalette(false); } },
    { id: 'open-projects', label: '项目 · 切换 / 添加', keywords: 'project switch add', run: () => { setProjOpen(true); setShowPalette(false); } },
    { id: 'toggle-mode', label: `默认模式 → ${mode === 'real' ? '模拟' : '真实'}`, keywords: 'mode 模式 simulate real 模拟 真实', run: () => { setMode((m) => (m === 'real' ? 'simulate' : 'real')); setShowPalette(false); } },
    { id: 'recheck', label: '重新工坊体检', keywords: 'health check 体检 环境', run: () => { void health.refresh(); setShowPalette(false); } },
    { id: 'shortcuts', label: '查看快捷键', keywords: 'help keys 帮助 快捷键', run: () => { setShowShortcuts(true); setShowPalette(false); } },
    ...(replay.active
      ? [{ id: 'stop-replay', label: '停止回放', keywords: 'replay stop 回放', run: () => { replay.stop(); setShowPalette(false); } } as PaletteCommand]
      : []),
    ...Array.from(new Set([...board.tasks].reverse().map((t) => t.prompt)))
      .slice(0, 5)
      .map(
        (p): PaletteCommand => ({
          id: 'rerun-' + p.slice(0, 24),
          label: '再派一次 · ' + p,
          keywords: p,
          run: () => {
            setShowPalette(false);
            if (mode === 'real') setPendingReal(p);
            else void board.enqueue(p, 'simulate');
          },
        }),
      ),
    ...sup.recentProjects.map(
      (r): PaletteCommand => ({
        id: 'proj-' + r.path,
        label: '切到项目 · ' + (r.alias ?? r.path.split('/').pop() ?? r.path),
        keywords: r.path,
        run: () => { void sup.selectRecentProject(r.path); setShowPalette(false); },
      }),
    ),
  ];
  const paletteEnqueue = (text: string) => {
    setShowPalette(false);
    if (mode === 'real') setPendingReal(text);
    else void board.enqueue(text, 'simulate');
  };

  const shell = {
    position: 'fixed',
    inset: 0,
    display: 'flex',
    flexDirection: 'column',
    background: PALETTE.night2,
    overflow: 'hidden',
  } as const;

  // —— 首次启动:选项目(居中)——
  if (!sup.projectPath) {
    return (
      <div style={{ ...shell, alignItems: 'center', justifyContent: 'center' }}>
        <Panel title="QUIVER" width={420}>
          <div style={{ textAlign: 'center', padding: '10px 6px' }}>
            <div style={{ color: cssVar('amber'), fontSize: 22, fontWeight: 'bold', letterSpacing: 2 }}>QUIVER</div>
            <div style={{ color: PALETTE.dim, fontSize: 12, margin: '8px 0 16px' }}>
              选一个 git 仓库当工坊,智能体就在里面接单干活
            </div>
            <Button onClick={() => void sup.pickProject()}>选择项目</Button>
          </div>
          {sup.recentProjects.length > 0 && (
            <div style={{ marginTop: 14, display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ color: PALETTE.dim, fontSize: 11 }}>最近项目</div>
              {sup.recentProjects.map((r) => (
                <ProjectCard
                  key={r.path}
                  name={r.alias ?? r.path.split('/').pop() ?? r.path}
                  path={r.path}
                  onClick={() => void sup.selectRecentProject(r.path)}
                />
              ))}
            </div>
          )}
        </Panel>
      </div>
    );
  }

  return (
    <div style={shell}>
      <MenuBar
        brand={projectName ? `QUIVER · ${projectName}` : 'QUIVER'}
        items={[
          { label: '看板', active: view === 'board', onClick: () => setView('board') },
          { label: '档案', active: view === 'archive', onClick: () => setView('archive') },
          { label: '设置', active: view === 'settings', onClick: () => setView('settings') },
          { label: '项目', onClick: () => setProjOpen(true) },
        ]}
        right={
          <>
            {replay.active && (
              <Button variant="ghost" onClick={() => replay.stop()}>
                停止回放
              </Button>
            )}
            <span
              onClick={() => setShowPalette(true)}
              title="命令面板"
              style={{ cursor: 'pointer', fontSize: 10, color: cssVar('dim'), boxShadow: 'inset 0 0 0 1px ' + PALETTE.wall, padding: '3px 7px', letterSpacing: 1 }}
            >
              ⌘K
            </span>
            <span
              onClick={() => setRailOpen((r) => !r)}
              title="折叠/展开右栏 ( ] )"
              style={{ cursor: 'pointer', fontSize: 12, color: railOpen ? cssVar('amber') : cssVar('dim'), boxShadow: 'inset 0 0 0 1px ' + PALETTE.wall, padding: '2px 7px' }}
            >
              ▤
            </span>
            <SoundToggle on={sound} onChange={setSound} />
          </>
        }
      />

      {/* —— 内容区:稳定的世界 + 悬浮 UI —— */}
      <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
        {/* 稳定世界:全幅场景始终常驻(UI 悬浮其上、不挤压它,切视图也不重挂载/重排) */}
        <div className="qv-stage" style={{ position: 'absolute', inset: 0, overflow: 'hidden' }}>
          <StudioRoom workers={roomWorkers} onWorkerClick={(id) => setSelected(id)} focusedTaskId={selected ?? undefined} />

          {/* HUD 仪表:左上悬浮 */}
          <div style={{ position: 'absolute', top: 10, left: 10, display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap', maxWidth: 'calc(100% - 20px)', zIndex: 6 }}>
            <StatTile value={running} label="运行中" />
            <StatTile value={queued} label="排队" />
            <StatTile value={`$${spentDay.toFixed(2)}`} label="今夜花费" color={budgetPaused ? PALETTE.red : PALETTE.green} />
            {stats && <StatTile value={stats.verified} label="已完成" color={PALETTE.green} />}
            {stats && (
              <XpBar
                xp={stats.xp - (stats.level - 1) ** 2 * 100}
                max={stats.level ** 2 * 100 - (stats.level - 1) ** 2 * 100}
                level={stats.level}
              />
            )}
            {barTotal > 0 && <BudgetBar spent={barSpent} total={barTotal} width={120} />}
          </div>

          {/* 横幅:顶部居中悬浮(仅告警时) */}
          {(healthFail || budgetPaused) && (
            <div style={{ position: 'absolute', top: 58, left: '50%', transform: 'translateX(-50%)', width: 'min(560px, calc(100% - 24px))', display: 'flex', flexDirection: 'column', gap: 8, zIndex: 7 }}>
              {budgetPaused && (
                <Banner kind="danger" action={<Button variant="ghost" onClick={() => setView('settings')}>提高上限</Button>}>
                  预算到顶 — 队列已暂停,直到你提高上限 / 月度额度刷新
                </Banner>
              )}
              {healthFail && (
                <Banner kind="warn" action={<Button variant="ghost" onClick={() => setView('settings')}>去体检</Button>}>
                  工坊体检:{healthFail.label} — {healthFail.message}
                </Banner>
              )}
            </div>
          )}

          {/* 右栏 Rail:悬浮在世界上(不挤压场景),] 折叠 */}
          {railOpen && (
            <div style={{ position: 'absolute', top: 10, right: 10, bottom: 76, width: 300, display: 'flex', flexDirection: 'column', gap: 8, zIndex: 5 }}>
              <Panel title={activeTasks.length ? `委托队列 · 进行 ${activeTasks.length}` : '委托队列'} width={300} bodyPadding={0}>
                <div ref={queueScrollRef} className="qv-scroll" style={{ maxHeight: displayTaskId ? '32vh' : '74vh', overflowY: 'auto' }}>
                  {activeTasks.length === 0 && recentDone.length === 0 && (
                    <div style={{ padding: 14, color: PALETTE.dim, fontSize: 11 }}>工坊静悄悄 · 在下面派个活(或 ⌘K)</div>
                  )}
                  {activeTasks.map((t, i) => (
                    <TaskCard
                      key={t.id}
                      task={t}
                      selected={selected === t.id}
                      cursored={i === queueCursor}
                      onClick={() => { setSelected(t.id); setQueueCursor(i); }}
                      onCancel={['queued', 'running', 'verifying'].includes(t.status) ? () => void board.cancel(t.id) : undefined}
                      draggable={t.status === 'queued'}
                      onDragStart={() => { dragId.current = t.id; }}
                      onDragOver={(e) => {
                        if (t.status === 'queued' && dragId.current && dragId.current !== t.id) {
                          e.preventDefault();
                          setDropTarget(t.id);
                        }
                      }}
                      onDrop={(e) => {
                        e.preventDefault();
                        const from = dragId.current;
                        if (from && from !== t.id) void board.reorder(from, t.position);
                        dragId.current = null;
                        setDropTarget(null);
                      }}
                      onDragEnd={() => { dragId.current = null; setDropTarget(null); }}
                      dropHint={dropTarget === t.id}
                    />
                  ))}
                  {recentDone.length > 0 && (
                    <div style={{ padding: '7px 9px 4px', fontSize: 9, letterSpacing: 1, color: PALETTE.dim2, borderTop: activeTasks.length ? '1px solid rgba(255,255,255,0.06)' : undefined, marginTop: activeTasks.length ? 4 : 0 }}>
                      最近完成
                    </div>
                  )}
                  {recentDone.map((t) => (
                    <TaskCard key={t.id} task={t} selected={selected === t.id} dimmed onClick={() => setSelected(t.id)} />
                  ))}
                </div>
              </Panel>

              {displayTaskId && (
                <Panel
                  title={selected ? '工位详情 · 对话 ▤' : '实时 · 对话 ▤'}
                  width={300}
                  onClose={selected ? () => setSelected(null) : undefined}
                  bodyPadding={0}
                >
                  <div ref={transcriptRef} className="qv-scroll" style={{ maxHeight: '40vh', overflowY: 'auto' }}>
                    <Transcript events={selEvents} prompt={selTask?.prompt} mode={selTask?.mode} branch={selTask?.branch} />
                  </div>
                </Panel>
              )}
            </div>
          )}

          {/* 底坞 Dock:底部悬浮指令栏(让出右栏宽度) */}
          <div style={{ position: 'absolute', bottom: 12, left: 12, right: railOpen ? 322 : 12, display: 'flex', gap: 8, alignItems: 'center', background: PALETTE.night, padding: 8, boxShadow: 'inset 0 0 0 2px ' + PALETTE.wall + ', 0 6px 18px #00000055', zIndex: 6 }}>
            <SegmentedControl options={['模拟', '真实']} value={mode === 'real' ? 1 : 0} onChange={(i) => setMode(i === 1 ? 'real' : 'simulate')} />
            <TextField
              placeholder="给工人派个活,比如:读 README 用一句话总结这个项目"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submit()}
              style={{ flex: 1 }}
            />
            <Button onClick={submit}>钉上去</Button>
          </div>
        </div>

        {/* 设置 / 档案:覆盖在世界之上(世界在底下保持稳定,不重排不重挂载) */}
        {view === 'settings' && (
          <div className="qv-scroll" style={{ position: 'absolute', inset: 0, background: PALETTE.night2, overflowY: 'auto', display: 'flex', justifyContent: 'center', padding: 20, zIndex: 10 }}>
            <SettingsView />
          </div>
        )}
        {view === 'archive' && (
          <div className="qv-scroll" style={{ position: 'absolute', inset: 0, background: PALETTE.night2, overflowY: 'auto', display: 'flex', justifyContent: 'center', padding: 20, zIndex: 10 }}>
            <ArchiveView onReplay={(evs) => { replay.start(evs); setView('board'); }} />
          </div>
        )}
      </div>

      {sup.error && (
        <div style={{ position: 'absolute', bottom: 8, left: 12, color: PALETTE.red, fontSize: 11, zIndex: 20 }}>{sup.error}</div>
      )}

      {achv && (
        <div style={{ position: 'fixed', right: 18, top: 70, zIndex: 60 }}>
          <Achievement title={achv.title} sub={achv.sub} />
        </div>
      )}

      {showPalette && (
        <CommandPalette
          onClose={() => setShowPalette(false)}
          onEnqueue={paletteEnqueue}
          mode={mode}
          onToggleMode={() => setMode((m) => (m === 'real' ? 'simulate' : 'real'))}
          commands={paletteCommands}
        />
      )}

      {showShortcuts && (
        <div className="qv-backdrop" onClick={() => setShowShortcuts(false)} style={{ position: 'fixed', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 70 }}>
          <div onClick={(e) => e.stopPropagation()}>
            <ShortcutsPanel
              onClose={() => setShowShortcuts(false)}
              items={[
                { label: '命令面板(派活/跳转/切项目)', keys: ['⌘', 'K'] },
                { label: '钉一条委托', keys: ['⌘', 'N'] },
                { label: '看板', keys: ['1'] },
                { label: '档案', keys: ['2'] },
                { label: '设置', keys: ['3'] },
                { label: '队列上下移动光标', keys: ['j', 'k'] },
                { label: '看选中任务对话', keys: ['↵'] },
                { label: '队列重排(上/下移)', keys: ['K', 'J'] },
                { label: '撤回待接任务', keys: ['x'] },
                { label: '折叠/展开右栏', keys: [']'] },
                { label: '快捷键', keys: ['?'] },
                { label: '关闭 / 取消', keys: ['Esc'] },
              ]}
            />
          </div>
        </div>
      )}

      <ProjectDialog
        open={projOpen}
        onClose={() => setProjOpen(false)}
        projectPath={sup.projectPath}
        onPick={sup.pickProject}
        onSelect={sup.selectRecentProject}
      />

      <Dialog
        open={pendingReal !== null}
        title="真实运行确认 ⚠"
        onClose={() => setPendingReal(null)}
        width={360}
        actions={
          <>
            <Button variant="ghost" onClick={() => setPendingReal(null)}>取消</Button>
            <Button variant="danger" onClick={confirmReal}>确认开工</Button>
          </>
        }
      >
        <div style={{ fontSize: 11, color: PALETTE.dim, lineHeight: 1.5 }}>
          真实模式会用真 Claude 在你的仓库读写文件、执行命令,
          <b style={{ color: PALETTE.amber }}>消耗你的月度额度</b>。产物只留在分支、不并入 main。
        </div>
      </Dialog>
    </div>
  );
}
