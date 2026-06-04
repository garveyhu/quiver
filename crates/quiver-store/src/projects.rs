//! Picked-project + recent-projects persistence.
//!
//! Backs the `app_config` (single picked project) and `recent_projects` (MRU
//! list) tables. Accessors hang off [`Store`] so callers go through one entry
//! point.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::Store;

/// One entry in the most-recently-used project list (DESIGN §11 `recent_projects`).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub last_used_at: i64,
}

/// How many recent projects the UI shows.
const RECENT_LIMIT: usize = 10;

impl Store {
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
}
