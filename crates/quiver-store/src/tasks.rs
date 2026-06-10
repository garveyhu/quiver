//! Task queue + lifecycle persistence (DESIGN §11 `task`, v1.0 spec module 5 —
//! the "bulletin board / task queue").
//!
//! Backs the `task` table: one row per task (the core-equation unit), carrying
//! its queue position, lifecycle status, cost, and attempt branch. This
//! supersedes the older `run_history` summary by being the live, mutable record
//! a task moves through (queued → running → verifying → done/failed/…), where
//! `run_history` only ever recorded *finished* runs. `run_history` is kept
//! intact and still written, so nothing that reads it breaks; `task` is the new
//! growth point the bulletin-board UI (Phase C) and the archive (Phase D) build
//! on.
//!
//! Ordering is "by `position` then `created_at`": the board is hand-orderable
//! (drag-to-reorder writes `position`), and ties / un-positioned rows fall back
//! to insertion order.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::Store;

/// A queued / in-flight / finished task (DESIGN §11 `task`).
///
/// `status` is the lifecycle vocabulary from the v1.0 spec module 5:
/// `queued | running | verifying | done | failed | needs_rebase`. Serialized
/// camelCase for the board UI.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub project: String,
    pub prompt: String,
    /// `"simulate"` | `"real"` (§4.2).
    pub mode: String,
    /// `queued | running | verifying | done | failed | needs_rebase`.
    pub status: String,
    pub cost_usd: Option<f64>,
    /// The attempt branch the work lives on, when kept un-merged (real-mode).
    pub branch: Option<String>,
    /// Hand-orderable board position (lower = nearer the top of the queue).
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// 派给哪个员工(角色名)跑的(§14 按人追溯);`None` = 没记录(旧任务/未派)。
    pub worker_role: Option<String>,
    pub attempt: i64,
    /// 父目标(§5 协作):子任务属于哪个被拆的原目标;None=不是子任务。
    pub parent_goal: Option<String>,
}

/// A per-task observability sample (§10-12), independent of the board-facing
/// [`TaskRecord`]. `tokens` / `duration_ms` are the agent-reported run metrics
/// (`None` until a run persists them).
#[derive(Clone, Debug, PartialEq)]
pub struct MetricSample {
    pub cost_usd: Option<f64>,
    pub tokens: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: String,
}

/// The fields needed to enqueue a new task (id + project + prompt + mode + the
/// caller's timestamp). Status starts `queued` and `position` defaults to the
/// end of the board unless the caller pins one.
#[derive(Clone, Debug)]
pub struct NewTask {
    pub id: String,
    pub project: String,
    pub prompt: String,
    pub mode: String,
    /// Initial status. Callers that enqueue-then-run immediately pass
    /// `"running"`; a board "pin a task for later" passes `"queued"`.
    pub status: String,
    pub created_at: i64,
}

impl Store {
    /// Enqueue a task. Its `position` defaults to one past the current max so a
    /// new task lands at the bottom of the board; `updated_at` mirrors
    /// `created_at` at insert. Idempotent on `id` (re-enqueue updates the row's
    /// mutable fields rather than erroring).
    pub fn enqueue_task(&self, task: &NewTask) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        // Next position = (max existing) + 1, or 0 for the first task.
        let next_position: i64 = conn
            .query_row("SELECT COALESCE(MAX(position) + 1, 0) FROM task", [], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO task
                (id, project, prompt, mode, status, cost_usd, branch,
                 position, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7, ?7)
             ON CONFLICT(id) DO UPDATE SET
                project    = excluded.project,
                prompt     = excluded.prompt,
                mode       = excluded.mode,
                status     = excluded.status,
                updated_at = excluded.updated_at",
            params![
                task.id,
                task.project,
                task.prompt,
                task.mode,
                task.status,
                next_position,
                task.created_at,
            ],
        )?;
        Ok(())
    }

    /// List tasks, ordered by `position` then `created_at` (board order).
    /// Optionally filter by project and/or status (`None` = no filter on that
    /// dimension).
    pub fn list_tasks(
        &self,
        project: Option<&str>,
        status: Option<&str>,
    ) -> anyhow::Result<Vec<TaskRecord>> {
        let conn = self.conn.lock().expect("store lock");
        // Build the WHERE clause from the present filters. Two optional
        // predicates → at most two bound params, kept positional for clarity.
        let mut sql = String::from(
            "SELECT id, project, prompt, mode, status, cost_usd, branch,
                    position, created_at, updated_at, worker_role, attempt, parent_goal
             FROM task",
        );
        let mut clauses: Vec<&str> = Vec::new();
        let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
        if let Some(p) = &project {
            clauses.push("project = ?");
            binds.push(p);
        }
        if let Some(s) = &status {
            clauses.push("status = ?");
            binds.push(s);
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            // rusqlite uses ?N positional params; rewrite the placeholders.
            let joined = clauses
                .iter()
                .enumerate()
                .map(|(i, c)| c.replacen('?', &format!("?{}", i + 1), 1))
                .collect::<Vec<_>>()
                .join(" AND ");
            sql.push_str(&joined);
        }
        sql.push_str(" ORDER BY position ASC, created_at ASC");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(binds.as_slice(), |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    prompt: row.get(2)?,
                    mode: row.get(3)?,
                    status: row.get(4)?,
                    cost_usd: row.get(5)?,
                    branch: row.get(6)?,
                    position: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    worker_role: row.get(10)?,
                    attempt: row.get(11)?,
                    parent_goal: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

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

    /// Move a task to a new lifecycle `status`, stamping `updated_at`.
    pub fn update_task_status(
        &self,
        id: &str,
        status: &str,
        updated_at: i64,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status, updated_at],
        )?;
        Ok(())
    }

    /// Record a finished task's summed cost and (un-merged) branch, stamping
    /// `updated_at`. Either field may be `None` (no dollars reported / merged).
    pub fn set_task_cost_branch(
        &self,
        id: &str,
        cost_usd: Option<f64>,
        branch: Option<&str>,
        updated_at: i64,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET cost_usd = ?2, branch = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, cost_usd, branch, updated_at],
        )?;
        Ok(())
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

    /// Persist a task's backend session handle (Claude `--resume` token),
    /// stamping `updated_at`. Threaded from the `WorkerStarted` event so the
    /// manager can durably resume this worker's context after a restart
    /// (DESIGN §4/§6 — pairs with the §20 `session_id` column).
    pub fn set_task_session_id(
        &self,
        id: &str,
        session_id: &str,
        updated_at: i64,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET session_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, session_id, updated_at],
        )?;
        Ok(())
    }

    /// 记录 running 任务的 worker 子进程 PID(持久化,§23 崩溃恢复:重启后据此 kill 孤儿)。
    pub fn set_task_pid(&self, id: &str, pid: i64, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET worker_pid = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, pid, updated_at],
        )?;
        Ok(())
    }

    /// 列出所有 `running` 任务记着的 worker PID(崩溃重启后,这些就是要清的孤儿进程候选)。
    pub fn running_task_pids(&self) -> anyhow::Result<Vec<(String, i64)>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, worker_pid FROM task WHERE status = 'running' AND worker_pid IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// 记录子任务属于哪个父目标(§5 协作:经理拆活时,子任务标上原目标文本,追溯室画协作树)。
    pub fn set_task_parent_goal(&self, id: &str, goal: &str, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET parent_goal = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, goal, updated_at],
        )?;
        Ok(())
    }

    /// 设置任务的尝试次数(§5 失败自愈:重试任务记 attempt+1)。
    pub fn set_task_attempt(&self, id: &str, attempt: i64, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET attempt = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, attempt, updated_at],
        )?;
        Ok(())
    }

    /// 记录任务派给了哪个员工(角色名,§14 按人追溯)。run 启动时调一次。
    pub fn set_task_worker_role(&self, id: &str, role_name: &str, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET worker_role = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, role_name, updated_at],
        )?;
        Ok(())
    }

    /// Read back a task's persisted session handle, or `None` if the task is gone
    /// or never reported one. The reconcile path reads this to resume in-flight
    /// workers (DESIGN §23 P0).
    pub fn task_session_id(&self, id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().expect("store lock");
        let got = conn
            .query_row(
                "SELECT session_id FROM task WHERE id = ?1",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        Ok(got)
    }

    /// Crash recovery (DESIGN §23 P0): flip every task left `running` by a
    /// previous session back to `queued`, stamping `updated_at`. After a restart
    /// no live worker owns these rows, so requeuing lets the dispatcher pick them
    /// up again (the persisted `session_id` is preserved for a future `--resume`).
    /// Returns how many rows were requeued.
    pub fn requeue_running_tasks(&self, updated_at: i64) -> anyhow::Result<usize> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute(
            // 清 worker_pid:重跑会起新进程,旧 PID 已由 reconcile kill 过(或进程已死),别留着误杀。
            "UPDATE task SET status = 'queued', worker_pid = NULL, updated_at = ?1 WHERE status = 'running'",
            params![updated_at],
        )?;
        Ok(n)
    }

    /// Distinct project paths that still have `queued` work — the set whose
    /// dispatchers reconcile must (re)start on launch. Ordered for determinism.
    pub fn projects_with_pending_tasks(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT DISTINCT project FROM task WHERE status = 'queued' ORDER BY project",
        )?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Set a task's board `position` (drag-to-reorder). Lower = nearer the top.
    pub fn reorder_task(&self, id: &str, position: i64, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET position = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, position, updated_at],
        )?;
        Ok(())
    }

    /// Remove a task from the board (cancel an un-started commission). The
    /// scheduler only ever cancels `queued` tasks via the command surface, but
    /// the delete itself is unconditional on `id` — its caller enforces the
    /// "queued-only" rule so a running task is never yanked out from under a
    /// live worker.
    pub fn delete_task(&self, id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute("DELETE FROM task WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Fetch a single task by id, or `None` if it is gone (e.g. cancelled). Used
    /// by the scheduler to re-check a task's current status before claiming it.
    pub fn get_task(&self, id: &str) -> anyhow::Result<Option<TaskRecord>> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            "SELECT id, project, prompt, mode, status, cost_usd, branch,
                    position, created_at, updated_at, worker_role, attempt, parent_goal
             FROM task WHERE id = ?1",
            params![id],
            |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    prompt: row.get(2)?,
                    mode: row.get(3)?,
                    status: row.get(4)?,
                    cost_usd: row.get(5)?,
                    branch: row.get(6)?,
                    position: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    worker_role: row.get(10)?,
                    attempt: row.get(11)?,
                    parent_goal: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Atomically claim the next `queued` task (lowest `position`, then oldest)
    /// for a project, flipping it to `running` in the same transaction so two
    /// concurrent scheduler ticks can never claim the same row. Returns the
    /// claimed [`TaskRecord`] (already `running`), or `None` if the queue is
    /// empty.
    ///
    /// The claim-and-flip is done under the connection mutex via an immediate
    /// transaction: SELECT the candidate, UPDATE it to `running`, commit. Because
    /// the whole store is serialized behind one `Mutex<Connection>`, this is the
    /// single point that hands a task to exactly one worker.
    pub fn claim_next_queued(
        &self,
        project: &str,
        updated_at: i64,
    ) -> anyhow::Result<Option<TaskRecord>> {
        let mut conn = self.conn.lock().expect("store lock");
        let tx = conn.transaction()?;
        let candidate: Option<TaskRecord> = tx
            .query_row(
                "SELECT id, project, prompt, mode, status, cost_usd, branch,
                        position, created_at, updated_at, worker_role, attempt, parent_goal
                 FROM task
                 WHERE project = ?1 AND status = 'queued'
                 ORDER BY position ASC, created_at ASC
                 LIMIT 1",
                params![project],
                |row| {
                    Ok(TaskRecord {
                        id: row.get(0)?,
                        project: row.get(1)?,
                        prompt: row.get(2)?,
                        mode: row.get(3)?,
                        status: row.get(4)?,
                        cost_usd: row.get(5)?,
                        branch: row.get(6)?,
                        position: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    worker_role: row.get(10)?,
                    attempt: row.get(11)?,
                    parent_goal: row.get(12)?,
                    })
                },
            )
            .optional()?;
        let Some(mut task) = candidate else {
            tx.commit()?;
            return Ok(None);
        };
        tx.execute(
            "UPDATE task SET status = 'running', updated_at = ?2 WHERE id = ?1",
            params![task.id, updated_at],
        )?;
        tx.commit()?;
        task.status = "running".to_string();
        task.updated_at = updated_at;
        Ok(Some(task))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_task(id: &str, project: &str, prompt: &str, status: &str, ts: i64) -> NewTask {
        NewTask {
            id: id.to_string(),
            project: project.to_string(),
            prompt: prompt.to_string(),
            mode: "simulate".to_string(),
            status: status.to_string(),
            created_at: ts,
        }
    }

    #[test]
    fn enqueue_assigns_increasing_positions() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("t1", "/r", "first", "queued", 100)).unwrap();
        store.enqueue_task(&new_task("t2", "/r", "second", "queued", 200)).unwrap();
        store.enqueue_task(&new_task("t3", "/r", "third", "queued", 300)).unwrap();

        let tasks = store.list_tasks(None, None).unwrap();
        assert_eq!(tasks.len(), 3);
        // Board order = position ascending = insertion order here.
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(tasks[1].id, "t2");
        assert_eq!(tasks[2].id, "t3");
        assert!(tasks[0].position < tasks[1].position);
        assert!(tasks[1].position < tasks[2].position);
    }

    #[test]
    fn crash_recovery_tracks_and_clears_worker_pid() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("t1", "/r", "p", "queued", 100)).unwrap();
        // worker 跑起来:标 running + 持久化它的子进程 PID。
        store.update_task_status("t1", "running", 200).unwrap();
        store.set_task_pid("t1", 4242, 200).unwrap();
        // reconcile 能从 DB 查到 running 任务的 PID(崩溃后据此 kill 孤儿 worker)。
        assert_eq!(store.running_task_pids().unwrap(), vec![("t1".to_string(), 4242)]);
        // requeue 把 running → queued 并清 PID(重跑起新进程,旧 PID 不留着免得误杀)。
        store.requeue_running_tasks(300).unwrap();
        assert!(
            store.running_task_pids().unwrap().is_empty(),
            "requeue 后不应再有 running PID"
        );
    }

    #[test]
    fn list_filters_by_project_and_status() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("a", "/alpha", "p", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("b", "/beta", "p", "queued", 2)).unwrap();
        store.enqueue_task(&new_task("c", "/alpha", "p", "running", 3)).unwrap();

        assert_eq!(store.list_tasks(Some("/alpha"), None).unwrap().len(), 2);
        assert_eq!(store.list_tasks(Some("/beta"), None).unwrap().len(), 1);
        assert_eq!(store.list_tasks(None, Some("queued")).unwrap().len(), 2);
        let running_alpha = store.list_tasks(Some("/alpha"), Some("running")).unwrap();
        assert_eq!(running_alpha.len(), 1);
        assert_eq!(running_alpha[0].id, "c");
    }

    #[test]
    fn status_and_cost_branch_updates_round_trip() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("t", "/r", "p", "running", 100)).unwrap();

        store.update_task_status("t", "verifying", 150).unwrap();
        store.set_task_cost_branch("t", Some(0.42), Some("quiver/task-t/attempt-1"), 200).unwrap();
        store.update_task_status("t", "done", 250).unwrap();

        let t = &store.list_tasks(None, None).unwrap()[0];
        assert_eq!(t.status, "done");
        assert_eq!(t.cost_usd, Some(0.42));
        assert_eq!(t.branch.as_deref(), Some("quiver/task-t/attempt-1"));
        assert_eq!(t.updated_at, 250, "updated_at tracks the latest mutation");
    }

    #[test]
    fn reorder_changes_board_order() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("t1", "/r", "p", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("t2", "/r", "p", "queued", 2)).unwrap();
        // Drag t2 to the very top.
        store.reorder_task("t2", -1, 99).unwrap();
        let order: Vec<String> = store
            .list_tasks(None, None)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(order, vec!["t2", "t1"]);
    }

    #[test]
    fn reenqueue_same_id_updates_in_place() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("t", "/r", "old", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("t", "/r", "new", "running", 2)).unwrap();
        let tasks = store.list_tasks(None, None).unwrap();
        assert_eq!(tasks.len(), 1, "same id must not duplicate");
        assert_eq!(tasks[0].prompt, "new");
        assert_eq!(tasks[0].status, "running");
    }

    #[test]
    fn delete_removes_a_queued_task() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("keep", "/r", "p", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("drop", "/r", "p", "queued", 2)).unwrap();
        store.delete_task("drop").unwrap();
        let ids: Vec<String> = store
            .list_tasks(None, None)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec!["keep"]);
        assert!(store.get_task("drop").unwrap().is_none());
        assert!(store.get_task("keep").unwrap().is_some());
    }

    #[test]
    fn claim_next_queued_flips_to_running_in_board_order() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("t1", "/r", "first", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("t2", "/r", "second", "queued", 2)).unwrap();

        let claimed = store.claim_next_queued("/r", 50).unwrap().unwrap();
        assert_eq!(claimed.id, "t1", "lowest position claimed first");
        assert_eq!(claimed.status, "running", "claim flips to running");
        // The stored row is now running too — a second claim picks t2, not t1.
        assert_eq!(store.get_task("t1").unwrap().unwrap().status, "running");
        let next = store.claim_next_queued("/r", 60).unwrap().unwrap();
        assert_eq!(next.id, "t2");
    }

    #[test]
    fn claim_next_queued_is_project_scoped_and_empties() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("a", "/alpha", "p", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("b", "/beta", "p", "queued", 2)).unwrap();

        // No queued task for an unrelated project.
        assert!(store.claim_next_queued("/gamma", 10).unwrap().is_none());
        // Claims only from the asked-for project.
        assert_eq!(store.claim_next_queued("/alpha", 11).unwrap().unwrap().id, "a");
        // /alpha now empty; /beta still has one.
        assert!(store.claim_next_queued("/alpha", 12).unwrap().is_none());
        assert_eq!(store.claim_next_queued("/beta", 13).unwrap().unwrap().id, "b");
    }

    #[test]
    fn tasks_persist_across_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = Store::default_db_path(dir.path());
        {
            let store = Store::open(&path).unwrap();
            store.enqueue_task(&new_task("kept", "/r", "p", "done", 1)).unwrap();
            store.set_task_cost_branch("kept", Some(1.5), None, 2).unwrap();
        }
        let reopened = Store::open(&path).unwrap();
        let tasks = reopened.list_tasks(None, None).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].cost_usd, Some(1.5));
    }

    #[test]
    fn project_cost_sums_per_project() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("a", "/alpha", "p", "done", 1)).unwrap();
        store.enqueue_task(&new_task("b", "/alpha", "p", "done", 2)).unwrap();
        store.enqueue_task(&new_task("c", "/beta", "p", "done", 3)).unwrap();
        // No costs yet → 0.
        assert_eq!(store.project_cost("/alpha").unwrap(), 0.0);
        store.set_task_cost_branch("a", Some(0.4), None, 4).unwrap();
        store.set_task_cost_branch("b", Some(0.6), None, 5).unwrap();
        store.set_task_cost_branch("c", Some(9.0), None, 6).unwrap();
        // Sums only the queried project; /beta's $9 must not leak into /alpha.
        assert!((store.project_cost("/alpha").unwrap() - 1.0).abs() < 1e-9);
        assert!((store.project_cost("/beta").unwrap() - 9.0).abs() < 1e-9);
        assert_eq!(store.project_cost("/nonexistent").unwrap(), 0.0);
        // total_cost spans all projects.
        assert!((store.total_cost().unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn cost_since_windows_by_created_at() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("old", "/r", "p", "done", 1_000)).unwrap();
        store.enqueue_task(&new_task("new", "/r", "p", "done", 9_000)).unwrap();
        store.set_task_cost_branch("old", Some(2.0), None, 1_001).unwrap();
        store.set_task_cost_branch("new", Some(3.0), None, 9_001).unwrap();
        // Window start after "old" but before "new" → only "new" counts.
        assert!((store.cost_since(5_000).unwrap() - 3.0).abs() < 1e-9);
        // Window covering both.
        assert!((store.cost_since(0).unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn task_stats_since_windows_by_created_at() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_task(&new_task("old", "/r", "p", "verified", 1)).unwrap();
        store.enqueue_task(&new_task("new", "/r", "p", "failed", 9_000)).unwrap();
        // Window after "old", before "new" → only "new" counts (1 total, 0 verified, 1 failed).
        assert_eq!(store.task_stats_since(5_000).unwrap().0, 1);
        let (total, verified, failed, _) = store.task_stats_since(5_000).unwrap();
        assert_eq!((total, verified, failed), (1, 0, 1));
        // Window covering both.
        let (t2, v2, f2, _) = store.task_stats_since(0).unwrap();
        assert_eq!((t2, v2, f2), (2, 1, 1));
    }

    #[test]
    fn session_id_round_trips_and_survives_reopen() {
        let file = tempfile::NamedTempFile::new().unwrap();
        {
            let store = Store::open(file.path()).unwrap();
            store.enqueue_task(&new_task("t", "/r", "p", "running", 1)).unwrap();
            // Unset until the WorkerStarted event reports one.
            assert_eq!(store.task_session_id("t").unwrap(), None);
            store.set_task_session_id("t", "sess-abc", 2).unwrap();
            assert_eq!(store.task_session_id("t").unwrap(), Some("sess-abc".to_string()));
        }
        // Persisted across reopen — the manager can resume after a restart.
        let store = Store::open(file.path()).unwrap();
        assert_eq!(store.task_session_id("t").unwrap(), Some("sess-abc".to_string()));
        // A missing task reads None, not an error.
        assert_eq!(store.task_session_id("nope").unwrap(), None);
    }

    #[test]
    fn reconcile_requeues_running_and_lists_pending_projects() {
        let store = Store::open_in_memory().unwrap();
        // A queued task, an interrupted running task, and a finished one.
        store.enqueue_task(&new_task("q", "/alpha", "p", "queued", 1)).unwrap();
        store.enqueue_task(&new_task("r", "/beta", "p", "running", 2)).unwrap();
        store.enqueue_task(&new_task("d", "/beta", "p", "done", 3)).unwrap();

        // Requeue flips only the `running` row → queued; the done row is untouched.
        assert_eq!(store.requeue_running_tasks(10).unwrap(), 1);
        assert_eq!(store.get_task("r").unwrap().unwrap().status, "queued");
        assert_eq!(store.get_task("d").unwrap().unwrap().status, "done");

        // Both projects now have queued work; the finished-only project would not
        // appear. Distinct + ordered.
        assert_eq!(
            store.projects_with_pending_tasks().unwrap(),
            vec!["/alpha".to_string(), "/beta".to_string()]
        );

        // Idempotent: a second requeue with nothing running affects 0 rows.
        assert_eq!(store.requeue_running_tasks(11).unwrap(), 0);
    }
}
