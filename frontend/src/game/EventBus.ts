import Phaser from 'phaser';
import type { TaskRecord } from '@/types/persistence.types';

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

  // Commands Phaser → React (GameBridge subscribes and calls the hooks).
  // payload: { hotspot: 'board' | 'settings' | 'archive' | 'project' }
  openHotspot: 'cmd:open-hotspot',
  // a world scene asks for keyboard input via the React IME-safe overlay (P2+).
  // payload: TextInputRequest (see below); the overlay replies on `req.channel`.
  textInput: 'cmd:text-input',
  // board mutations a world scene issues; GameBridge routes them to useTaskBoard.
  boardEnqueue: 'cmd:board-enqueue', // { prompt: string; mode: RunMode }
  boardReorder: 'cmd:board-reorder', // { id: string; position: number }
  boardCancel: 'cmd:board-cancel', // { id: string }
} as const;

// Snapshot of the board's live concurrency, surfaced on the HUD.
export interface TaskBoardSummary {
  running: number;
  queued: number;
  maxWorkers: number;
}

// The diegetic world objects a hotspot click can open. The notice board is now
// an in-world scene (TaskBoardScene, P2); settings / archive / project still ride
// the temporary React overlay bridge (P3..P4 replace those with in-world scenes).
export type Hotspot = 'settings' | 'archive' | 'project';

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

// Re-export so scenes import the board row shape from one place.
export type { TaskRecord };
