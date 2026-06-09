//! Bi-temporal semantic facts (DESIGN §6.3): append-only, never deleted, each
//! carrying a trust tier. P1 writes facts and reads *current truth*
//! (`invalid_at IS NULL`); the supersede/invalidate machinery (§6.4) is P2 — so
//! this slice exposes write + current-read only.

use rusqlite::params;

use crate::MemoryStore;

/// Default scope for a fact when unspecified (DESIGN §19 — a fact is about the
/// current repo unless promoted to company-wide).
const DEFAULT_SCOPE: &str = "本仓库";
/// Default importance (1–10) when unspecified.
const DEFAULT_IMPORTANCE: i64 = 5;

/// A new fact to append. `scope` / `importance` fall back to defaults when `None`.
/// Bi-temporal bookkeeping (`invalid_at` / `superseded_by` / `retired_at`) is not
/// set on write — a fresh fact is current truth; P2's librarian retires it later.
#[derive(Clone, Debug)]
pub struct NewFact {
    pub project: String,
    pub scope: Option<String>,
    pub kind: String,
    pub text: String,
    /// JSON array of entity strings, or `None`.
    pub entities: Option<String>,
    pub importance: Option<i64>,
    /// When the fact became true in reality (§6.3); `None` = unknown/now.
    pub valid_at: Option<i64>,
    /// When we recorded it.
    pub recorded_at: i64,
    pub provenance: Option<String>,
    /// Trust tier (§6.2). Must be one of the five tiers or the schema CHECK rejects it.
    pub trust: String,
    pub source_commit: Option<String>,
    pub source_episode_id: Option<i64>,
}

/// A stored fact row (current-truth read shape — the fields recall/brief needs).
#[derive(Clone, Debug, PartialEq)]
pub struct FactRecord {
    pub id: i64,
    pub project: String,
    pub scope: String,
    pub kind: String,
    pub text: String,
    pub entities: Option<String>,
    pub importance: i64,
    pub valid_at: Option<i64>,
    pub invalid_at: Option<i64>,
    pub recorded_at: i64,
    pub trust: String,
}

impl MemoryStore {
    /// Append a fact; returns its new row id. Append-only (§6.3 — never UPDATE/DELETE
    /// a fact's truth; P2 supersede inserts a new row + retires the old in one txn).
    pub fn insert_fact(&self, f: &NewFact) -> anyhow::Result<i64> {
        let scope = f.scope.as_deref().unwrap_or(DEFAULT_SCOPE);
        let importance = f.importance.unwrap_or(DEFAULT_IMPORTANCE);
        let conn = self.conn.lock().expect("memory store lock");
        conn.execute(
            "INSERT INTO memory_fact
                (project, scope, kind, text, entities, importance, valid_at,
                 recorded_at, provenance, trust, source_commit, source_episode_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                f.project,
                scope,
                f.kind,
                f.text,
                f.entities,
                importance,
                f.valid_at,
                f.recorded_at,
                f.provenance,
                f.trust,
                f.source_commit,
                f.source_episode_id,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Current-truth facts for `project` (`invalid_at IS NULL`), most important and
    /// most recent first (DESIGN §6.3 — current truth is the un-retired set). The
    /// §6.7 recall ranking (recency·importance·trust + FTS/vector) is a later slice;
    /// this is the simple importance/recency order brief reads from for now.
    pub fn current_facts(&self, project: &str) -> anyhow::Result<Vec<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, scope, kind, text, entities, importance,
                    valid_at, invalid_at, recorded_at, trust
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(project: &str, text: &str, importance: i64, recorded_at: i64) -> NewFact {
        NewFact {
            project: project.into(),
            scope: None,
            kind: "decision".into(),
            text: text.into(),
            entities: None,
            importance: Some(importance),
            valid_at: None,
            recorded_at,
            provenance: None,
            trust: "员工汇报".into(),
            source_commit: None,
            source_episode_id: None,
        }
    }

    #[test]
    fn insert_and_current_facts_orders_by_importance_then_recency() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert_fact(&fact("/r", "low-old", 3, 1)).unwrap();
        store.insert_fact(&fact("/r", "high", 9, 2)).unwrap();
        store.insert_fact(&fact("/r", "low-new", 3, 5)).unwrap();

        let facts = store.current_facts("/r").unwrap();
        let texts: Vec<&str> = facts.iter().map(|f| f.text.as_str()).collect();
        // Importance desc, then recency desc within equal importance.
        assert_eq!(texts, vec!["high", "low-new", "low-old"]);
        // Defaults applied.
        assert_eq!(facts[0].scope, "本仓库");
        assert_eq!(facts[0].invalid_at, None, "fresh fact is current truth");
    }

    #[test]
    fn current_facts_excludes_retired_and_other_projects() {
        let store = MemoryStore::open_in_memory().unwrap();
        let id = store.insert_fact(&fact("/r", "retire-me", 5, 1)).unwrap();
        store.insert_fact(&fact("/other", "elsewhere", 5, 1)).unwrap();
        // Simulate a P2 retire (mark invalid_at) directly to prove the filter.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE memory_fact SET invalid_at = 99 WHERE id = ?1",
                params![id],
            )
            .unwrap();
        }
        let facts = store.current_facts("/r").unwrap();
        assert!(
            facts.is_empty(),
            "retired fact (invalid_at set) must drop out of current truth"
        );
    }
}
