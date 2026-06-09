//! Durable agent memory in SQLite (DESIGN §6 "记忆" / §20 schema, P1 subset).
//!
//! The supervisor's semantic memory: **episodes** (what happened — each run bound
//! to its commit + verify result + diff, §6.2) and **bi-temporal facts** (what is
//! true — append-only, never deleted, carrying a trust tier, §6.3). It lives in
//! its own `memory.sqlite` file, separate from the operational `quiver.sqlite`
//! ([`quiver_store`]) — memory grows independently and has different access shapes.
//!
//! ## P1 scope (DESIGN §23 P1)
//!
//! This is the **append-only** foundation: open + migrate the two base tables.
//! Per §23 the P1 phase is *append-only* — facts are written and read; the
//! *invalidation / supersede* machinery (the librarian, §6.4) is P2. The schema
//! already carries the bi-temporal columns (`valid_at` / `invalid_at` /
//! `superseded_by` / …) so P2 only adds behavior, not columns. FTS5 / vector
//! search (§20 `memory_fact_fts` / `memory_fact_vec`) land in later P1/P2 slices.
//!
//! Layout mirrors [`quiver_store`]: [`schema`] owns the idempotent migration;
//! record/fact accessor modules hang off [`MemoryStore`] as they are added.

mod brief;
mod episodes;
mod facts;
mod schema;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

pub use brief::Brief;
pub use episodes::{EpisodeRecord, NewEpisode};
pub use facts::{FactRecord, NewFact};

/// A SQLite-backed durable memory store. Cheap to construct; one connection
/// behind a `Mutex` (rusqlite is synchronous, calls are short — same contract as
/// [`quiver_store::Store`](../quiver_store/struct.Store.html)).
pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl MemoryStore {
    /// The default memory DB path under an app data dir (`<data>/memory.sqlite`).
    pub fn default_db_path(data_dir: impl AsRef<Path>) -> PathBuf {
        data_dir.as_ref().join("memory.sqlite")
    }

    /// Open (creating if absent) the memory DB at `db_path` and run the migration.
    /// Parent dirs are created if needed.
    pub fn open(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(db_path)?)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_migrates_clean() {
        // Opening runs the migration; a second open is independent + also clean.
        MemoryStore::open_in_memory().expect("first open migrates");
        MemoryStore::open_in_memory().expect("second open migrates");
    }

    #[test]
    fn default_db_path_is_under_data_dir() {
        let p = MemoryStore::default_db_path("/tmp/quiver-data");
        assert!(p.ends_with("memory.sqlite"));
        assert!(p.starts_with("/tmp/quiver-data"));
    }
}
