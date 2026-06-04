// Wire shapes for the durable §11 store, serialized by quiver-core's `store`
// module (camelCase). Kept in lockstep with crates/quiver-core/src/store.rs.

export interface RecentProject {
  path: string;
  lastUsedAt: number;
}

export interface RunRecord {
  id: number;
  project: string;
  prompt: string;
  mode: string;
  status: string;
  costUsd: number | null;
  branch: string | null;
  createdAt: number;
}

// Returned by the `get_initial_state` command on app load.
export interface InitialState {
  lastProject: string | null;
  recentProjects: RecentProject[];
  history: RunRecord[];
}
