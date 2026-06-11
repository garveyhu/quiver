//! 单任务字段读写(§11/§12):状态、花费+分支、会话句柄、PID、请示问题、父目标、重试次数、
//! 派给的员工角色 —— 按 id 定点改一行或读一个字段。

use rusqlite::{params, OptionalExtension};

use crate::Store;

impl Store {
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

    /// 记录 worker 主动请示的问题(§5 双向协作 worker→经理);传空串则清掉(经理已回答续跑)。
    pub fn set_task_question(&self, id: &str, question: &str, updated_at: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        let q: Option<&str> = if question.is_empty() { None } else { Some(question) };
        conn.execute(
            "UPDATE task SET question = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, q, updated_at],
        )?;
        Ok(())
    }

    /// 读 worker 请示的问题(没请示 → None)。spawn_worker 据此给 PendingReview 带上问题给经理看。
    pub fn task_question(&self, id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().expect("store lock");
        let q = conn
            .query_row("SELECT question FROM task WHERE id = ?1", params![id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten();
        Ok(q)
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

}
