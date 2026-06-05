import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { useSettings } from '@/hooks/useSettings';
import { useTaskBoard } from '@/hooks/useTaskBoard';
import { useArchive } from '@/hooks/useArchive';
import { useReplay } from '@/hooks/useReplay';
import {
  EventBus,
  BUS,
  type Hotspot,
  type TaskBoardSummary,
  type BoardEnqueuePayload,
  type BoardReorderPayload,
  type BoardCancelPayload,
} from '@/game/EventBus';
import { GameStateContext, type GameState } from '@/game/useGameState';
import type { RunMode } from '@/types/run.types';

interface GameBridgeProps {
  /** A hotspot was clicked in the world — App opens the matching React overlay. */
  onOpenHotspot: (hotspot: Hotspot) => void;
  /** The temporary React overlays, which read the shared GameState via context. */
  children: ReactNode;
}

/**
 * The headless React↔Phaser bridge. It owns the data layer (every IPC hook is
 * instantiated here, exactly once) and does three things:
 *
 *  1. re-emits the hooks' current state onto the EventBus so the Phaser scenes
 *     can render it (the live AgentEvent stream / a replay, settings, board
 *     summary, project, replay flag);
 *  2. turns world *commands* coming back off the bus (a hotspot click) into the
 *     App-level overlay action — never an IPC call directly;
 *  3. exposes the hooks to the temporary overlays via Context.
 *
 * Hard rule: the EventBus never reaches Rust. All IPC stays inside the hooks;
 * the bridge only shuttles their already-fetched data onto the bus and routes
 * bus commands to the hooks' existing action functions.
 */
export function GameBridge({ onOpenHotspot, children }: GameBridgeProps) {
  const supervisor = useSupervisor();
  const settings = useSettings();
  const board = useTaskBoard(supervisor.projectPath);
  const archive = useArchive();
  const replay = useReplay();

  const [mode, setMode] = useState<RunMode>('simulate');

  // Pre-select the run mode from the saved default — once, before the user
  // touches the toggle (preserves App.tsx's old seeding behaviour).
  const seededMode = useRef(false);
  useEffect(() => {
    if (!seededMode.current && settings.settings) {
      seededMode.current = true;
      const def = settings.settings.defaultMode;
      if (def === 'real' || def === 'simulate') setMode(def);
    }
  }, [settings.settings]);

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
      EventBus.emit(BUS.project, { projectPath: supervisor.projectPath });
      EventBus.emit(BUS.replay, { active: replay.active });
      // board snapshot, so the TaskBoardScene re-hydrates on (re)launch.
      EventBus.emit(BUS.boardTasks, board.tasks);
      EventBus.emit(BUS.boardMeta, {
        projectPath: supervisor.projectPath,
        error: board.error,
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

  // --- bus: world commands → App / hooks --------------------------------

  useEffect(() => {
    const onOpen = (payload: { hotspot: Hotspot }) => onOpenHotspot(payload.hotspot);
    EventBus.on(BUS.openHotspot, onOpen);
    return () => {
      EventBus.off(BUS.openHotspot, onOpen);
    };
  }, [onOpenHotspot]);

  // Board mutations from the TaskBoardScene → the existing useTaskBoard actions
  // (the only IPC caller). The run mode is owned here, seeded from settings.
  useEffect(() => {
    const onEnqueue = (p: BoardEnqueuePayload) => void board.enqueue(p.prompt, mode);
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
  }, [board, mode]);

  const value: GameState = {
    supervisor,
    settings,
    board,
    archive,
    replay,
    mode,
    setMode,
  };

  return <GameStateContext.Provider value={value}>{children}</GameStateContext.Provider>;
}
