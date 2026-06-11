//! Bi-temporal semantic facts (DESIGN §6.3): append-only, never deleted, each
//! carrying a trust tier. This module root owns the wire types ([`NewFact`] /
//! [`FactRecord`]) + their defaults; the `impl MemoryStore` methods are split by
//! responsibility into sibling files:
//! - [`write`]  追加 / 顶替作废 / 印证升档(§6.3/§6.4)
//! - [`read`]   当前真相 / FTS 检索 / 状态读(§6.3)
//! - [`recall`] §6.7 内在分召回(importance·trust·recency 打分)
//! - [`vector`] §6.7 向量召回 + RRF 混合检索

mod read;
mod recall;
mod vector;
mod write;

/// Default scope for a fact when unspecified (DESIGN §19 — a fact is about the
/// current repo unless promoted to company-wide). `pub(super)` so [`write`] uses them.
pub(super) const DEFAULT_SCOPE: &str = "本仓库";
/// Default importance (1–10) when unspecified.
pub(super) const DEFAULT_IMPORTANCE: i64 = 5;

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

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::{Embedder, MemoryStore};

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

    /// 一条状态事实(kind='状态' + entity),用于测自我演化消解。
    fn state(project: &str, entity: &str, text: &str, recorded_at: i64) -> NewFact {
        NewFact { kind: "状态".into(), entity: Some(entity.into()), ..fact(project, text, 7, recorded_at) }
    }

    #[test]
    fn state_fact_self_supersedes_same_entity_but_conventions_coexist() {
        let s = MemoryStore::open_in_memory().unwrap();
        // §6.4 自我演化:同主题(数据库)状态,新的作废旧的 —— 只剩一个当前真相。
        s.insert_fact(&state("/r", "数据库", "用 SQLite", 100)).unwrap();
        s.insert_fact(&state("/r", "数据库", "用 PostgreSQL", 200)).unwrap();
        let db: Vec<_> = s
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .filter(|f| f.entity.as_deref() == Some("数据库"))
            .collect();
        assert_eq!(db.len(), 1, "同主题状态只剩一个当前真相");
        assert!(db[0].text.contains("PostgreSQL"), "当前是最新状态");
        // 不同主题(部署)互不影响。
        s.insert_fact(&state("/r", "部署", "用 Docker", 300)).unwrap();
        assert_eq!(s.current_facts("/r").unwrap().len(), 2, "数据库 + 部署 两个当前状态");
        // 非状态(约定)不互斥 → 多条并存,不消解。
        s.insert_fact(&fact("/r", "约定:函数加 q_ 前缀", 8, 400)).unwrap();
        s.insert_fact(&fact("/r", "约定:用 4 空格缩进", 8, 500)).unwrap();
        let convs: Vec<_> = s
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .filter(|f| f.kind == "decision")
            .collect();
        assert_eq!(convs.len(), 2, "非状态事实并存,不消解");
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
        // §6.4 自我演化:同 (project,entity) 插第二个当前状态 → insert_fact 自动作废旧的(不再靠
        // DB 唯一约束拒绝、要人手动 supersede),成功;当前只剩最新那个 —— 记忆自动跟上现实变化。
        assert!(store.insert_fact(&state_fact("/r", "moduleA", "uses async-std")).is_ok());
        let a: Vec<_> = store
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .filter(|f| f.entity.as_deref() == Some("moduleA"))
            .collect();
        assert_eq!(a.len(), 1, "同主题状态唯一当前真相");
        assert!(a[0].text.contains("async-std"), "当前是最新状态、旧的已失效");
        // 不同 entity、非状态事实正常。
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

    #[test]
    fn hybrid_recall_fuses_keyword_vector_intrinsic() {
        let store = MemoryStore::open_in_memory().unwrap();
        let a = store.insert_fact(&fact("/r", "tokio async runtime", 5, 1)).unwrap();
        let b = store.insert_fact(&fact("/r", "database schema", 9, 1)).unwrap();
        let c = store.insert_fact(&fact("/r", "tokio tasks scheduling", 5, 1)).unwrap();
        store.set_fact_embedding(a, &[1.0, 0.0, 0.0], "fake").unwrap();
        store.set_fact_embedding(b, &[0.0, 1.0, 0.0], "fake").unwrap();
        store.set_fact_embedding(c, &[0.6, 0.4, 0.0], "fake").unwrap();

        // Query "tokio" + vector near A → A wins all three legs (keyword + nearest vector + present).
        let hits = store.hybrid_recall("/r", "tokio", &[1.0, 0.0, 0.0], 1000, 3).unwrap();
        assert_eq!(hits.len(), 3, "intrinsic leg always includes every current fact");
        assert_eq!(hits[0].text, "tokio async runtime", "wins keyword+vector+intrinsic fusion");
        // B (no keyword/semantic match, high importance) still surfaces via the intrinsic leg.
        assert!(hits.iter().any(|f| f.text == "database schema"));
    }

    #[test]
    fn embed_unembedded_backfills_and_is_idempotent() {
        use crate::FakeEmbedder;
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert_fact(&fact("/r", "alpha", 5, 1)).unwrap();
        store.insert_fact(&fact("/r", "beta", 5, 1)).unwrap();
        let emb = FakeEmbedder::new(8);

        assert_eq!(store.embed_unembedded("/r", &emb, 100).unwrap(), 2, "both backfilled");
        assert_eq!(store.embed_unembedded("/r", &emb, 100).unwrap(), 0, "nothing left to embed");

        // The stored vectors are now queryable: alpha's own vector matches alpha.
        let qv = emb.embed(&["alpha".into()]).unwrap().remove(0);
        let hits = store.vector_search("/r", &qv, 2).unwrap();
        assert_eq!(hits[0].text, "alpha", "alpha's own embedding is its closest match");
    }
}
