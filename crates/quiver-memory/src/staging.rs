//! The memory staging area (DESIGN §6.4 待审/隔离 / §20 `memory_staging`): facts a
//! worker or manager *proposes* land here first — untrusted, never auto-promoted
//! into current truth. The librarian (P2) reviews and `promote`s them into
//! `memory_fact` with an explicit trust tier, or leaves them pending. This is the
//! "你毒不了绿测试" gate:员工写的不能自动塞进记忆。

use rusqlite::params;

use crate::MemoryStore;

/// A proposed fact awaiting review (writes from a worker/manager decision —
/// §21 `memory_writes` → 进待审区).
#[derive(Clone, Debug)]
pub struct NewStaged {
    pub project: String,
    pub kind: String,
    pub text: String,
    /// JSON array of entity strings, or `None`.
    pub entities: Option<String>,
    /// The node that proposed it (orchestration.db id); `None` in P0/P1.
    pub writer_node_id: Option<String>,
    pub created_at: i64,
}

/// A pending staged row (read shape).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedRecord {
    pub id: i64,
    pub project: String,
    pub kind: String,
    pub text: String,
    pub entities: Option<String>,
    pub writer_node_id: Option<String>,
    pub created_at: i64,
}

impl MemoryStore {
    /// Stage a proposed fact for review. It is NOT current truth and never appears
    /// in `current_facts` / `recall` until promoted. Returns its staging id.
    pub fn stage_fact(&self, s: &NewStaged) -> anyhow::Result<i64> {
        let conn = self.conn.lock().expect("memory store lock");
        conn.execute(
            "INSERT INTO memory_staging
                (project, kind, text, entities, writer_node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![s.project, s.kind, s.text, s.entities, s.writer_node_id, s.created_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Pending (not-yet-promoted) staged proposals for `project`, oldest first
    /// (review queue order).
    pub fn pending_staged(&self, project: &str) -> anyhow::Result<Vec<StagedRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, kind, text, entities, writer_node_id, created_at
             FROM memory_staging
             WHERE project = ?1 AND promoted = 0
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt
            .query_map(params![project], |row| {
                Ok(StagedRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    kind: row.get(2)?,
                    text: row.get(3)?,
                    entities: row.get(4)?,
                    writer_node_id: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Promote a pending staged proposal into `memory_fact` as current truth with
    /// the reviewer-assigned `trust` tier (§6.2 — AI proposes, the librarian/Rust
    /// decides the tier; 员工写的 never self-grants trust). One transaction: insert
    /// the fact, mark the staged row promoted. Returns the new fact id, or `None`
    /// if the staged id is unknown or already promoted (idempotent — no double-insert).
    pub fn promote_staged(
        &self,
        staged_id: i64,
        trust: &str,
        at_ms: i64,
    ) -> anyhow::Result<Option<i64>> {
        let mut conn = self.conn.lock().expect("memory store lock");
        let tx = conn.transaction()?;
        // Read the pending row; bail (None) if missing or already promoted.
        let staged: Option<(String, String, String, Option<String>)> = tx
            .query_row(
                "SELECT project, kind, text, entities FROM memory_staging
                 WHERE id = ?1 AND promoted = 0",
                params![staged_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();
        let Some((project, kind, text, entities)) = staged else {
            return Ok(None);
        };
        tx.execute(
            "INSERT INTO memory_fact
                (project, kind, text, entities, importance, recorded_at, trust,
                 provenance)
             VALUES (?1, ?2, ?3, ?4, 5, ?5, ?6, 'staged')",
            params![project, kind, text, entities, at_ms, trust],
        )?;
        let new_id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE memory_staging SET promoted = 1 WHERE id = ?1",
            params![staged_id],
        )?;
        tx.commit()?;
        Ok(Some(new_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(project: &str, text: &str, created_at: i64) -> NewStaged {
        NewStaged {
            project: project.into(),
            kind: "decision".into(),
            text: text.into(),
            entities: None,
            writer_node_id: None,
            created_at,
        }
    }

    #[test]
    fn stage_does_not_enter_current_truth_until_promoted() {
        let store = MemoryStore::open_in_memory().unwrap();
        let id = store.stage_fact(&staged("/r", "proposed thing", 1)).unwrap();
        // Staged ≠ current truth.
        assert!(store.current_facts("/r").unwrap().is_empty());
        assert_eq!(store.pending_staged("/r").unwrap().len(), 1);

        // Promote with a reviewer tier → now a current fact, staged drops out of pending.
        let fid = store.promote_staged(id, "员工汇报", 100).unwrap();
        assert!(fid.is_some());
        let current = store.current_facts("/r").unwrap();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].text, "proposed thing");
        assert_eq!(current[0].trust, "员工汇报");
        assert!(store.pending_staged("/r").unwrap().is_empty(), "promoted leaves the review queue");
    }

    #[test]
    fn promote_is_idempotent_and_none_on_unknown() {
        let store = MemoryStore::open_in_memory().unwrap();
        let id = store.stage_fact(&staged("/r", "x", 1)).unwrap();
        assert!(store.promote_staged(id, "员工汇报", 10).unwrap().is_some());
        // Second promote of the same id → None (already promoted, no double fact).
        assert!(store.promote_staged(id, "员工汇报", 11).unwrap().is_none());
        assert_eq!(store.current_facts("/r").unwrap().len(), 1, "no double-insert");
        // Unknown id → None.
        assert!(store.promote_staged(9999, "员工汇报", 12).unwrap().is_none());
    }

    #[test]
    fn pending_staged_is_per_project_oldest_first() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.stage_fact(&staged("/r", "first", 1)).unwrap();
        store.stage_fact(&staged("/r", "second", 2)).unwrap();
        store.stage_fact(&staged("/other", "elsewhere", 1)).unwrap();
        let texts: Vec<String> = store
            .pending_staged("/r")
            .unwrap()
            .into_iter()
            .map(|s| s.text)
            .collect();
        assert_eq!(texts, vec!["first", "second"]);
    }
}
