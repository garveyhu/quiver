//! 事实读路径(§6.3):当前真相事实(invalid_at IS NULL)、FTS5 关键词检索、状态事实读取。

use rusqlite::{params, OptionalExtension};

use super::FactRecord;
use crate::MemoryStore;

impl MemoryStore {
    /// Current-truth facts for `project` (`invalid_at IS NULL`), most important and
    /// most recent first (DESIGN §6.3 — current truth is the un-retired set). The
    /// §6.7 recall ranking (recency·importance·trust + FTS/vector) is a later slice;
    /// this is the simple importance/recency order brief reads from for now.
    pub fn current_facts(&self, project: &str) -> anyhow::Result<Vec<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, scope, kind, text, entities, importance,
                    valid_at, invalid_at, recorded_at, trust, entity
             FROM memory_fact
             WHERE project = ?1 AND invalid_at IS NULL
             ORDER BY importance DESC, recorded_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map(params![project], |row| {
                Ok(FactRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    scope: row.get(2)?,
                    kind: row.get(3)?,
                    text: row.get(4)?,
                    entities: row.get(5)?,
                    importance: row.get(6)?,
                    valid_at: row.get(7)?,
                    invalid_at: row.get(8)?,
                    recorded_at: row.get(9)?,
                    trust: row.get(10)?,
                    entity: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Full-text search current-truth facts for `project` by `query` (FTS5 MATCH,
    /// DESIGN §20), best relevance first. Retired facts (`invalid_at` set) and other
    /// projects are excluded. `query` is an FTS5 query string — callers usually pass
    /// plain keywords. The §6.7 hybrid recall (FTS + vector + recency/importance/
    /// trust weighting) builds on this; for now it's the keyword leg.
    pub fn search_facts(&self, project: &str, query: &str) -> anyhow::Result<Vec<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT mf.id, mf.project, mf.scope, mf.kind, mf.text, mf.entities,
                    mf.importance, mf.valid_at, mf.invalid_at, mf.recorded_at, mf.trust,
                    mf.entity
             FROM memory_fact_fts
             JOIN memory_fact mf ON mf.id = memory_fact_fts.rowid
             WHERE memory_fact_fts MATCH ?1
               AND mf.project = ?2
               AND mf.invalid_at IS NULL
             ORDER BY memory_fact_fts.rank",
        )?;
        let rows = stmt
            .query_map(params![query, project], |row| {
                Ok(FactRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    scope: row.get(2)?,
                    kind: row.get(3)?,
                    text: row.get(4)?,
                    entities: row.get(5)?,
                    importance: row.get(6)?,
                    valid_at: row.get(7)?,
                    invalid_at: row.get(8)?,
                    recorded_at: row.get(9)?,
                    trust: row.get(10)?,
                    entity: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The current 状态 fact for `(project, entity)` — "这个模块现在是什么状态"
    /// (§6.4 内容态,供经理理解代码;绝不进派活决策)。`None` if none recorded.
    /// At most one exists by the §6.4 partial-unique index.
    pub fn current_state(&self, project: &str, entity: &str) -> anyhow::Result<Option<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let row = conn
            .query_row(
                "SELECT id, project, scope, kind, text, entities, importance,
                        valid_at, invalid_at, recorded_at, trust, entity
                 FROM memory_fact
                 WHERE project = ?1 AND entity = ?2 AND kind = '状态' AND invalid_at IS NULL",
                params![project, entity],
                |row| {
                    Ok(FactRecord {
                        id: row.get(0)?,
                        project: row.get(1)?,
                        scope: row.get(2)?,
                        kind: row.get(3)?,
                        text: row.get(4)?,
                        entities: row.get(5)?,
                        importance: row.get(6)?,
                        valid_at: row.get(7)?,
                        invalid_at: row.get(8)?,
                        recorded_at: row.get(9)?,
                        trust: row.get(10)?,
                        entity: row.get(11)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }
}
