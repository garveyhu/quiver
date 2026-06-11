//! §6.7 内在分召回:当前真相事实按 importance + trust 档 + recency 的固定权重融合排序。
//! `score` 以 `pub(super)` 暴露给兄弟模块 `vector` —— hybrid_recall 的内在腿复用同一打分。

use super::FactRecord;
use crate::MemoryStore;

const RECALL_HALF_LIFE_DAYS: f64 = 14.0;
const W_IMPORTANCE: f64 = 1.0; // importance is 1–10
const W_TRUST: f64 = 2.0; // trust tier maps to 1–5 → contributes 2–10
const W_RECENCY: f64 = 3.0; // recency is 0–1 → contributes 0–3

impl MemoryStore {
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

/// The §6.7 fixed-weight recall score for a fact. `pub(super)` so `vector`'s
/// hybrid_recall intrinsic leg reuses the exact same scoring.
pub(super) fn score(f: &FactRecord, now_ms: i64) -> f64 {
    W_IMPORTANCE * f.importance as f64
        + W_TRUST * trust_weight(&f.trust)
        + W_RECENCY * recency_score(now_ms, f.recorded_at)
}
