//! 记忆官(§6.4):用 AI 判断把矛盾的旧事实作废,让新事实成为当前真相。
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

/// Map a verdict string (from the AI) to [`Verdict`]; unknown → `Independent`
/// (the safe default — never retire a fact on an ambiguous answer).
/// 真 AI 裁决器(claude 驱动,§0「思考全程用 claude」)在 app 层实现后用它解析;
/// 千问版 QwenJudge 已删(千问只配做 embedding,不做思考)。
pub fn parse_verdict(s: &str) -> Verdict {
    match s.trim().to_ascii_lowercase().as_str() {
        "incoming_supersedes" => Verdict::IncomingSupersedes,
        "keep_existing" => Verdict::KeepExisting,
        _ => Verdict::Independent,
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

    /// 记忆官核对(§6.4):新事实 `new_id` 对一组候选旧事实逐一过 `judge`,凡裁为
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

    /// 便捷:新事实 `new_id` 自动挑候选(§6.4 矛盾位点 = 同一 `entity` 的其它当前事实)
    /// 再 [`reconcile_fact`](Self::reconcile_fact)。新事实没 entity 时返回空(用显式
    /// `reconcile_fact` 传候选)。语义邻居(vector)选候选是后续精化。
    pub fn reconcile_new_fact(
        &self,
        new_id: i64,
        judge: &dyn ContradictionJudge,
        at_ms: i64,
    ) -> anyhow::Result<Vec<i64>> {
        let (project, entity): (String, Option<String>) = {
            let conn = self.conn.lock().expect("memory store lock");
            let row = conn
                .query_row(
                    "SELECT project, entity FROM memory_fact WHERE id = ?1 AND invalid_at IS NULL",
                    params![new_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            row.ok_or_else(|| anyhow::anyhow!("new fact {new_id} 不存在或非当前真相"))?
        };
        let Some(entity) = entity else {
            return Ok(Vec::new()); // 无实体 → 无自动候选
        };
        let candidates: Vec<i64> = self
            .current_facts(&project)?
            .into_iter()
            .filter(|f| f.id != new_id && f.entity.as_deref() == Some(entity.as_str()))
            .map(|f| f.id)
            .collect();
        self.reconcile_fact(new_id, &candidates, judge, at_ms)
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

    /// **机械矛盾消解**(§6.4 无 AI):新事实 `new_id` 有 `entity` 时,作废同 (project, entity)
    /// 的所有**可信度严格更低**的旧当前事实 —— 同一主题/模块上,更可信的新事实顶替不那么
    /// 可信的旧事实(如 CEO 权威事实顶替员工汇报)。纯按可信度档比较,不做语义判断,所以免费、
    /// 确定、可单测。返回被作废的 id。新事实无 entity → 不动(无法机械框定"同一主题")。
    pub fn supersede_lower_same_entity(
        &self,
        new_id: i64,
        at_ms: i64,
    ) -> anyhow::Result<Vec<i64>> {
        let (project, entity, trust): (String, Option<String>, String) = {
            let conn = self.conn.lock().expect("memory store lock");
            let row = conn
                .query_row(
                    "SELECT project, entity, trust FROM memory_fact WHERE id = ?1 AND invalid_at IS NULL",
                    params![new_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            match row {
                Some(v) => v,
                None => return Ok(Vec::new()), // 新事实不存在/已失效
            }
        };
        let Some(entity) = entity else {
            return Ok(Vec::new());
        };
        let new_rank = trust_rank(&trust);
        let candidates: Vec<i64> = self
            .current_facts(&project)?
            .into_iter()
            .filter(|f| {
                f.id != new_id
                    && f.entity.as_deref() == Some(entity.as_str())
                    && trust_rank(&f.trust) < new_rank
            })
            .map(|f| f.id)
            .collect();
        let mut retired = Vec::new();
        for cid in candidates {
            if self.retire_fact(cid, new_id, at_ms)? {
                retired.push(cid);
            }
        }
        Ok(retired)
    }
}

/// 可信度档的机械排序(§6.2,数字大=更可信)。未知档当最低。
fn trust_rank(t: &str) -> u8 {
    match t {
        "权威" => 5,
        "已验证·机械" => 4,
        "已验证·印证" => 3,
        "员工汇报" => 2,
        _ => 1, // 不可信 / 未知
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

    fn fact_et(project: &str, text: &str, entity: &str, trust: &str) -> NewFact {
        NewFact {
            entity: Some(entity.into()),
            trust: trust.into(),
            ..fact(project, text)
        }
    }

    #[test]
    fn supersede_lower_same_entity_retires_only_lower_trust_same_entity() {
        let store = MemoryStore::open_in_memory().unwrap();
        // 同主题「数据库」:一条员工汇报的旧事实 + 一条别主题的 + 一条同主题但已验证·机械(更高)。
        let old_low = store.insert_fact(&fact_et("/r", "数据库用 RocksDB", "数据库", "员工汇报")).unwrap();
        let other = store.insert_fact(&fact_et("/r", "前端用 React", "前端", "员工汇报")).unwrap();
        let old_high = store.insert_fact(&fact_et("/r", "数据库已机械验证用 X", "数据库", "已验证·机械")).unwrap();
        // CEO 权威事实(同主题「数据库」)进来。
        let new = store.insert_fact(&fact_et("/r", "数据库迁到 SQLite", "数据库", "权威")).unwrap();
        let retired = store.supersede_lower_same_entity(new, 100).unwrap();
        // 只作废同主题、可信度更低的(员工汇报);别主题不动;同主题但≥的(已验证·机械<权威 → 也作废)。
        assert!(retired.contains(&old_low), "同主题低档旧事实被作废");
        assert!(retired.contains(&old_high), "同主题更低于权威的也被作废");
        let current: Vec<i64> = store.current_facts("/r").unwrap().into_iter().map(|f| f.id).collect();
        assert!(current.contains(&new) && current.contains(&other), "新事实+别主题保留");
        assert!(!current.contains(&old_low) && !current.contains(&old_high), "同主题旧的失效");
    }

    #[test]
    fn supersede_noop_without_entity() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert_fact(&fact("/r", "数据库用 RocksDB")).unwrap(); // 无 entity
        let new = store.insert_fact(&fact("/r", "数据库用 SQLite")).unwrap(); // 无 entity
        assert!(store.supersede_lower_same_entity(new, 100).unwrap().is_empty(), "无 entity 不作废");
        assert_eq!(store.current_facts("/r").unwrap().len(), 2);
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

    fn fact_e(project: &str, text: &str, entity: &str) -> NewFact {
        NewFact {
            kind: "笔记".into(),
            entity: Some(entity.into()),
            ..fact(project, text)
        }
    }

    #[test]
    fn reconcile_new_fact_auto_selects_same_entity_only() {
        let store = MemoryStore::open_in_memory().unwrap();
        let old = store.insert_fact(&fact_e("/r", "A 用 RocksDB", "moduleA")).unwrap();
        let new = store.insert_fact(&fact_e("/r", "A 改用 SQLite", "moduleA")).unwrap();
        // 不同实体的事实绝不被当候选。
        let other = store.insert_fact(&fact_e("/r", "B 用 Redis", "moduleB")).unwrap();

        let retired = store
            .reconcile_new_fact(new, &FakeJudge(Verdict::IncomingSupersedes), 100)
            .unwrap();
        assert_eq!(retired, vec![old], "只作废同实体的旧事实");
        let current: Vec<i64> = store
            .current_facts("/r")
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert!(current.contains(&other), "别的实体的事实不受影响");
        assert!(!current.contains(&old));
    }

    #[test]
    fn reconcile_new_fact_no_entity_is_noop() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert_fact(&fact("/r", "用 SQLite")).unwrap();
        let new = store.insert_fact(&fact("/r", "用 RocksDB")).unwrap();
        let retired = store
            .reconcile_new_fact(new, &FakeJudge(Verdict::IncomingSupersedes), 100)
            .unwrap();
        assert!(retired.is_empty(), "无 entity → 不自动选候选");
        assert_eq!(store.current_facts("/r").unwrap().len(), 2);
    }

    #[test]
    fn parse_verdict_maps_and_defaults_safe() {
        assert_eq!(parse_verdict("incoming_supersedes"), Verdict::IncomingSupersedes);
        assert_eq!(parse_verdict(" Keep_Existing "), Verdict::KeepExisting);
        assert_eq!(parse_verdict("independent"), Verdict::Independent);
        assert_eq!(parse_verdict("???"), Verdict::Independent, "unknown → 安全默认不作废");
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
