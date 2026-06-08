//! Idempotent SQLite schema migration (DESIGN §11 subset).
//!
//! [`migrate`] runs on every [`Store::open`](crate::Store::open) and is safe to
//! re-run. **This is the single growth point for persistence.** As the store
//! grows (memory, workflow state, per-project config), add the new table's
//! `CREATE TABLE IF NOT EXISTS` statement to this batch and give it a module +
//! accessors hanging off [`Store`](crate::Store) — exactly like
//! `settings` / `agent_event` / `task` did.
//!
//! v1.0 Phase A added three tables on top of the original picked-project /
//! recents / history trio:
//! - `settings` — single-row typed app config (module [`settings`](crate::settings)).
//! - `agent_event` — the §11 append-only source-of-truth event log
//!   (module [`events`](crate::events)).
//! - `task` — the queue + lifecycle board (module [`tasks`](crate::tasks)).

use rusqlite::Connection;

/// Add `column` (`decl` = its SQL type/decl) to `table` only if it isn't already
/// there. SQLite has no `ADD COLUMN IF NOT EXISTS`, so we probe `PRAGMA
/// table_info` first; this keeps `migrate` idempotent across re-opens and across
/// DBs created before/after the column existed.
fn add_column_if_absent(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column);
    if !exists {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
    }
    Ok(())
}

/// Idempotent schema creation. Safe to run on every open.
pub(crate) fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_config (
            id           INTEGER PRIMARY KEY CHECK (id = 1),
            last_project TEXT
        );
        CREATE TABLE IF NOT EXISTS recent_projects (
            path         TEXT PRIMARY KEY,
            last_used_at INTEGER NOT NULL,
            alias        TEXT
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
        );
        -- Single-row typed app settings (DESIGN §11, v1.0 spec module 1). The
        -- row is seeded with defaults just below so reads are never empty.
        CREATE TABLE IF NOT EXISTS settings (
            id                     INTEGER PRIMARY KEY CHECK (id = 1),
            default_mode           TEXT NOT NULL,
            model                  TEXT NOT NULL,
            max_workers            INTEGER NOT NULL,
            monthly_credit_cap_usd REAL,
            nightly_budget_usd     REAL,
            agent_bin_override     TEXT,
            fake_delay_ms          INTEGER NOT NULL,
            theme                  TEXT NOT NULL,
            ui_scale               REAL NOT NULL
        );
        -- Append-only normalized event log (DESIGN §11 source of truth).
        -- Replayable in order by (task_id, seq).
        CREATE TABLE IF NOT EXISTS agent_event (
            task_id      TEXT NOT NULL,
            seq          INTEGER NOT NULL,
            ts_ms        INTEGER NOT NULL,
            runner       TEXT NOT NULL,
            kind         TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            PRIMARY KEY (task_id, seq)
        );
        -- Task queue + lifecycle board (DESIGN §11, v1.0 spec module 5).
        CREATE TABLE IF NOT EXISTS task (
            id         TEXT PRIMARY KEY,
            project    TEXT NOT NULL,
            prompt     TEXT NOT NULL,
            mode       TEXT NOT NULL,
            status     TEXT NOT NULL,
            cost_usd   REAL,
            branch     TEXT,
            position   INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_task_project ON task(project);
        CREATE INDEX IF NOT EXISTS idx_task_status  ON task(status);",
    )?;

    // Backfill columns onto already-deployed tables. `CREATE TABLE IF NOT
    // EXISTS` above only shapes a *fresh* DB — an existing `recent_projects`
    // keeps its original (alias-less) shape, so add the column idempotently for
    // databases created before the alias feature landed.
    add_column_if_absent(conn, "recent_projects", "alias", "TEXT")?;
    // The verify-gate command (DESIGN §7) — added after the original settings table,
    // so existing DBs gain it idempotently (default '' = no real gate / always-pass).
    add_column_if_absent(conn, "settings", "verify_command", "TEXT NOT NULL DEFAULT ''")?;

    // Seed the single settings row with defaults if absent, so `get_settings`
    // always returns a complete config. `INSERT OR IGNORE` keeps it idempotent
    // and never clobbers a user's saved settings on a later open.
    seed_default_settings(conn)?;
    Ok(())
}

/// Insert the default settings row (id = 1) only if it does not already exist.
fn seed_default_settings(conn: &Connection) -> anyhow::Result<()> {
    use crate::settings::Settings;
    let d = Settings::default();
    conn.execute(
        "INSERT OR IGNORE INTO settings
            (id, default_mode, model, max_workers, monthly_credit_cap_usd,
             nightly_budget_usd, agent_bin_override, fake_delay_ms, theme, ui_scale)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            d.default_mode,
            d.model,
            d.max_workers,
            d.monthly_credit_cap_usd,
            d.nightly_budget_usd,
            d.agent_bin_override,
            d.fake_delay_ms,
            d.theme,
            d.ui_scale,
        ],
    )?;
    Ok(())
}
