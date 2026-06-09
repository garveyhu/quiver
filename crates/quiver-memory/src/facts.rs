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
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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

    /// Full-text search current-truth facts for `project` by `query` (FTS5 MATCH,
    /// DESIGN §20), best relevance first. Retired facts (`invalid_at` set) and other
    /// projects are excluded. `query` is an FTS5 query string — callers usually pass
    /// plain keywords. The §6.7 hybrid recall (FTS + vector + recency/importance/
    /// trust weighting) builds on this; for now it's the keyword leg.
    pub fn search_facts(&self, project: &str, query: &str) -> anyhow::Result<Vec<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT mf.id, mf.project, mf.scope, mf.kind, mf.text, mf.entities,
                    mf.importance, mf.valid_at, mf.invalid_at, mf.recorded_at, mf.trust
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// §6.7 recall: rank current-truth facts for `project` by a fixed-weight blend
    /// of importance + trust tier + recency. With a non-empty `query`, candidates
    /// come from FTS5 (the keyword leg, §20); without one (cold start / open recall)
    /// all current facts are scored. Returns up to `limit`, best first. Weights are
    /// fixed constants — §6.7: don't tune before there's an eval set. The vector leg
    /// of hybrid recall is P2.
    pub fn recall(
        &self,
        project: &str,
        query: Option<&str>,
        now_ms: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<FactRecord>> {
        let mut candidates = match query {
            Some(q) if !q.trim().is_empty() => self.search_facts(project, q)?,
            _ => self.current_facts(project)?,
        };
        candidates.sort_by(|a, b| {
            score(b, now_ms)
                .partial_cmp(&score(a, now_ms))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(limit);
        Ok(candidates)
    }
}

/// Half-life (days) of a fact's recency contribution — older facts decay smoothly.
const RECALL_HALF_LIFE_DAYS: f64 = 14.0;
const W_IMPORTANCE: f64 = 1.0; // importance is 1–10
const W_TRUST: f64 = 2.0; // trust tier maps to 1–5 → contributes 2–10
const W_RECENCY: f64 = 3.0; // recency is 0–1 → contributes 0–3

/// Numeric weight of a §6.2 trust tier (higher = more trustworthy). Unknown /
/// untrusted tiers floor at 1.
fn trust_weight(trust: &str) -> f64 {
    match trust {
        "权威" => 5.0,
        "已验证·机械" | "已验证·印证" => 4.0,
        "员工汇报" => 2.0,
        _ => 1.0, // 不可信 / unknown
    }
}

/// Smooth recency in [0, 1]: 1 when just recorded, 0.5 at one half-life, → 0 old.
fn recency_score(now_ms: i64, recorded_at: i64) -> f64 {
    let age_days = ((now_ms - recorded_at).max(0) as f64) / 86_400_000.0;
    1.0 / (1.0 + age_days / RECALL_HALF_LIFE_DAYS)
}

/// The §6.7 fixed-weight recall score for a fact.
fn score(f: &FactRecord, now_ms: i64) -> f64 {
    W_IMPORTANCE * f.importance as f64
        + W_TRUST * trust_weight(&f.trust)
        + W_RECENCY * recency_score(now_ms, f.recorded_at)
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
    fn search_facts_matches_keyword_filtered_by_project() {
        let store = MemoryStore::open_in_memory().unwrap();
        store
            .insert_fact(&fact("/r", "use tokio for the async runtime", 5, 1))
            .unwrap();
        store
            .insert_fact(&fact("/r", "prefer rusqlite bundled sqlite", 5, 2))
            .unwrap();
        store
            .insert_fact(&fact("/other", "tokio lives elsewhere", 5, 3))
            .unwrap();

        // 'tokio' → only /r's tokio fact (other project excluded).
        let hits = store.search_facts("/r", "tokio").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("tokio"));
        // 'sqlite' → the other /r fact.
        let hits2 = store.search_facts("/r", "sqlite").unwrap();
        assert_eq!(hits2.len(), 1);
        assert!(hits2[0].text.contains("rusqlite"));
        // no match → empty.
        assert!(store.search_facts("/r", "kubernetes").unwrap().is_empty());
    }

    #[test]
    fn recall_trust_breaks_importance_ties() {
        let store = MemoryStore::open_in_memory().unwrap();
        const NOW: i64 = 1_000_000_000_000;
        store
            .insert_fact(&NewFact {
                trust: "权威".into(),
                ..fact("/r", "authoritative", 5, NOW)
            })
            .unwrap();
        store
            .insert_fact(&NewFact {
                trust: "员工汇报".into(),
                ..fact("/r", "staff", 5, NOW)
            })
            .unwrap();
        let texts: Vec<String> = store
            .recall("/r", None, NOW, 10)
            .unwrap()
            .into_iter()
            .map(|f| f.text)
            .collect();
        assert_eq!(
            texts,
            vec!["authoritative", "staff"],
            "higher trust ranks first at equal importance/recency"
        );
    }

    #[test]
    fn recall_recency_breaks_ties() {
        let store = MemoryStore::open_in_memory().unwrap();
        const NOW: i64 = 1_000_000_000_000;
        let day = 86_400_000;
        store.insert_fact(&fact("/r", "old", 5, NOW - 60 * day)).unwrap();
        store.insert_fact(&fact("/r", "fresh", 5, NOW)).unwrap();
        let r = store.recall("/r", None, NOW, 10).unwrap();
        assert_eq!(
            r[0].text, "fresh",
            "fresher fact ranks first at equal importance/trust"
        );
    }

    #[test]
    fn recall_with_query_uses_fts_then_ranks() {
        let store = MemoryStore::open_in_memory().unwrap();
        const NOW: i64 = 1_000_000_000_000;
        store
            .insert_fact(&fact("/r", "tokio async runtime choice", 3, NOW))
            .unwrap();
        store
            .insert_fact(&NewFact {
                trust: "权威".into(),
                ..fact("/r", "tokio is mandated", 3, NOW)
            })
            .unwrap();
        // High importance but doesn't match the query → excluded by FTS.
        store.insert_fact(&fact("/r", "unrelated sqlite note", 9, NOW)).unwrap();

        let texts: Vec<String> = store
            .recall("/r", Some("tokio"), NOW, 10)
            .unwrap()
            .into_iter()
            .map(|f| f.text)
            .collect();
        assert_eq!(texts.len(), 2, "only tokio-matching facts (FTS filters)");
        assert_eq!(
            texts[0], "tokio is mandated",
            "authoritative tokio fact outranks the staff one"
        );
    }

    #[test]
    fn search_excludes_retired_facts() {
        let store = MemoryStore::open_in_memory().unwrap();
        let id = store
            .insert_fact(&fact("/r", "deprecated approach foobar", 5, 1))
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE memory_fact SET invalid_at = 9 WHERE id = ?1",
                params![id],
            )
            .unwrap();
        }
        assert!(
            store.search_facts("/r", "foobar").unwrap().is_empty(),
            "a retired fact must drop out of full-text search too"
        );
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
