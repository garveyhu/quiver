//! Append-only run history persistence.
//!
//! Backs the `run_history` table (DESIGN §11 — a pragmatic merge of the design's
//! `task` / `run_summary` / `cost_ledger` summary fields). Accessors hang off
//! [`Store`].

use rusqlite::params;
use serde::Serialize;

use crate::Store;

/// One past run, newest first.
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

impl Store {
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
}
