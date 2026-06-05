import { useEffect, useMemo, useRef, useState } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { useSettings } from '@/hooks/useSettings';
import { useTaskBoard } from '@/hooks/useTaskBoard';
import { useArchive } from '@/hooks/useArchive';
import { useReplay } from '@/hooks/useReplay';
import {
  EventBus,
  BUS,
  type TaskBoardSummary,
  type BoardEnqueuePayload,
  type BoardReorderPayload,
  type BoardCancelPayload,
  type LogbookRequest,
} from '@/game/EventBus';
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
      // archive snapshot, so the ArchiveScene bookshelf re-hydrates on (re)launch.
      EventBus.emit(BUS.archiveRecords, archive.records);
      EventBus.emit(BUS.archiveMeta, { error: archive.error });
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

  // --- bus: world commands → hooks --------------------------------------

  // The door/sign hotspot → useSupervisor.pickProject (the native folder dialog
  // over the unchanged pick_project IPC). No React overlay; the door drives the
  // hook directly.
  useEffect(() => {
    const onPick = () => void supervisor.pickProject();
    EventBus.on(BUS.pickProject, onPick);
    return () => {
      EventBus.off(BUS.pickProject, onPick);
    };
  }, [supervisor]);

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

  // LogbookScene event loads → useArchive.loadEvents (the only §11 IPC caller).
  // The request carries a one-shot reply channel; resolve it exactly once with
  // the loaded log, or null on failure — so a bad load never wedges the scroll.
  useEffect(() => {
    const onLoad = (req: LogbookRequest) => {
      archive
        .loadEvents(req.taskId)
        .then(events => EventBus.emit(req.channel, events))
        .catch(() => EventBus.emit(req.channel, null));
    };
    EventBus.on(BUS.logbookLoad, onLoad);
    return () => {
      EventBus.off(BUS.logbookLoad, onLoad);
    };
  }, [archive]);

  // "回放" from the LogbookScene → useReplay.start (the workshop then re-enacts
  // the run via the archer; replay.events already feed officeEvents above, so the
  // HallScene swaps to the replay stream with no extra wiring).
  useEffect(() => {
    const onReplay = (stored: StoredEvent[]) => replay.start(stored);
    EventBus.on(BUS.replayStart, onReplay);
    return () => {
      EventBus.off(BUS.replayStart, onReplay);
    };
  }, [replay]);

  return null;
}
