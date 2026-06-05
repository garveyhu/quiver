import Phaser from 'phaser';
import type { TaskRecord, SettingsPatch, StoredEvent } from '@/types/persistence.types';

/**
 * The single React ↔ Phaser event seam (lifted from phaserjs/template-react-ts).
 *
 * A process-wide `Phaser.Events.EventEmitter` that the headless `GameBridge`
 * pushes hook data onto (settings / tasks / live agent events) and that scenes
 * subscribe to in `create()` — and that scenes emit *commands* back onto, which
 * the bridge turns into the existing hooks' `invoke` calls.
 *
 * Hard rule: the bus NEVER touches Rust. Hooks remain the only IPC caller; the
 * bus is a pure in-process pub/sub layer between the data layer and the canvas.
 */
export const EventBus = new Phaser.Events.EventEmitter();

// DEV-ONLY: expose the bus so the headless verification harness can listen for
// scene→bridge commands (board reorder / enqueue) and assert the IME gate. The
// packaged app never sets this.
if (import.meta.env.DEV) {
  (window as unknown as { __bus?: typeof EventBus }).__bus = EventBus;
}

// --- channel names -------------------------------------------------------
// Data pushed React → Phaser (GameBridge re-emits hook state onto these).
export const BUS = {
  // live `AgentEvent[]` snapshot (live stream or an active replay) for the archers
  officeEvents: 'office:events',
  // `Settings | null` — drives the HUD budget / fireplace + ledger overlay
  settings: 'settings:changed',
  // `TaskBoardSummary` — running / queued counts for the HUD
  board: 'board:changed',
  // `{ projectPath: string | null }` — gates the door / first-run glow
  project: 'project:changed',
  // `{ active: boolean }` — a Logbook re-enactment is playing
  replay: 'replay:changed',
  // the HallScene is created and ready to receive data (bridge flushes on this)
  sceneReady: 'scene:ready',
  // full `TaskRecord[]` board snapshot for the in-world TaskBoardScene (P2).
  boardTasks: 'board:tasks',
  // `{ projectPath: string | null; error: string | null }` — board-scene gating.
  boardMeta: 'board:meta',
  // every persisted run (`TaskRecord[]`, newest first) for the in-world
  // ArchiveScene bookshelf (P4). Pushed from useArchive.records.
  archiveRecords: 'archive:records',
  // `{ error: string | null }` — archive-scene error surface.
  archiveMeta: 'archive:meta',

  // Commands Phaser → React (GameBridge subscribes and calls the hooks).
  // the door/sign hotspot asks to pick a project (the native folder dialog via the
  // unchanged useSupervisor.pickProject IPC). No payload — the LAST temporary React
  // overlay is gone (P5): the door drives the hook directly, no DOM panel.
  pickProject: 'cmd:pick-project',
  // the LogbookScene asks the bridge to load ONE run's full §11 event log; the
  // bridge calls useArchive.loadEvents and replies once on `req.channel`.
  // payload: LogbookRequest (see below).
  logbookLoad: 'cmd:logbook-load',
  // the LogbookScene asks the bridge to start a "回放" re-enactment in the Hall.
  // payload: StoredEvent[] — the run's raw stored log; the bridge calls
  // useReplay.start with it (the workshop then re-enacts via the archer).
  replayStart: 'cmd:replay-start',
  // a world scene asks for keyboard input via the React IME-safe overlay (P2+).
  // payload: TextInputRequest (see below); the overlay replies on `req.channel`.
  textInput: 'cmd:text-input',
  // board mutations a world scene issues; GameBridge routes them to useTaskBoard.
  boardEnqueue: 'cmd:board-enqueue', // { prompt: string; mode: RunMode }
  boardReorder: 'cmd:board-reorder', // { id: string; position: number }
  boardCancel: 'cmd:board-cancel', // { id: string }
  // a settings edit a world scene (the ledger, P3) issues; GameBridge routes it
  // to the unchanged useSettings.patch (the sole settings IPC seam, debounced).
  // payload: SettingsPatch
  settingsPatch: 'cmd:settings-patch',
  // `SaveStatus` — the ledger's "记录中… / 已记录" chip, pushed from useSettings.
  settingsSave: 'settings:save-status',
} as const;

// Snapshot of the board's live concurrency, surfaced on the HUD.
export interface TaskBoardSummary {
  running: number;
  queued: number;
  maxWorkers: number;
}

// Anchor rect (canvas/page pixels) the React overlay positions its field over.
export interface TextInputAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * A scene's request for keyboard input, satisfied by the IME-safe React overlay
 * (§0 option-b: never type in canvas). The overlay mounts a real `<textarea>` /
 * `<input>` over `anchor`, and on commit/cancel emits ONCE on `channel` with the
 * resolved string (or `null` if cancelled). One-shot: the channel is the reply
 * address, so two concurrent requests never cross wires.
 */
export interface TextInputRequest {
  /** Unique reply channel name the overlay emits the result on (exactly once). */
  channel: string;
  anchor: TextInputAnchor;
  /** Initial field value. */
  value: string;
  /** Multi-line `<textarea>` (Enter = newline, ⌘/Ctrl+Enter = submit) vs single. */
  multiline: boolean;
  placeholder: string;
  /** Submit button label (Chinese). */
  submitLabel: string;
}

// Board mutation command payloads (scene → GameBridge → useTaskBoard). The run
// mode is owned by the bridge (seeded from settings.defaultMode), so the scene
// only carries the prompt.
export interface BoardEnqueuePayload {
  prompt: string;
}
export interface BoardReorderPayload {
  id: string;
  position: number;
}
export interface BoardCancelPayload {
  id: string;
}

// `board:meta` snapshot: which project the board belongs to + any board error.
export interface BoardMeta {
  projectPath: string | null;
  error: string | null;
}

// `archive:meta` snapshot: the archive load error, if any.
export interface ArchiveMeta {
  error: string | null;
}

/**
 * The LogbookScene's request for ONE run's full §11 event log (scene →
 * GameBridge → useArchive.loadEvents). Like {@link TextInputRequest} it carries a
 * one-shot reply `channel`: the bridge resolves it exactly once with the loaded
 * `StoredEvent[]` (or `null` on failure), so concurrent opens never cross wires.
 */
export interface LogbookRequest {
  channel: string;
  taskId: string;
}

// Re-export so scenes import the board/archive row + settings-patch + stored
// event shapes from one place.
export type { TaskRecord, SettingsPatch, StoredEvent };
