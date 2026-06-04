//! Durable app state in SQLite (DESIGN §11, pragmatic subset).
//!
//! Quiver's picked project, recent projects, and run history must survive an app
//! restart. This crate owns a single embedded SQLite file (`quiver.sqlite` under
//! the app data dir) and exposes a small, synchronous API over it through one
//! [`Store`] entry point.
//!
//! It is its own crate (not part of `quiver-core`) so persistence has a
//! growth-ready home: memory and workflow-state persistence will land here later,
//! each as its own module + migration alongside the existing project/history
//! concerns. The layout is deliberately split by concern:
//!
//! - [`lib`](self) — the [`Store`] connection entry point (open + migrate + the
//!   [`initial_state`](Store::initial_state) startup bundle) and the public
//!   re-exports.
//! - [`schema`] — the idempotent migration runner. New tables go here.
//! - [`projects`] — `app_config` (single picked project) + `recent_projects`.
//! - [`history`] — append-only `run_history`.
//! - [`settings`] — single-row typed app config (v1.0 spec module 1).
//! - [`events`] — append-only `agent_event` source-of-truth log (§11).
//! - [`tasks`] — the `task` queue + lifecycle board (v1.0 spec module 5).
//!
//! rusqlite is synchronous; the connection is guarded by a `Mutex`. All calls are
//! short (single statements over a tiny local file) so holding the lock briefly
//! on the Tauri command thread is fine — no `.await` is held across it.

mod events;
mod history;
mod projects;
mod schema;
mod settings;
mod tasks;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;

pub use events::{NewEvent, StoredEvent};
pub use history::{NewRun, RunRecord};
pub use projects::RecentProject;
pub use settings::{Settings, SettingsPatch};
pub use tasks::{NewTask, TaskRecord};

/// A SQLite-backed durable store. Cheap to construct; holds one pooled connection
/// behind a `Mutex`. New persistence concerns (memory, workflow state) get their
/// own module + accessor methods on this same `Store`.
pub struct Store {
    conn: Mutex<Connection>,
}

/// The startup bundle the UI hydrates from (`get_initial_state`).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitialState {
    pub last_project: Option<String>,
    pub recent_projects: Vec<RecentProject>,
    pub history: Vec<RunRecord>,
}

impl Store {
    /// Open (creating if absent) the SQLite DB at `db_path` and run the schema
    /// migration. Parent dirs are created if needed.
    pub fn open(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        Self::from_connection(conn)
    }

    /// In-memory store — for tests.
    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> anyhow::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The default DB path under an app data dir.
    pub fn default_db_path(app_data_dir: impl AsRef<Path>) -> PathBuf {
        app_data_dir.as_ref().join("quiver.sqlite")
    }

    /// The full startup bundle: last project + recents + history in one read.
    pub fn initial_state(&self) -> anyhow::Result<InitialState> {
        Ok(InitialState {
            last_project: self.last_project()?,
            recent_projects: self.recent_projects()?,
            history: self.run_history()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_run(project: &str, prompt: &str, ts: i64) -> NewRun {
        NewRun {
            project: project.to_string(),
            prompt: prompt.to_string(),
            mode: "simulate".to_string(),
            status: "verified".to_string(),
            cost_usd: Some(0.01),
            branch: None,
            created_at: ts,
        }
    }

    #[test]
    fn last_project_round_trips() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.last_project().unwrap(), None);
        store.set_last_project("/repos/alpha").unwrap();
        assert_eq!(store.last_project().unwrap().as_deref(), Some("/repos/alpha"));
        // Single-row config: setting again overwrites, not appends.
        store.set_last_project("/repos/beta").unwrap();
        assert_eq!(store.last_project().unwrap().as_deref(), Some("/repos/beta"));
    }

    #[test]
    fn recent_projects_are_mru_and_deduped() {
        let store = Store::open_in_memory().unwrap();
        store.touch_recent_project("/repos/a", 100).unwrap();
        store.touch_recent_project("/repos/b", 200).unwrap();
        // Re-touching /repos/a with a newer ts moves it to the front, no dup.
        store.touch_recent_project("/repos/a", 300).unwrap();

        let recents = store.recent_projects().unwrap();
        assert_eq!(recents.len(), 2, "deduped by path");
        assert_eq!(recents[0].path, "/repos/a");
        assert_eq!(recents[0].last_used_at, 300);
        assert_eq!(recents[1].path, "/repos/b");
    }

    #[test]
    fn run_history_records_and_reads_newest_first() {
        let store = Store::open_in_memory().unwrap();
        let id1 = store.record_run(&new_run("/repos/a", "first", 1000)).unwrap();
        let id2 = store
            .record_run(&new_run("/repos/a", "second", 2000))
            .unwrap();
        assert!(id2 > id1);

        let history = store.run_history().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].prompt, "second", "newest first");
        assert_eq!(history[0].cost_usd, Some(0.01));
        assert_eq!(history[1].prompt, "first");
    }

    #[test]
    fn initial_state_bundles_everything() {
        let store = Store::open_in_memory().unwrap();
        store.set_last_project("/repos/a").unwrap();
        store.touch_recent_project("/repos/a", 500).unwrap();
        store.record_run(&new_run("/repos/a", "task", 600)).unwrap();

        let state = store.initial_state().unwrap();
        assert_eq!(state.last_project.as_deref(), Some("/repos/a"));
        assert_eq!(state.recent_projects.len(), 1);
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn reopening_the_same_file_restores_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = Store::default_db_path(dir.path());

        {
            let store = Store::open(&path).unwrap();
            store.set_last_project("/repos/persisted").unwrap();
            store.touch_recent_project("/repos/persisted", 42).unwrap();
            store
                .record_run(&new_run("/repos/persisted", "remembered", 99))
                .unwrap();
        }

        // A fresh handle on the SAME file (simulating an app restart) sees it all.
        let reopened = Store::open(&path).unwrap();
        let state = reopened.initial_state().unwrap();
        assert_eq!(state.last_project.as_deref(), Some("/repos/persisted"));
        assert_eq!(state.recent_projects.len(), 1);
        assert_eq!(state.recent_projects[0].path, "/repos/persisted");
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].prompt, "remembered");
    }
}
