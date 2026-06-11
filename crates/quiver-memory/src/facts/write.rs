//! 事实写路径(§6.3/§6.4):追加(append-only)、顶替作废(supersede)、绿测试印证升档
//! (corroborate)。状态事实插新前自动作废同主题旧状态(§6.4 自我演化)。

use rusqlite::{params, OptionalExtension};

use super::{NewFact, DEFAULT_IMPORTANCE, DEFAULT_SCOPE};
use crate::MemoryStore;

/// The tier a fact reaches once corroborated by ≥2 independent green-test episodes
/// (§6.2), and how many distinct episodes that takes.
const TIER_CORROBORATED: &str = "已验证·印证";
const CORROBORATION_THRESHOLD: i64 = 2;

impl MemoryStore {
    /// Append a fact; returns its new row id. Append-only (§6.3 — never UPDATE/DELETE
    /// a fact's truth; P2 supersede inserts a new row + retires the old in one txn).
    pub fn insert_fact(&self, f: &NewFact) -> anyhow::Result<i64> {
        // §6.4 状态自我演化:状态事实((project,entity) 当前唯一)插新前,自动作废同主题的旧状态 ——
        // 「用 SQLite」被「用 PostgreSQL」取代时旧的退场,brief 只见当前真相、记忆不堆矛盾。
        // 非状态事实(约定/教训/提炼)正常并存、不消解(它们不是互斥的单一真相)。
        if f.kind == "状态" {
            if let Some(ent) = f.entity.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
                let old: Option<i64> = {
                    let conn = self.conn.lock().expect("memory store lock");
                    conn.query_row(
                        "SELECT id FROM memory_fact WHERE project = ?1 AND entity = ?2 \
                         AND kind = '状态' AND invalid_at IS NULL ORDER BY recorded_at DESC LIMIT 1",
                        params![f.project, ent],
                        |r| r.get::<_, i64>(0),
                    )
                    .optional()?
                };
                if let Some(old_id) = old {
                    // 失效旧状态 + 插新状态,一个事务原子(reader 永不见两个当前版本)。
                    return self.supersede_fact(old_id, f, f.recorded_at);
                }
            }
        }
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
}

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
