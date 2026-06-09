//! Bi-temporal semantic facts (DESIGN §6.3): append-only, never deleted, each
//! carrying a trust tier. P1 writes facts and reads *current truth*
//! (`invalid_at IS NULL`); the supersede/invalidate machinery (§6.4) is P2 — so
//! this slice exposes write + current-read only.

use rusqlite::{params, OptionalExtension};

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
    /// Scalar entity this fact is about — only meaningful for `kind='状态'`, where
    /// (project, entity) is unique among current facts (§6.4). `None` otherwise.
    pub entity: Option<String>,
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
    /// Scalar entity (§6.4) — set for `kind='状态'` facts, else `None`.
    pub entity: Option<String>,
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
                (project, scope, kind, text, entities, entity, importance, valid_at,
                 recorded_at, provenance, trust, source_commit, source_episode_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                f.project,
                scope,
                f.kind,
                f.text,
                f.entities,
                f.entity,
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

    /// Supersede a current fact with a new one in ONE transaction (DESIGN §6.4 —
    /// the librarian's 作废/顶替 primitive, the start of P2). Inserts `new` as
    /// current truth, then retires the old fact (`invalid_at` + `retired_at` =
    /// `at_ms`, `superseded_by` = the new id) — only if it was still current
    /// (`invalid_at IS NULL`, so a stale supersede is a no-op on the old row).
    /// Atomic: a reader never sees two current versions or a dangling link.
    /// Returns the new fact's id. (AI judging WHICH fact to supersede is the
    /// librarian's job, layered on top later; this is the mechanical write.)
    pub fn supersede_fact(
        &self,
        old_id: i64,
        new: &NewFact,
        at_ms: i64,
    ) -> anyhow::Result<i64> {
        let scope = new.scope.as_deref().unwrap_or(DEFAULT_SCOPE);
        let importance = new.importance.unwrap_or(DEFAULT_IMPORTANCE);
        let mut conn = self.conn.lock().expect("memory store lock");
        let tx = conn.transaction()?;
        // (1) Retire the old fact FIRST — frees the current-状态 (project,entity) slot
        // before the insert, so the §6.4 partial-unique index never sees two current
        // versions (§6.4: "先把旧的标失效、再插新的"). Conditional on it still being
        // current, so a stale supersede is a no-op on the old row.
        let retired = tx.execute(
            "UPDATE memory_fact SET invalid_at = ?2, retired_at = ?2
             WHERE id = ?1 AND invalid_at IS NULL",
            params![old_id, at_ms],
        )?;
        // (2) Insert the new current-truth fact.
        tx.execute(
            "INSERT INTO memory_fact
                (project, scope, kind, text, entities, entity, importance, valid_at,
                 recorded_at, provenance, trust, source_commit, source_episode_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                new.project,
                scope,
                new.kind,
                new.text,
                new.entities,
                new.entity,
                importance,
                new.valid_at,
                new.recorded_at,
                new.provenance,
                new.trust,
                new.source_commit,
                new.source_episode_id,
            ],
        )?;
        let new_id = tx.last_insert_rowid();
        // (3) Link old → new, but only if WE just retired it (else don't re-link an
        // already-retired fact to a newer successor).
        if retired == 1 {
            tx.execute(
                "UPDATE memory_fact SET superseded_by = ?2 WHERE id = ?1",
                params![old_id, new_id],
            )?;
        }
        tx.commit()?;
        Ok(new_id)
    }

    /// Record that `episode_id` (an independent green-test run) corroborates fact
    /// `fact_id`, and — if the fact now has ≥2 distinct corroborating episodes and
    /// its trust is below 已验证·印证 — mechanically upgrade it to that tier
    /// (DESIGN §6.2: "两次独立绿测试印证 → Rust 升档;AI 拥有否决、不是授予"). Set
    /// semantics (the (fact,episode) pair is INSERT-OR-IGNOREd) → recording the SET
    /// not a count, so replaying the same corroboration is idempotent. Never
    /// downgrades (权威 / 已验证·* stay). Returns whether an upgrade happened.
    pub fn corroborate_fact(&self, fact_id: i64, episode_id: i64) -> anyhow::Result<bool> {
        let mut conn = self.conn.lock().expect("memory store lock");
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO fact_corroboration (fact_id, episode_id) VALUES (?1, ?2)",
            params![fact_id, episode_id],
        )?;
        let distinct: i64 = tx.query_row(
            "SELECT COUNT(*) FROM fact_corroboration WHERE fact_id = ?1",
            params![fact_id],
            |r| r.get(0),
        )?;
        let mut upgraded = false;
        if distinct >= CORROBORATION_THRESHOLD {
            let current: Option<String> = tx
                .query_row(
                    "SELECT trust FROM memory_fact WHERE id = ?1 AND invalid_at IS NULL",
                    params![fact_id],
                    |r| r.get(0),
                )
                .ok();
            if let Some(tier) = current {
                if tier_rank(&tier) < tier_rank(TIER_CORROBORATED) {
                    tx.execute(
                        "UPDATE memory_fact SET trust = ?2 WHERE id = ?1",
                        params![fact_id, TIER_CORROBORATED],
                    )?;
                    upgraded = true;
                }
            }
        }
        tx.commit()?;
        Ok(upgraded)
    }

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

    /// Attach a dense embedding to a fact (§6.7 向量召回). Stores the f32 vector as a
    /// little-endian BLOB and stamps which model/dim produced it (so a model swap
    /// knows what to recompute). Idempotent overwrite.
    pub fn set_fact_embedding(
        &self,
        fact_id: i64,
        embedding: &[f32],
        embed_model: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("memory store lock");
        conn.execute(
            "UPDATE memory_fact SET embedding = ?2, embed_model = ?3, embed_dim = ?4 WHERE id = ?1",
            params![fact_id, vec_to_blob(embedding), embed_model, embedding.len() as i64],
        )?;
        Ok(())
    }

    /// §6.7 向量腿:在 `project` 的当前真相事实里按 cosine 相似度排序返回 top-k(只看已
    /// 向量化的事实)。小库直接 Rust 暴力余弦;sqlite-vec(vec0)ANN 索引是后续刀(规模化时)。
    /// 与 `search_facts`(FTS 关键词腿)互补,上层融合成混合检索。
    pub fn vector_search(
        &self,
        project: &str,
        query: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<FactRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, scope, kind, text, entities, importance,
                    valid_at, invalid_at, recorded_at, trust, entity, embedding
             FROM memory_fact
             WHERE project = ?1 AND invalid_at IS NULL AND embedding IS NOT NULL",
        )?;
        let mut scored: Vec<(f32, FactRecord)> = stmt
            .query_map(params![project], |row| {
                let blob: Vec<u8> = row.get(12)?;
                let rec = FactRecord {
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
                };
                Ok((cosine(query, &blob_to_vec(&blob)), rec))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(_, r)| r).collect())
    }
}

/// Half-life (days) of a fact's recency contribution — older facts decay smoothly.
const RECALL_HALF_LIFE_DAYS: f64 = 14.0;
const W_IMPORTANCE: f64 = 1.0; // importance is 1–10
const W_TRUST: f64 = 2.0; // trust tier maps to 1–5 → contributes 2–10
const W_RECENCY: f64 = 3.0; // recency is 0–1 → contributes 0–3

/// The tier a fact reaches once corroborated by ≥2 independent green-test episodes
/// (§6.2), and how many distinct episodes that takes.
const TIER_CORROBORATED: &str = "已验证·印证";
const CORROBORATION_THRESHOLD: i64 = 2;

/// Ordinal rank of a §6.2 trust tier for "only ever upgrade, never downgrade"
/// comparisons. 权威 > 已验证·* > 员工汇报 > 不可信/unknown.
fn tier_rank(trust: &str) -> i32 {
    match trust {
        "权威" => 4,
        "已验证·机械" | "已验证·印证" => 3,
        "员工汇报" => 1,
        _ => 0,
    }
}

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

/// Pack an f32 embedding into a little-endian BLOB.
fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Unpack a little-endian BLOB back into f32s (trailing partial bytes ignored).
fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity in [-1, 1]; 0 when either vector is empty/zero or dims differ.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
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
            entity: None,
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

    #[test]
    fn supersede_retires_old_makes_new_current_and_links() {
        let store = MemoryStore::open_in_memory().unwrap();
        let old = store.insert_fact(&fact("/r", "old approach foo", 5, 1)).unwrap();
        let new = store
            .supersede_fact(old, &fact("/r", "new approach bar", 5, 2), 100)
            .unwrap();

        // Only the new fact is current truth; FTS reflects the swap.
        let texts: Vec<String> = store
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .map(|f| f.text)
            .collect();
        assert_eq!(texts, vec!["new approach bar"]);
        assert!(store.search_facts("/r", "foo").unwrap().is_empty(), "old text no longer current in FTS");
        assert_eq!(store.search_facts("/r", "bar").unwrap().len(), 1);

        // Old row retired + linked forward.
        let conn = store.conn.lock().unwrap();
        let (inv, retired, sup): (Option<i64>, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT invalid_at, retired_at, superseded_by FROM memory_fact WHERE id = ?1",
                params![old],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(inv, Some(100));
        assert_eq!(retired, Some(100));
        assert_eq!(sup, Some(new), "old fact links to its successor");
    }

    #[test]
    fn supersede_is_noop_on_already_retired_old() {
        let store = MemoryStore::open_in_memory().unwrap();
        let old = store.insert_fact(&fact("/r", "v1", 5, 1)).unwrap();
        let v2 = store.supersede_fact(old, &fact("/r", "v2", 5, 2), 10).unwrap();
        // Superseding the ALREADY-retired old again must not re-touch it (WHERE
        // invalid_at IS NULL); the new v3 still inserts as current.
        let v3 = store.supersede_fact(old, &fact("/r", "v3", 5, 3), 20).unwrap();
        let conn = store.conn.lock().unwrap();
        let sup: Option<i64> = conn
            .query_row("SELECT superseded_by FROM memory_fact WHERE id = ?1", params![old], |r| r.get(0))
            .unwrap();
        assert_eq!(sup, Some(v2), "old still points at v2, not re-linked to v3");
        drop(conn);
        // v2 is now stale (still current — supersede(old,...) didn't retire v2), v3 also current.
        // Both v2 and v3 are current truth here (we only retired `old`); that's expected —
        // higher-level librarian decides what to supersede. Assert v3 exists + is current.
        let texts: Vec<String> = store.current_facts("/r").unwrap().into_iter().map(|f| f.text).collect();
        assert!(texts.contains(&"v3".to_string()) && texts.contains(&"v2".to_string()));
        let _ = v3;
    }

    fn episode(store: &MemoryStore, ts: i64) -> i64 {
        store
            .record_episode(&crate::NewEpisode {
                project: "/r".into(),
                created_at: ts,
                ..Default::default()
            })
            .unwrap()
    }

    #[test]
    fn corroborate_upgrades_after_two_distinct_episodes_idempotently() {
        let store = MemoryStore::open_in_memory().unwrap();
        let (ep1, ep2) = (episode(&store, 1), episode(&store, 2));
        let fid = store.insert_fact(&fact("/r", "claim", 5, 1)).unwrap(); // 员工汇报
        // 1st corroboration: below threshold → no upgrade.
        assert!(!store.corroborate_fact(fid, ep1).unwrap());
        assert_eq!(store.current_facts("/r").unwrap()[0].trust, "员工汇报");
        // 2nd DISTINCT episode → mechanical upgrade to 已验证·印证.
        assert!(store.corroborate_fact(fid, ep2).unwrap());
        assert_eq!(store.current_facts("/r").unwrap()[0].trust, "已验证·印证");
        // Replaying ep1 (dup pair) → set unchanged, no re-upgrade (idempotent).
        assert!(!store.corroborate_fact(fid, ep1).unwrap());
        assert_eq!(store.current_facts("/r").unwrap()[0].trust, "已验证·印证");
    }

    #[test]
    fn corroborate_never_downgrades_authoritative() {
        let store = MemoryStore::open_in_memory().unwrap();
        let (ep1, ep2) = (episode(&store, 1), episode(&store, 2));
        let fid = store
            .insert_fact(&NewFact { trust: "权威".into(), ..fact("/r", "ceo says", 5, 1) })
            .unwrap();
        store.corroborate_fact(fid, ep1).unwrap();
        assert!(
            !store.corroborate_fact(fid, ep2).unwrap(),
            "权威 must not be touched by corroboration"
        );
        assert_eq!(store.current_facts("/r").unwrap()[0].trust, "权威");
    }

    fn state_fact(project: &str, entity: &str, text: &str) -> NewFact {
        NewFact {
            kind: "状态".into(),
            entity: Some(entity.into()),
            ..fact(project, text, 5, 1)
        }
    }

    #[test]
    fn current_state_unique_per_project_entity() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert_fact(&state_fact("/r", "moduleA", "uses tokio")).unwrap();
        // A second CURRENT 状态 fact for the same (project, entity) is rejected (§6.4).
        assert!(
            store.insert_fact(&state_fact("/r", "moduleA", "uses async-std")).is_err(),
            "two current 状态 facts for same (project,entity) must be rejected by the DB"
        );
        // A different entity — and a non-状态 fact on the same entity — are fine.
        assert!(store.insert_fact(&state_fact("/r", "moduleB", "uses sqlite")).is_ok());
        assert!(store.insert_fact(&fact("/r", "moduleA note", 5, 1)).is_ok());
    }

    #[test]
    fn supersede_state_fact_swaps_without_violating_unique() {
        let store = MemoryStore::open_in_memory().unwrap();
        let old = store.insert_fact(&state_fact("/r", "moduleA", "uses tokio")).unwrap();
        // Retire-old-before-insert-new lets the same-entity 状态 swap pass the unique index.
        store.supersede_fact(old, &state_fact("/r", "moduleA", "uses smol"), 100).unwrap();
        let current: Vec<String> = store
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .filter(|f| f.kind == "状态")
            .map(|f| f.text)
            .collect();
        assert_eq!(current, vec!["uses smol"], "only the new 状态 is current after supersede");
    }

    #[test]
    fn current_state_reads_entity_status_and_follows_supersede() {
        let store = MemoryStore::open_in_memory().unwrap();
        assert!(store.current_state("/r", "moduleA").unwrap().is_none());
        store.insert_fact(&state_fact("/r", "moduleA", "uses tokio")).unwrap();
        let s = store.current_state("/r", "moduleA").unwrap().expect("has a current state");
        assert_eq!(s.text, "uses tokio");
        assert_eq!(s.entity.as_deref(), Some("moduleA"), "entity is now readable on FactRecord");
        assert_eq!(s.kind, "状态");
        assert!(store.current_state("/r", "moduleZ").unwrap().is_none());
        // After supersede, current_state reflects the new status.
        store.supersede_fact(s.id, &state_fact("/r", "moduleA", "uses smol"), 50).unwrap();
        assert_eq!(
            store.current_state("/r", "moduleA").unwrap().unwrap().text,
            "uses smol"
        );
    }

    #[test]
    fn vector_search_ranks_by_cosine_and_skips_unembedded() {
        let store = MemoryStore::open_in_memory().unwrap();
        let a = store.insert_fact(&fact("/r", "alpha", 5, 1)).unwrap();
        let b = store.insert_fact(&fact("/r", "beta", 5, 1)).unwrap();
        let c = store.insert_fact(&fact("/r", "gamma", 5, 1)).unwrap();
        store.set_fact_embedding(a, &[1.0, 0.0, 0.0], "qwen-test").unwrap();
        store.set_fact_embedding(b, &[0.0, 1.0, 0.0], "qwen-test").unwrap();
        store.set_fact_embedding(c, &[0.9, 0.1, 0.0], "qwen-test").unwrap();
        // A fact with no embedding must be excluded from the vector leg.
        store.insert_fact(&fact("/r", "no-vec", 5, 1)).unwrap();

        let hits = store.vector_search("/r", &[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(hits.len(), 2, "limit honored, un-embedded fact skipped");
        assert_eq!(hits[0].text, "alpha", "exact-match vector ranks first");
        assert_eq!(hits[1].text, "gamma", "near vector second, orthogonal beta drops");
    }
}
