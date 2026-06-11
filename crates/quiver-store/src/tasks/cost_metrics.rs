//! 统计 / 成本 / 指标(§10-12):任务计数与验收率、按窗口/项目的花费聚合、per-task 指标写入与
//! 样本读取(给观测面板)。

use rusqlite::params;

use super::MetricSample;
use crate::Store;

impl Store {
    /// Aggregate progression stats over the whole `task` table (all projects):
    /// `(total, verified, failed, total_cost_usd)`. Read-only — used by the
    /// `get_stats` IPC to drive the XP / level HUD. Verified counts `verified`
    /// + `done`; failed counts `failed` + `verify_failed`.
    pub fn task_stats(&self) -> anyhow::Result<(i64, i64, i64, f64)> {
        let conn = self.conn.lock().expect("store lock");
        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status IN ('verified','done') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status IN ('failed','verify_failed') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(cost_usd), 0.0)
             FROM task",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        Ok(row)
    }

    /// Like [`task_stats`](Self::task_stats) but only over tasks created at/after
    /// `since_ms` — a rolling window. Drives the "昨晚" morning report (last 24h)
    /// so its counts reflect the night, not all-time. Returns
    /// `(total, verified, failed, cost_usd)`.
    pub fn task_stats_since(&self, since_ms: i64) -> anyhow::Result<(i64, i64, i64, f64)> {
        let conn = self.conn.lock().expect("store lock");
        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status IN ('verified','done') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status IN ('failed','verify_failed') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(cost_usd), 0.0)
             FROM task WHERE created_at >= ?1",
            params![since_ms],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        Ok(row)
    }

    /// Total dollars spent on a single project (sum of `cost_usd` over its tasks).
    pub fn project_cost(&self, project: &str) -> anyhow::Result<f64> {
        let conn = self.conn.lock().expect("store lock");
        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM task WHERE project = ?1",
            params![project],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    /// Total dollars spent across ALL projects. Drives the §10 budget gate: the
    /// `monthly_credit_cap_usd` / `nightly_budget_usd` settings are global caps,
    /// so the gate must compare against global spend (not one project's).
    pub fn total_cost(&self) -> anyhow::Result<f64> {
        let conn = self.conn.lock().expect("store lock");
        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM task",
            [],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    /// Dollars spent across all projects on tasks created at/after `since_ms`.
    /// Drives the §10 budget gate with a ROLLING window (nightly = last 24h,
    /// monthly ≈ last 30d) — far more correct than an all-time sum, which would
    /// pause forever once lifetime spend ever exceeded a periodic cap.
    pub fn cost_since(&self, since_ms: i64) -> anyhow::Result<f64> {
        let conn = self.conn.lock().expect("store lock");
        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM task WHERE created_at >= ?1",
            params![since_ms],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    /// Persist a task's agent-reported run metrics (§10-12): tokens + real duration(ms).
    /// `None` leaves the column untouched-as-NULL on first write; get_metrics falls back
    /// to a wall-clock duration proxy / 0 tokens when absent.
    pub fn set_task_metrics(
        &self,
        id: &str,
        tokens: Option<i64>,
        duration_ms: Option<i64>,
        updated_at: i64,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET tokens = ?2, duration_ms = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, tokens, duration_ms, updated_at],
        )?;
        Ok(())
    }

    /// Per-task samples for §10-12 metrics: just the fields the aggregator needs, so this
    /// stays independent of [`TaskRecord`]'s board shape. One row per task (all statuses;
    /// the caller filters to terminal ones).
    pub fn metric_samples(&self) -> anyhow::Result<Vec<MetricSample>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT cost_usd, tokens, duration_ms, created_at, updated_at, status FROM task",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(MetricSample {
                    cost_usd: r.get(0)?,
                    tokens: r.get(1)?,
                    duration_ms: r.get(2)?,
                    created_at: r.get(3)?,
                    updated_at: r.get(4)?,
                    status: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

}
