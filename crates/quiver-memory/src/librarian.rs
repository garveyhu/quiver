//! 图书管理员(§6.4):用 AI 判断把矛盾的旧事实作废,让新事实成为当前真相。
//!
//! AI 判断走可注入的 [`ContradictionJudge`] —— 测试用确定性 [`FakeJudge`](免费),
//! 真跑用 LLM(§21)。机制层只做"判 → 作废",**绝不自动改信任档**(那是机械印证的事,
//! §6.2);候选由调用方挑(同实体 / `hybrid_recall` 语义邻居)。

use rusqlite::{params, OptionalExtension};

use crate::MemoryStore;

/// 一对(已有 existing / 新来 incoming)事实的矛盾裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 不冲突,各自成立。
    Independent,
    /// 矛盾,新事实取代旧的(旧的作废)。
    IncomingSupersedes,
    /// 矛盾,但旧的更可信,保留旧的(不动)。
    KeepExisting,
}

/// 判两条事实是否矛盾、谁当道。一次调用 = 一对。真实现调 LLM;测试用 [`FakeJudge`]。
pub trait ContradictionJudge {
    fn judge(&self, existing: &str, incoming: &str) -> anyhow::Result<Verdict>;
}

/// 确定性裁决器(测试 / simulate):永远返回构造时给定的 verdict。
pub struct FakeJudge(pub Verdict);

impl ContradictionJudge for FakeJudge {
    fn judge(&self, _existing: &str, _incoming: &str) -> anyhow::Result<Verdict> {
        Ok(self.0)
    }
}

impl MemoryStore {
    /// Retire a current fact: mark it invalid/retired now and link it as superseded by
    /// `superseded_by` (an already-existing fact). Returns true if it was current and
    /// got retired (idempotent: a stale call on an already-invalid fact returns false).
    pub fn retire_fact(
        &self,
        fact_id: i64,
        superseded_by: i64,
        at_ms: i64,
    ) -> anyhow::Result<bool> {
        let conn = self.conn.lock().expect("memory store lock");
        let n = conn.execute(
            "UPDATE memory_fact SET invalid_at = ?2, retired_at = ?2, superseded_by = ?3
             WHERE id = ?1 AND invalid_at IS NULL",
            params![fact_id, at_ms, superseded_by],
        )?;
        Ok(n == 1)
    }

    /// 图书管理员核对(§6.4):新事实 `new_id` 对一组候选旧事实逐一过 `judge`,凡裁为
    /// [`Verdict::IncomingSupersedes`] 的旧事实即作废(`superseded_by = new_id`)。返回被
    /// 作废的 id。候选由调用方挑(同实体 / hybrid_recall 邻居);AI 判断注入,绝不改信任档。
    pub fn reconcile_fact(
        &self,
        new_id: i64,
        candidate_ids: &[i64],
        judge: &dyn ContradictionJudge,
        at_ms: i64,
    ) -> anyhow::Result<Vec<i64>> {
        let incoming = self
            .fact_text(new_id)?
            .ok_or_else(|| anyhow::anyhow!("new fact {new_id} 不存在或非当前真相"))?;
        let mut retired = Vec::new();
        for &cid in candidate_ids {
            if cid == new_id {
                continue;
            }
            let Some(existing) = self.fact_text(cid)? else {
                continue; // 已失效/不存在的候选直接跳过
            };
            if judge.judge(&existing, &incoming)? == Verdict::IncomingSupersedes
                && self.retire_fact(cid, new_id, at_ms)?
            {
                retired.push(cid);
            }
        }
        Ok(retired)
    }

    /// Text of a CURRENT fact by id (None if missing or already invalid).
    fn fact_text(&self, id: i64) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().expect("memory store lock");
        let t = conn
            .query_row(
                "SELECT text FROM memory_fact WHERE id = ?1 AND invalid_at IS NULL",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewFact;

    fn fact(project: &str, text: &str) -> NewFact {
        NewFact {
            project: project.into(),
            scope: None,
            kind: "decision".into(),
            text: text.into(),
            entities: None,
            entity: None,
            importance: Some(5),
            valid_at: None,
            recorded_at: 1,
            provenance: None,
            trust: "员工汇报".into(),
            source_commit: None,
            source_episode_id: None,
        }
    }

    #[test]
    fn reconcile_retires_contradicting_when_incoming_supersedes() {
        let store = MemoryStore::open_in_memory().unwrap();
        let old = store.insert_fact(&fact("/r", "用 RocksDB 存状态")).unwrap();
        let new = store.insert_fact(&fact("/r", "改用 SQLite 存状态")).unwrap();
        let retired = store
            .reconcile_fact(new, &[old], &FakeJudge(Verdict::IncomingSupersedes), 100)
            .unwrap();
        assert_eq!(retired, vec![old], "contradicting old fact retired");
        let current: Vec<i64> = store
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert!(current.contains(&new), "new fact stays current");
        assert!(!current.contains(&old), "old fact no longer current");
    }

    #[test]
    fn reconcile_keeps_both_when_independent() {
        let store = MemoryStore::open_in_memory().unwrap();
        let a = store.insert_fact(&fact("/r", "用 SQLite")).unwrap();
        let b = store.insert_fact(&fact("/r", "前端用 React")).unwrap();
        let retired = store
            .reconcile_fact(b, &[a], &FakeJudge(Verdict::Independent), 100)
            .unwrap();
        assert!(retired.is_empty(), "independent facts: nothing retired");
        assert_eq!(store.current_facts("/r").unwrap().len(), 2);
    }

    #[test]
    fn reconcile_keep_existing_does_not_retire() {
        let store = MemoryStore::open_in_memory().unwrap();
        let old = store.insert_fact(&fact("/r", "权威决定:用 X")).unwrap();
        let new = store.insert_fact(&fact("/r", "听说要用 Y")).unwrap();
        let retired = store
            .reconcile_fact(new, &[old], &FakeJudge(Verdict::KeepExisting), 100)
            .unwrap();
        assert!(retired.is_empty(), "KeepExisting leaves the old fact current");
        assert_eq!(store.current_facts("/r").unwrap().len(), 2);
    }
}
