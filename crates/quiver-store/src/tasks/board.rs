//! 任务板生命周期 + 调度认领(§11/§12):入队、列表、改板序、删除、原子认领下一个排队任务、
//! 崩溃恢复重入队、列出有待办的项目。

use rusqlite::{params, OptionalExtension};

use super::{NewTask, TaskRecord};
use crate::Store;

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
                    position, created_at, updated_at, worker_role, attempt, parent_goal, question
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
                    question: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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
                    position, created_at, updated_at, worker_role, attempt, parent_goal, question
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
                    question: row.get(13)?,
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
                        position, created_at, updated_at, worker_role, attempt, parent_goal, question
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
                    question: row.get(13)?,
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
