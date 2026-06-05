import { useEffect, useMemo, useRef } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { useSettings } from '@/hooks/useSettings';
import { useTaskBoard } from '@/hooks/useTaskBoard';
import { useArchive } from '@/hooks/useArchive';
import { useProjects } from '@/hooks/useProjects';
import { useReplay } from '@/hooks/useReplay';
import { useEnvironmentCheck } from '@/hooks/useEnvironmentCheck';
import {
  EventBus,
  BUS,
  type TaskBoardSummary,
  type BoardEnqueuePayload,
  type BoardReorderPayload,
  type BoardCancelPayload,
  type LogbookOpen,
  type LogbookState,
  type ProjectsState,
  type ProjectSelectPayload,
  type ProjectRemovePayload,
  type ProjectAliasPayload,
} from '@/game/EventBus';
import { logbookTotals } from '@/game/logbookState';
import type { RunMode } from '@/types/run.types';
import type { SettingsPatch, StoredEvent } from '@/types/persistence.types';

/**
 * The headless React↔Phaser bridge. It owns the data layer (every IPC hook is
 * instantiated here, exactly once) and does two things:
 *
 *  1. re-emits the hooks' current state onto the EventBus so the Phaser scenes
 *     can render it (the live AgentEvent stream / a replay, settings, board
 *     summary, project, replay flag);
 *  2. turns world *commands* coming back off the bus (pick-project, board
 *     mutations, settings patches, logbook loads, replay) into the existing
 *     hooks' action functions — never an IPC call directly.
 *
 * It renders nothing: the whole UI is the Phaser canvas plus the IME text
 * overlay. Hard rule: the EventBus never reaches Rust. All IPC stays inside the
 * hooks; the bridge only shuttles their already-fetched data onto the bus and
 * routes bus commands to the hooks' existing action functions.
 */
export function GameBridge(): null {
  const supervisor = useSupervisor();
  const settings = useSettings();
  const board = useTaskBoard(supervisor.projectPath);
  const archive = useArchive();
  const projects = useProjects();
  const replay = useReplay();
  const health = useEnvironmentCheck();

  // New commissions always run in the saved default mode. The in-canvas mode
  // toggle was removed in the game-first redesign, so the settings ledger is the
  // single source of truth — read it live, because a one-time seed would ignore
  // the user later switching the default to 真实/real.
  const modeRef = useRef<RunMode>('simulate');
  modeRef.current = settings.settings?.defaultMode === 'real' ? 'real' : 'simulate';

  // Apply appearance settings to the document (uiScale + theme), as App used to.
  useEffect(() => {
    const s = settings.settings;
    if (!s) return;
    document.documentElement.style.setProperty('--ui-scale', String(s.uiScale));
    document.documentElement.dataset.theme = s.theme;
  }, [settings.settings]);

  // While a replay is playing (or its last frame is on screen) the workshop
  // shows the replay's events instead of the live stream (App's old logic).
  const officeEvents = replay.events.length > 0 ? replay.events : supervisor.events;

  // --- bus: push hook state → Phaser ------------------------------------

  // Re-flush the whole current snapshot whenever the Hall (re)mounts and signals
  // ready, so a scene restart re-hydrates without waiting for the next change.
  useEffect(() => {
    const flush = () => {
      EventBus.emit(BUS.officeEvents, officeEvents);
      EventBus.emit(BUS.settings, settings.settings);
      // settings save chip, so the SettingsScene ledger re-hydrates on (re)launch.
      EventBus.emit(BUS.settingsSave, settings.saveStatus);
      EventBus.emit(BUS.project, { projectPath: supervisor.projectPath });
      EventBus.emit(BUS.replay, { active: replay.active });
      // board snapshot, so the TaskBoardScene re-hydrates on (re)launch.
      EventBus.emit(BUS.boardTasks, board.tasks);
      EventBus.emit(BUS.boardMeta, {
        projectPath: supervisor.projectPath,
        error: board.error,
      });
      // archive snapshot, so the 档案库 DOM overlay re-hydrates on (re)open.
      EventBus.emit(BUS.archiveRecords, archive.records);
      EventBus.emit(BUS.archiveMeta, { error: archive.error });
      // projects snapshot, so the 项目管理 DOM overlay re-hydrates on (re)open.
      EventBus.emit(BUS.projectsState, {
        recent: projects.recent,
        current: supervisor.projectPath,
        error: projects.error ?? supervisor.error,
      } satisfies ProjectsState);
      // health snapshot, so the ledger's 工坊体检 page re-hydrates on (re)launch.
      EventBus.emit(BUS.health, {
        checks: health.checks,
        loading: health.loading,
        error: health.error,
      });
    };
    EventBus.on(BUS.sceneReady, flush);
    return () => {
      EventBus.off(BUS.sceneReady, flush);
    };
  });

  useEffect(() => {
    EventBus.emit(BUS.officeEvents, officeEvents);
  }, [officeEvents]);

  useEffect(() => {
    EventBus.emit(BUS.settings, settings.settings);
  }, [settings.settings]);

  useEffect(() => {
    EventBus.emit(BUS.settingsSave, settings.saveStatus);
  }, [settings.saveStatus]);

  useEffect(() => {
    EventBus.emit(BUS.project, { projectPath: supervisor.projectPath });
  }, [supervisor.projectPath]);

  useEffect(() => {
    EventBus.emit(BUS.replay, { active: replay.active });
  }, [replay.active]);

  // Board concurrency summary for the HUD.
  const summary = useMemo<TaskBoardSummary>(() => {
    const running = board.tasks.filter(
      t => t.status === 'running' || t.status === 'verifying',
    ).length;
    const queued = board.tasks.filter(t => t.status === 'queued').length;
    return { running, queued, maxWorkers: settings.settings?.maxWorkers ?? 1 };
  }, [board.tasks, settings.settings]);

  useEffect(() => {
    EventBus.emit(BUS.board, summary);
  }, [summary]);

  // Full board snapshot + meta for the in-world TaskBoardScene (P2).
  useEffect(() => {
    EventBus.emit(BUS.boardTasks, board.tasks);
  }, [board.tasks]);

  useEffect(() => {
    EventBus.emit(BUS.boardMeta, {
      projectPath: supervisor.projectPath,
      error: board.error,
    });
  }, [supervisor.projectPath, board.error]);

  // Full archive snapshot + meta for the in-world ArchiveScene (P4).
  useEffect(() => {
    EventBus.emit(BUS.archiveRecords, archive.records);
  }, [archive.records]);

  useEffect(() => {
    EventBus.emit(BUS.archiveMeta, { error: archive.error });
  }, [archive.error]);

  // Recent-projects list + current pick for the 项目管理 DOM overlay (door hotspot).
  useEffect(() => {
    EventBus.emit(BUS.projectsState, {
      recent: projects.recent,
      current: supervisor.projectPath,
      error: projects.error ?? supervisor.error,
    } satisfies ProjectsState);
  }, [projects.recent, projects.error, supervisor.projectPath, supervisor.error]);

  // Workshop pre-flight self-diagnosis for the ledger's 工坊体检 page (M4-UI).
  useEffect(() => {
    EventBus.emit(BUS.health, {
      checks: health.checks,
      loading: health.loading,
      error: health.error,
    });
  }, [health.checks, health.loading, health.error]);

  // --- bus: world commands → hooks --------------------------------------

  // The door/sign hotspot → useSupervisor.pickProject (the native folder dialog
  // over the unchanged pick_project IPC). Kept for any direct pick caller; the
  // door now opens the 项目管理 overlay (BUS.openProjects), whose 添加 button routes
  // through BUS.projectAdd below.
  useEffect(() => {
    const onPick = () => void supervisor.pickProject();
    EventBus.on(BUS.pickProject, onPick);
    return () => {
      EventBus.off(BUS.pickProject, onPick);
    };
  }, [supervisor]);

  // Project CRUD from the 项目管理 overlay → the existing hooks (the only IPC
  // callers; the bus never touches Rust). Add (folder dialog) + select-as-current
  // stay in useSupervisor, which owns projectPath (the world's project gate);
  // remove + rename are in useProjects. After a supervisor mutation we also
  // refresh useProjects so its recent list (which feeds the overlay) stays live.
  useEffect(() => {
    const onAdd = () =>
      void supervisor.pickProject().then(() => projects.refresh());
    const onSelect = (p: ProjectSelectPayload) =>
      void supervisor.selectRecentProject(p.path).then(() => projects.refresh());
    const onRemove = (p: ProjectRemovePayload) => void projects.remove(p.path);
    const onAlias = (p: ProjectAliasPayload) => void projects.rename(p.path, p.alias);
    EventBus.on(BUS.projectAdd, onAdd);
    EventBus.on(BUS.projectSelect, onSelect);
    EventBus.on(BUS.projectRemove, onRemove);
    EventBus.on(BUS.projectAlias, onAlias);
    return () => {
      EventBus.off(BUS.projectAdd, onAdd);
      EventBus.off(BUS.projectSelect, onSelect);
      EventBus.off(BUS.projectRemove, onRemove);
      EventBus.off(BUS.projectAlias, onAlias);
    };
  }, [supervisor, projects]);

  // Board mutations from the TaskBoardScene → the existing useTaskBoard actions
  // (the only IPC caller). New commissions read the live default mode.
  useEffect(() => {
    const onEnqueue = (p: BoardEnqueuePayload) => void board.enqueue(p.prompt, modeRef.current);
    const onReorder = (p: BoardReorderPayload) => void board.reorder(p.id, p.position);
    const onCancel = (p: BoardCancelPayload) => void board.cancel(p.id);
    EventBus.on(BUS.boardEnqueue, onEnqueue);
    EventBus.on(BUS.boardReorder, onReorder);
    EventBus.on(BUS.boardCancel, onCancel);
    return () => {
      EventBus.off(BUS.boardEnqueue, onEnqueue);
      EventBus.off(BUS.boardReorder, onReorder);
      EventBus.off(BUS.boardCancel, onCancel);
    };
  }, [board]);

  // Settings edits from the SettingsScene ledger → the unchanged useSettings.patch
  // (debounced persistence, the only settings IPC seam). The bus carries a patch;
  // the hook owns the optimistic merge + round-trip exactly as before.
  useEffect(() => {
    const onPatch = (p: SettingsPatch) => settings.patch(p);
    EventBus.on(BUS.settingsPatch, onPatch);
    return () => {
      EventBus.off(BUS.settingsPatch, onPatch);
    };
  }, [settings]);

  // Open-Logbook requests (Hall archer / Archive book) → useArchive.loadEvents
  // (the only §11 IPC caller). The bridge owns the data seam: it pushes a loading
  // state immediately, loads + parses the run's events, computes the totals, and
  // pushes the resolved state on BUS.logbookState for the React overlay to render.
  // A stale load (the overlay re-opened on another run before the first resolved)
  // is discarded by comparing against a monotonic token. The IPC stays in the hook.
  const logbookToken = useRef(0);
  useEffect(() => {
    const onOpen = (req: LogbookOpen) => {
      const token = ++logbookToken.current;
      const loading: LogbookState = {
        meta: { ...req.meta, count: 0, cost: req.meta.costUsd ?? null, durationMs: 0, turns: null },
        from: req.from,
        raw: [],
        loading: true,
        error: false,
      };
      EventBus.emit(BUS.logbookState, loading);
      archive
        .loadEvents(req.meta.taskId)
        .then(events => {
          if (token !== logbookToken.current) return;
          const totals = logbookTotals(events, req.meta.costUsd ?? null);
          EventBus.emit(BUS.logbookState, {
            meta: { ...req.meta, ...totals },
            from: req.from,
            raw: events,
            loading: false,
            error: false,
          } satisfies LogbookState);
        })
        .catch(() => {
          if (token !== logbookToken.current) return;
          EventBus.emit(BUS.logbookState, {
            ...loading,
            loading: false,
            error: true,
          } satisfies LogbookState);
        });
    };
    const onClose = () => {
      // bump the token so a still-in-flight load can't re-open the overlay.
      logbookToken.current++;
      EventBus.emit(BUS.logbookState, null);
    };
    EventBus.on(BUS.openLogbook, onOpen);
    EventBus.on(BUS.closeLogbook, onClose);
    return () => {
      EventBus.off(BUS.openLogbook, onOpen);
      EventBus.off(BUS.closeLogbook, onClose);
    };
  }, [archive]);

  // "回放" from the Logbook overlay → useReplay.start (the workshop then re-enacts
  // the run via the archer; replay.events already feed officeEvents above, so the
  // HallScene swaps to the replay stream with no extra wiring).
  useEffect(() => {
    const onReplay = (stored: StoredEvent[]) => replay.start(stored);
    EventBus.on(BUS.replayStart, onReplay);
    return () => {
      EventBus.off(BUS.replayStart, onReplay);
    };
  }, [replay]);

  // 工坊体检 page's 重新检查 button → useEnvironmentCheck.refresh (the only
  // check_environment IPC caller; the result re-pushes on BUS.health above).
  useEffect(() => {
    const onRefresh = () => void health.refresh();
    EventBus.on(BUS.healthRefresh, onRefresh);
    return () => {
      EventBus.off(BUS.healthRefresh, onRefresh);
    };
  }, [health]);

  return null;
}
