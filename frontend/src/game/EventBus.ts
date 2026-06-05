import Phaser from 'phaser';

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

  // Commands Phaser → React (GameBridge subscribes and calls the hooks).
  // payload: { hotspot: 'board' | 'settings' | 'archive' | 'project' }
  openHotspot: 'cmd:open-hotspot',
} as const;

// Snapshot of the board's live concurrency, surfaced on the HUD.
export interface TaskBoardSummary {
  running: number;
  queued: number;
  maxWorkers: number;
}

// The diegetic world objects a hotspot click can open (temporary React overlay
// bridge for milestone 1 — P2..P4 replace these with in-world scenes).
export type Hotspot = 'board' | 'settings' | 'archive' | 'project';
