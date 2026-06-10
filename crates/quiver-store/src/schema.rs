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
    // 自治开关(DESIGN §5):on=经理控制循环驱动调度(基于决策派活),off=旧 scheduler 无脑流水线。
    // 默认 0(关)——经理循环验证稳了用户再翻开,可随时回退。
    add_column_if_absent(conn, "settings", "autonomous", "INTEGER NOT NULL DEFAULT 0")?;

    // P0 crash-recovery columns on `task` (DESIGN §20 / §23 P0). The supervisor's
    // reconcile keys off these; all NULLABLE so existing rows and the current
    // INSERT/SELECT paths stay valid until orchestration wires them.
    //   session_id — backend session handle (Claude --resume token), persisted so
    //                the manager can durably resume a worker's context (DESIGN §4/§6).
    //   saga_step  — the orchestration step the task is at; the §5 single commit point.
    //   fence      — fencing token (§20 run_token) for reconcile anti-split-brain.
    //   done_at    — completion marker (ms epoch); NULL = not finalized.
    add_column_if_absent(conn, "task", "session_id", "TEXT")?;
    add_column_if_absent(conn, "task", "saga_step", "TEXT")?;
    add_column_if_absent(conn, "task", "fence", "INTEGER")?;
    add_column_if_absent(conn, "task", "done_at", "INTEGER")?;
    // §10-12 观测:agent result 报的 tokens / 真实耗时(ms),落到任务行供 get_metrics 精确聚合
    // (无则 get_metrics 退回墙钟代理 / 0)。
    add_column_if_absent(conn, "task", "tokens", "INTEGER")?;
    add_column_if_absent(conn, "task", "duration_ms", "INTEGER")?;
    // §14 按人追溯:派给哪个员工(角色名)跑的,run 时记录,追溯室/工作台据此显示"由员工X干"。
    add_column_if_absent(conn, "task", "worker_role", "TEXT")?;
    // §5 失败自愈:第几次尝试(初始 1)。验证失败时经理自动重试会 +1,到上限才停手等 CEO。
    add_column_if_absent(conn, "task", "attempt", "INTEGER NOT NULL DEFAULT 1")?;
    // §5 协作:被经理拆出来的子任务,记它属于哪个父目标(原目标文本)。追溯室据此显示协作树。
    add_column_if_absent(conn, "task", "parent_goal", "TEXT")?;
    // worker_pid:running 任务的 claude 子进程 PID,持久化(不只内存 running_pids)→ 崩溃重启后
    // reconcile 能据此 kill 上个会话遗留的孤儿 worker 进程(§23 崩溃恢复卫生)。
    add_column_if_absent(conn, "task", "worker_pid", "INTEGER")?;

    // 决策日志(DESIGN §20 decision_log 裁剪/§10 复盘室):经理每拍真实决策一行,只追加。
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS decision_log (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            project              TEXT NOT NULL,
            seq                  INTEGER NOT NULL,
            action               TEXT NOT NULL,
            reason               TEXT,
            node_id              TEXT,
            task_id              TEXT,
            task_prompt          TEXT,
            executed             INTEGER NOT NULL,
            inflight             INTEGER NOT NULL,
            queued               INTEGER NOT NULL,
            max_inflight         INTEGER NOT NULL,
            budget_remaining_usd REAL NOT NULL,
            ts_ms                INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_decision_project ON decision_log(project, ts_ms);",
    )?;

    // 人事部(DESIGN §14):角色配置表 + 内置经理/员工 seed。经理的 brain 字段是
    // 「烧钱大脑」的唯一显式开关(seed 必须是免费的 rule)。
    // 先只建表(新 DB 含 specialty 列),再幂等补列(既有 DB),**最后**才 seed —— seed 的
    // INSERT 引用了 specialty 列,必须等列存在后再跑,否则既有 DB 上 INSERT 引用不存在的列会崩。
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_role (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            kind          TEXT NOT NULL,
            brain         TEXT NOT NULL DEFAULT 'rule',
            model         TEXT NOT NULL DEFAULT 'sonnet',
            system_prompt TEXT NOT NULL DEFAULT '',
            budget_usd    REAL,
            max_turns     INTEGER,
            specialty     TEXT NOT NULL DEFAULT '',
            version       INTEGER NOT NULL DEFAULT 1,
            updated_at    INTEGER NOT NULL
        );",
    )?;
    // 既有 DB(在 specialty 列之前建的 agent_role)幂等补列(§14 每个员工的专长标签)。
    add_column_if_absent(conn, "agent_role", "specialty", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute_batch(
        "INSERT OR IGNORE INTO agent_role (id, name, kind, brain, model, updated_at)
            VALUES ('manager', '经理', 'manager', 'rule', 'sonnet', 0);
        INSERT OR IGNORE INTO agent_role (id, name, kind, brain, model, specialty, updated_at)
            VALUES ('worker', '员工 1', 'worker', 'rule', 'sonnet', '通用', 0);
        INSERT OR IGNORE INTO agent_role (id, name, kind, brain, model, specialty, updated_at)
            VALUES ('worker-2', '员工 2', 'worker', 'rule', 'sonnet', '测试', 0);
        INSERT OR IGNORE INTO agent_role (id, name, kind, brain, model, specialty, updated_at)
            VALUES ('worker-3', '员工 3', 'worker', 'rule', 'sonnet', '前端', 0);
        INSERT OR IGNORE INTO agent_role (id, name, kind, brain, model, updated_at)
            VALUES ('librarian', '记忆官', 'librarian', 'rule', 'sonnet', 0);",
    )?;
    // 改名迁移:既有 DB 里 INSERT OR IGNORE 不更新已存在行的 name,把旧的"图书管理员"
    // 一次性改成"记忆官"(幂等:只命中旧名)。
    conn.execute(
        "UPDATE agent_role SET name = '记忆官' WHERE id = 'librarian' AND name = '图书管理员'",
        [],
    )?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Column names of `table`, via PRAGMA table_info (col 1 = name).
    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare table_info");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("query table_info")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect column names")
    }

    #[test]
    fn migrate_adds_p0_task_columns_and_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        migrate(&conn).expect("first migrate");
        // Re-running migrate must not error (idempotent) and must not duplicate columns.
        migrate(&conn).expect("second migrate");

        let cols = columns(&conn, "task");
        for col in ["session_id", "saga_step", "fence", "done_at"] {
            assert!(cols.contains(&col.to_string()), "task missing column `{col}`");
        }
        // The idempotent re-run added no duplicate.
        assert_eq!(
            cols.iter().filter(|n| n.as_str() == "fence").count(),
            1,
            "fence column duplicated by re-migrate"
        );
    }
}
