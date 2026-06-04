//! Idempotent SQLite schema migration (DESIGN §11 subset).
//!
//! [`migrate`] runs on every [`Store::open`](crate::Store::open) and is safe to
//! re-run. As persistence grows (memory, workflow state), add their `CREATE TABLE
//! IF NOT EXISTS` statements to this batch.

use rusqlite::Connection;

/// Idempotent schema creation. Safe to run on every open.
pub(crate) fn migrate(conn: &Connection) -> anyhow::Result<()> {
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
