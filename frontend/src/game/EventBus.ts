import Phaser from 'phaser';
import type {
  TaskRecord,
  SettingsPatch,
  StoredEvent,
  RecentProject,
} from '@/types/persistence.types';
import type { EnvironmentCheck } from '@/types/environment.types';

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
  // `HealthSnapshot` — the workshop pre-flight self-diagnosis result (DESIGN §9),
  // pushed from useEnvironmentCheck for the ledger's 工坊体检 page (M4-UI).
  health: 'health:changed',
  // `ProjectsState` — the recent-projects list + current project for the door's
  // 项目管理 DOM overlay. Pushed from useSupervisor / useProjects via the bridge.
  projectsState: 'projects:state',

  // Commands Phaser → React (GameBridge subscribes and calls the hooks).
  // the door/sign hotspot asks to pick a project (the native folder dialog via the
  // unchanged useSupervisor.pickProject IPC). No payload — the LAST temporary React
  // overlay is gone (P5): the door drives the hook directly, no DOM panel.
  pickProject: 'cmd:pick-project',
  // a world scene (Hall archer / Archive book) asks to unroll ONE run's Logbook —
  // now a real React DOM overlay (the 卷轴皮 wraps a native scrollable transcript so
  // dense text never overflows a hand-painted mask). Payload: LogbookOpen (below).
  // The GameBridge satisfies it via the unchanged useArchive.loadEvents (the only
  // §11 IPC caller) and pushes the loaded state on `BUS.logbookState`.
  openLogbook: 'cmd:logbook-open',
  // the overlay asks to roll the scroll back up (its 卷起 button / scrim click).
  closeLogbook: 'cmd:logbook-close',
  // the loaded Logbook state the React overlay renders (meta + transcript +
  // loading / error). Pushed by the GameBridge after it loads the run's events.
  // payload: LogbookState | null (null = overlay closed).
  logbookState: 'logbook:state',
  // the Logbook overlay asks the bridge to start a "回放" re-enactment in the Hall.
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
  // the ledger's 工坊体检 page asks the bridge to (re)run the pre-flight checks.
  // No payload; GameBridge routes it to useEnvironmentCheck.refresh (the only
  // check_environment IPC caller), which re-pushes the result on `BUS.health`.
  healthRefresh: 'cmd:health-refresh',

  // --- 档案库 DOM overlay (book shelf hotspot) -------------------------------
  // the Hall's bookshelf hotspot asks to open the 档案库 overlay (a React DOM
  // parchment list — replaces the old in-world ArchiveScene whose hand-painted
  // list overflowed + mis-hit on large windows). No payload; App renders the
  // <ArchiveOverlay/> which reads the records the bridge already pushes on
  // `BUS.archiveRecords` / `BUS.archiveMeta`.
  openArchive: 'cmd:archive-open',
  // the overlay asks to close (its 关闭 button / scrim click / Escape).
  closeArchive: 'cmd:archive-close',

  // --- 项目管理 DOM overlay (door hotspot) -----------------------------------
  // the Hall's door hotspot asks to open the 项目管理 overlay (a React DOM
  // panel — the door no longer drives pickProject directly; it now opens a full
  // CRUD panel over the dimmed world). No payload; App renders <ProjectManagerOverlay/>.
  openProjects: 'cmd:projects-open',
  // the overlay asks to close (its 关闭 button / scrim click / Escape).
  closeProjects: 'cmd:projects-close',
  // project mutations the overlay issues; the bridge routes each to the existing
  // useSupervisor / useProjects action (the only IPC callers — the bus never
  // touches Rust). No payload for add (opens the native folder dialog).
  projectAdd: 'cmd:project-add',
  projectSelect: 'cmd:project-select', // { path: string }
  projectRemove: 'cmd:project-remove', // { path: string }
  projectAlias: 'cmd:project-alias', // { path: string; alias: string | null }
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
 * The recent-projects list + current selection the 项目管理 DOM overlay renders.
 * Pushed by the GameBridge from useSupervisor (recentProjects / projectPath).
 * The overlay only renders this; every mutation goes back over the project
 * command channels and is satisfied by the hooks (the bus never touches Rust).
 */
export interface ProjectsState {
  recent: RecentProject[];
  current: string | null;
  error: string | null;
}

// Project mutation command payloads (overlay → GameBridge → useProjects/useSupervisor).
export interface ProjectSelectPayload {
  path: string;
}
export interface ProjectRemovePayload {
  path: string;
}
export interface ProjectAliasPayload {
  path: string;
  /** New alias, or `null` to clear it (fall back to the path leaf). */
  alias: string | null;
}

/**
 * The §1 commission head a Logbook shows above its transcript. Assembled at the
 * open site (Hall archer / Archive book) from whatever record it has; the totals
 * (event count / cost / duration / turns) are filled in by the GameBridge once it
 * has loaded + parsed the run's events. A Hall archer click may only know the
 * `taskId`, so every field but `taskId` is optional.
 */
export interface LogbookMeta {
  taskId: string;
  prompt?: string;
  project?: string;
  /** Raw run mode ('real' | 'simulate') — labelled to 真实 / 模拟 in the overlay. */
  mode?: string;
  /** Raw task status — labelled via TASK_STATUS_LABEL in the overlay. */
  status?: string;
  branch?: string | null;
  /** Fallback cost from the record, used only if the events carry none. */
  costUsd?: number | null;
}

/**
 * A world scene's request to open the Logbook overlay for one run (scene →
 * GameBridge). The bridge loads the events (useArchive.loadEvents) and pushes a
 * {@link LogbookState} on `BUS.logbookState`. `from` records where it was opened
 * so a "回放" can fully clean up an archive scene if needed (parity with the old
 * scene's bookkeeping).
 */
export interface LogbookOpen {
  meta: LogbookMeta;
  from: 'archive' | 'hall';
}

/**
 * The fully-resolved Logbook state the React overlay renders. `raw` is the run's
 * verbatim stored log (handed to useReplay.start on 回放); `meta` carries the head
 * plus the totals the bridge computed from the parsed events. `loading` is true
 * until the load settles; `error` is set if the §11 load failed.
 */
export interface LogbookState {
  meta: LogbookMeta & {
    count: number;
    cost: number | null;
    durationMs: number;
    turns: number | null;
  };
  from: 'archive' | 'hall';
  raw: StoredEvent[];
  loading: boolean;
  error: boolean;
}

/**
 * The workshop pre-flight self-diagnosis snapshot pushed on `BUS.health`
 * (useEnvironmentCheck → SettingsScene's 工坊体检 page). `loading` is true while a
 * check is in flight; `error` carries a command-level failure (vs per-check
 * warn/fail rows, which live inside `checks`).
 */
export interface HealthSnapshot {
  checks: EnvironmentCheck[];
  loading: boolean;
  error: string | null;
}

// Re-export so scenes import the board/archive row + settings-patch + stored
// event + health-check + recent-project shapes from one place.
export type { TaskRecord, SettingsPatch, StoredEvent, RecentProject };
export type { EnvironmentCheck };
