// Wire shapes for the durable §11 store, serialized by the quiver-store crate
// (camelCase). Kept in lockstep with crates/quiver-store/src/*.rs.

export interface RecentProject {
  path: string;
  lastUsedAt: number;
  // Optional user-given display name; when null/absent the UI falls back to the
  // path's last segment. Set/cleared via `set_project_alias`.
  alias?: string | null;
}

// Returned by the `get_initial_state` command on app load. The legacy
// single-shot run history was superseded by the durable `task` queue (the
// 公告板 + 档案库 read it via `list_tasks`), so only the last project + recents
// are consumed here. The Rust command may still carry a `history` field on the
// wire; it is intentionally not surfaced to the UI.
export interface InitialState {
  lastProject: string | null;
  recentProjects: RecentProject[];
}

// Single-row typed app settings (settings.rs / v1.0 module 1). Returned by
// `get_settings` and `update_settings`. For Phase B's settings ledger.
export interface Settings {
  defaultMode: string;
  model: string;
  maxWorkers: number;
  monthlyCreditCapUsd: number | null;
  nightlyBudgetUsd: number | null;
  agentBinOverride: string | null;
  fakeDelayMs: number;
  theme: string;
  uiScale: number;
  /** 验证关命令(sh -c),空=不真正校验(等同总是通过) */
  verifyCommand: string;
}

// Partial update sent to `update_settings`: omit a key to leave it unchanged,
// send `null` on a nullable field to clear it. All keys optional.
export type SettingsPatch = Partial<Settings>;

// A queued / in-flight / finished task (tasks.rs / v1.0 module 5). Returned by
// `list_tasks`, ordered by board position then time. For Phase C's bulletin board.
export interface TaskRecord {
  id: string;
  project: string;
  prompt: string;
  mode: string;
  // queued | running | verifying | done | failed | needs_rebase
  status: string;
  costUsd: number | null;
  branch: string | null;
  position: number;
  createdAt: number;
  updatedAt: number;
}

// One stored event from the §11 source-of-truth log (events.rs). Returned by
// `get_task_events`, ordered by seq. `payloadJson` is the verbatim flat
// camelCase AgentEvent JSON — parse it with the live `AgentEvent` contract
// (agentEvent.types.ts) for Phase D's Logbook replay.
export interface StoredEvent {
  taskId: string;
  seq: number;
  tsMs: number;
  runner: string;
  kind: string;
  payloadJson: string;
}
