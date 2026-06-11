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

use serde::Serialize;

// 按职责拆出的子模块(单一职责):任务板生命周期+调度、统计/成本/指标、单任务字段读写。
// impl Store 跨文件(同 crate,conn 私有字段对后代可见);struct 留本文件 → re-export 不变。
mod board;
mod cost_metrics;
mod fields;

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
    /// worker 主动请示的问题(§5 双向协作 worker→经理);None=没请示。
    pub question: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

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
