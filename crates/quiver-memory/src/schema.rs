//! Idempotent SQLite schema migration for the memory store (DESIGN §20).
//!
//! [`migrate`] runs on every [`MemoryStore::open`](crate::MemoryStore) and is safe
//! to re-run. The single growth point — new memory tables (FTS5 `memory_fact_fts`,
//! vector `memory_fact_vec`, `entity_path`, `memory_staging`, `reflection`) get
//! their `CREATE … IF NOT EXISTS` here as later P1/P2 slices land.

use rusqlite::Connection;

/// Create the P1 memory tables if absent. Append-only foundation (DESIGN §6.2/§6.3):
/// `episode` (mechanical run records) + `memory_fact` (bi-temporal facts). The
/// invalidation/supersede machinery is P2 (§6.4) — the columns exist now so P2
/// adds only behavior. Safe to run on every open.
pub(crate) fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS episode (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            project       TEXT NOT NULL,
            node_id       TEXT,
            task_id       TEXT,
            commit_sha    TEXT,
            merge_seq     INTEGER,
            verify_result TEXT,
            diff_stat     TEXT,
            summary       TEXT,
            created_at    INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_episode_project ON episode(project);
        CREATE INDEX IF NOT EXISTS idx_episode_task    ON episode(task_id);

        -- Bi-temporal, append-only semantic facts (DESIGN §6.3, Zep-style). Current
        -- truth = `invalid_at IS NULL`; history follows `superseded_by`. Never
        -- DELETEd. `entities` is a JSON array of entity strings. `trust` is the
        -- §6.2 credibility tier — a CHECK guards against bogus tiers leaking in.
        CREATE TABLE IF NOT EXISTS memory_fact (
            id                     INTEGER PRIMARY KEY AUTOINCREMENT,
            project                TEXT NOT NULL,
            scope                  TEXT NOT NULL DEFAULT '本仓库',  -- 本仓库 / 全公司 (§19)
            kind                   TEXT NOT NULL,                   -- decision / 状态 / …
            text                   TEXT NOT NULL,
            entities               TEXT,                            -- JSON array
            entity                 TEXT,                            -- 标量实体(§6.4 状态唯一约束键)
            importance             INTEGER NOT NULL DEFAULT 5,
            valid_at               INTEGER,                         -- 现实成立
            invalid_at             INTEGER,                         -- 现实失效 (NULL=当前真相)
            recorded_at            INTEGER NOT NULL,                -- 我们记下
            retired_at             INTEGER,                         -- 我们撤下
            superseded_by          INTEGER REFERENCES memory_fact(id),
            invalidated_by_episode INTEGER REFERENCES episode(id),
            provenance             TEXT,
            trust                  TEXT NOT NULL
                CHECK (trust IN ('权威','已验证·机械','已验证·印证','员工汇报','不可信')),
            embed_model            TEXT,
            embed_dim              INTEGER,
            source_commit          TEXT,
            source_episode_id      INTEGER REFERENCES episode(id)
        );
        -- §20 hot-path indexes: current-truth lookups by scope/kind, + supersede chains.
        CREATE INDEX IF NOT EXISTS idx_fact_current
            ON memory_fact(project, scope, kind, invalid_at);
        CREATE INDEX IF NOT EXISTS idx_fact_superseded
            ON memory_fact(superseded_by);

        -- FTS5 full-text index over fact text (DESIGN §20 memory_fact_fts).
        -- External-content (content='memory_fact') so text isn't duplicated; an
        -- AFTER INSERT trigger keeps it in sync. Append-only → ONLY an insert
        -- trigger is needed: P2 invalidation sets `invalid_at` (a logical filter on
        -- read), it never deletes the row, so no delete/update trigger (§20 note).
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_fact_fts
            USING fts5(text, content='memory_fact', content_rowid='id');
        CREATE TRIGGER IF NOT EXISTS memory_fact_ai AFTER INSERT ON memory_fact BEGIN
            INSERT INTO memory_fact_fts(rowid, text) VALUES (new.id, new.text);
        END;

        -- §20 memory_staging:员工/经理写入的事实先进待审区(默认不可信),由图书管理员
        -- 审核后 promote 进 memory_fact(§6.4 待审/隔离)。promoted=0 待审 / 1 已晋升。
        -- 绝不自动塞进当前真相——只追加,经理主动查才以\"未核实\"出现。
        CREATE TABLE IF NOT EXISTS memory_staging (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            project        TEXT NOT NULL,
            kind           TEXT NOT NULL,
            text           TEXT NOT NULL,
            entities       TEXT,
            writer_node_id TEXT,
            created_at     INTEGER NOT NULL,
            promoted       INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_staging_pending ON memory_staging(project, promoted);

        -- §6.2 机械印证集合:一条事实被哪些 episode(独立绿测试)印证过。复合主键 =
        -- 集合语义(同一 (fact,episode) 重放被忽略):记集合而非计数,所以重放幂等。
        -- 同一事实印证 episode 数 ≥ 2 时,Rust 把它升到「已验证·印证」(AI 碰不到授予)。
        CREATE TABLE IF NOT EXISTS fact_corroboration (
            fact_id    INTEGER NOT NULL REFERENCES memory_fact(id),
            episode_id INTEGER NOT NULL REFERENCES episode(id),
            PRIMARY KEY (fact_id, episode_id)
        );",
    )?;

    // Backfill the scalar `entity` column onto memory_fact for DBs created before it
    // (CREATE TABLE IF NOT EXISTS won't reshape an existing table). Idempotent.
    add_column_if_absent(conn, "memory_fact", "entity", "TEXT")?;

    // §6.4 hard rule: at most ONE current 状态 fact per (project, entity) — the
    // database rejects a second. A partial unique index (only current 状态 rows)
    // enforces it; supersede must retire-old-before-insert-new so the swap never
    // trips it (see `supersede_fact`).
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_fact_current_state
            ON memory_fact(project, entity)
            WHERE kind = '状态' AND invalid_at IS NULL;",
    )?;
    Ok(())
}

/// Add `column` (`decl` = its SQL type) to `table` only if absent (SQLite has no
/// `ADD COLUMN IF NOT EXISTS`; probe `PRAGMA table_info` first). Keeps `migrate`
/// idempotent across re-opens and across DBs created before the column existed.
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
    fn migrate_creates_tables_idempotently() {
        let conn = Connection::open_in_memory().expect("open");
        migrate(&conn).expect("first migrate");
        migrate(&conn).expect("second migrate (idempotent)");

        let ep = columns(&conn, "episode");
        for c in ["project", "commit_sha", "merge_seq", "verify_result", "created_at"] {
            assert!(ep.contains(&c.to_string()), "episode missing `{c}`");
        }
        let mf = columns(&conn, "memory_fact");
        for c in [
            "text",
            "entities",
            "valid_at",
            "invalid_at",
            "superseded_by",
            "trust",
            "embed_dim",
            "source_episode_id",
        ] {
            assert!(mf.contains(&c.to_string()), "memory_fact missing `{c}`");
        }
    }

    #[test]
    fn trust_check_rejects_unknown_tier() {
        let conn = Connection::open_in_memory().expect("open");
        migrate(&conn).expect("migrate");
        // A valid tier inserts.
        conn.execute(
            "INSERT INTO memory_fact(project,kind,text,recorded_at,trust)
             VALUES('p','decision','ok',1,'权威')",
            [],
        )
        .expect("valid trust tier inserts");
        // A bogus tier is rejected by the CHECK constraint.
        let bad = conn.execute(
            "INSERT INTO memory_fact(project,kind,text,recorded_at,trust)
             VALUES('p','decision','no',1,'made-up')",
            [],
        );
        assert!(bad.is_err(), "unknown trust tier must be rejected by CHECK");
    }

    #[test]
    fn fact_defaults_current_truth_and_repo_scope() {
        let conn = Connection::open_in_memory().expect("open");
        migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO memory_fact(project,kind,text,recorded_at,trust)
             VALUES('p','decision','t',1,'员工汇报')",
            [],
        )
        .expect("insert");
        // Append-only default: invalid_at NULL (current truth), scope 本仓库.
        let (scope, invalid_at): (String, Option<i64>) = conn
            .query_row(
                "SELECT scope, invalid_at FROM memory_fact WHERE text='t'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read back");
        assert_eq!(scope, "本仓库");
        assert_eq!(invalid_at, None, "a fresh fact is current truth (invalid_at NULL)");
    }
}
