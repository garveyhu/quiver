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
                    position, created_at, updated_at
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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

    /// Set a task's board `position` (drag-to-reorder). Lower = nearer the top.
    pub fn reorder_task(&self, id: &str, position: i64, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE task SET position = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, position, updated_at],
        )?;
        Ok(())
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
}
