//! Durable app state in SQLite (DESIGN §11, pragmatic subset).
//!
//! Quiver's picked project, recent projects, and run history must survive an app
//! restart. This module owns a single embedded SQLite file (`quiver.sqlite` under
//! the app data dir) and exposes a small, synchronous API over it:
//!
//! - [`Store::open`] — open/create the DB and run the idempotent schema migration.
//! - [`Store::set_last_project`] / [`Store::last_project`] — the single picked repo.
//! - [`Store::touch_recent_project`] / [`Store::recent_projects`] — the MRU repo list.
//! - [`Store::record_run`] / [`Store::run_history`] — append-only run history.
//! - [`Store::initial_state`] — the bundle the UI loads on startup.
//!
//! rusqlite is synchronous; the connection is guarded by a `Mutex`. All calls are
//! short (single statements over a tiny local file) so holding the lock briefly
//! on the Tauri command thread is fine — no `.await` is held across it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

/// A SQLite-backed durable store. Cheap to construct; holds one pooled connection
/// behind a `Mutex`.
pub struct Store {
    conn: Mutex<Connection>,
}

/// One entry in the most-recently-used project list (DESIGN §11 `recent_projects`).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub last_used_at: i64,
}

/// One past run, newest first (DESIGN §11 `run_history` — a pragmatic merge of the
/// design's `task` / `run_summary` / `cost_ledger` summary fields).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: i64,
    pub project: String,
    pub prompt: String,
    pub mode: String,
    pub status: String,
    pub cost_usd: Option<f64>,
    /// The attempt branch the work lives on, when left un-merged (real-mode).
    pub branch: Option<String>,
    pub created_at: i64,
}

/// The startup bundle the UI hydrates from (`get_initial_state`).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitialState {
    pub last_project: Option<String>,
    pub recent_projects: Vec<RecentProject>,
    pub history: Vec<RunRecord>,
}

/// The fields of a finished run to persist (everything but the auto id + caller's
/// timestamp).
#[derive(Clone, Debug)]
pub struct NewRun {
    pub project: String,
    pub prompt: String,
    pub mode: String,
    pub status: String,
    pub cost_usd: Option<f64>,
    pub branch: Option<String>,
    pub created_at: i64,
}

/// How many history rows the UI shows / `initial_state` returns by default.
const HISTORY_LIMIT: usize = 50;
/// How many recent projects the UI shows.
const RECENT_LIMIT: usize = 10;

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
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The default DB path under an app data dir.
    pub fn default_db_path(app_data_dir: impl AsRef<Path>) -> PathBuf {
        app_data_dir.as_ref().join("quiver.sqlite")
    }

    /// Persist the single last-picked project path.
    pub fn set_last_project(&self, path: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO app_config (id, last_project) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET last_project = excluded.last_project",
            params![path],
        )?;
        Ok(())
    }

    /// The last-picked project path, if any was ever set.
    pub fn last_project(&self) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().expect("store lock");
        let value: Option<String> = conn
            .query_row(
                "SELECT last_project FROM app_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(value)
    }

    /// Upsert a project into the recent list, stamping `used_at` as its last use.
    pub fn touch_recent_project(&self, path: &str, used_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO recent_projects (path, last_used_at) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET last_used_at = excluded.last_used_at",
            params![path, used_at],
        )?;
        Ok(())
    }

    /// The most-recently-used projects, newest first (capped at [`RECENT_LIMIT`]).
    pub fn recent_projects(&self) -> anyhow::Result<Vec<RecentProject>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT path, last_used_at FROM recent_projects
             ORDER BY last_used_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![RECENT_LIMIT as i64], |row| {
                Ok(RecentProject {
                    path: row.get(0)?,
                    last_used_at: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Append one finished run to history; returns its row id.
    pub fn record_run(&self, run: &NewRun) -> anyhow::Result<i64> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO run_history
                (project, prompt, mode, status, cost_usd, branch, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.project,
                run.prompt,
                run.mode,
                run.status,
                run.cost_usd,
                run.branch,
                run.created_at
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Past runs, newest first (capped at [`HISTORY_LIMIT`]).
    pub fn run_history(&self) -> anyhow::Result<Vec<RunRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, prompt, mode, status, cost_usd, branch, created_at
             FROM run_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![HISTORY_LIMIT as i64], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    prompt: row.get(2)?,
                    mode: row.get(3)?,
                    status: row.get(4)?,
                    cost_usd: row.get(5)?,
                    branch: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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

/// Idempotent schema creation (DESIGN §11 subset). Safe to run on every open.
fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_config (
            id           INTEGER PRIMARY KEY CHECK (id = 1),
            last_project TEXT
        );
        CREATE TABLE IF NOT EXISTS recent_projects (
            path         TEXT PRIMARY KEY,
            last_used_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS run_history (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            project    TEXT NOT NULL,
            prompt     TEXT NOT NULL,
            mode       TEXT NOT NULL,
            status     TEXT NOT NULL,
            cost_usd   REAL,
            branch     TEXT,
            created_at INTEGER NOT NULL
        );",
    )?;
    Ok(())
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
