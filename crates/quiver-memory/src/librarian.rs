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

/// Map a verdict string (from the LLM) to [`Verdict`]; unknown → `Independent`
/// (the safe default — never retire a fact on an ambiguous answer).
#[cfg_attr(not(feature = "qwen"), allow(dead_code))] // only QwenJudge (or tests) use it
fn parse_verdict(s: &str) -> Verdict {
    match s.trim().to_ascii_lowercase().as_str() {
        "incoming_supersedes" => Verdict::IncomingSupersedes,
        "keep_existing" => Verdict::KeepExisting,
        _ => Verdict::Independent,
    }
}

/// Extract the first balanced-ish `{…}` block from an LLM reply (tolerates code
/// fences / prose around the JSON).
#[cfg_attr(not(feature = "qwen"), allow(dead_code))] // only QwenJudge (or tests) use it
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start {
        Some(&s[start..=end])
    } else {
        None
    }
}

/// 真 LLM 矛盾判断器(§21):问千问两条事实是否矛盾、谁当道,要 JSON 裁决。behind `qwen`
/// feature(reqwest)。key 运行时从 ~/.agents/resources.json 读,绝不打印/提交。
#[cfg(feature = "qwen")]
pub struct QwenJudge {
    creds: crate::QwenCreds,
    model: String,
    client: reqwest::blocking::Client,
}

#[cfg(feature = "qwen")]
impl QwenJudge {
    /// `profile` = `"personal"`/`"company"`; `model` e.g. `"qwen-plus"`/`"qwen-turbo"`.
    pub fn new(profile: &str, model: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            creds: crate::load_qwen_creds(profile)?,
            model: model.into(),
            client: reqwest::blocking::Client::new(),
        })
    }
}

#[cfg(feature = "qwen")]
impl ContradictionJudge for QwenJudge {
    fn judge(&self, existing: &str, incoming: &str) -> anyhow::Result<Verdict> {
        use anyhow::Context;
        let url = format!("{}/chat/completions", self.creds.base_url.trim_end_matches('/'));
        let sys = "你是项目记忆库的图书管理员。判断两条关于同一项目的事实是否矛盾,以及该让哪条当道。只输出一个 JSON 对象,不要解释。";
        let user = format!(
            "已有事实:{existing}\n新来事实:{incoming}\n\n输出 {{\"verdict\":\"X\"}},X 取:\
             independent(不冲突,各自成立)、incoming_supersedes(矛盾,新的取代旧的)、\
             keep_existing(矛盾,但旧的更可信,保留旧的)。"
        );
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": sys},
                {"role": "user", "content": user}
            ],
            "temperature": 0
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.creds.api_key)
            .json(&body)
            .send()
            .context("千问判断请求失败")?
            .error_for_status()
            .context("千问判断返回错误状态")?;
        let parsed: serde_json::Value = resp.json().context("千问判断响应非 JSON")?;
        let content = parsed["choices"][0]["message"]["content"]
            .as_str()
            .context("千问响应缺 choices[0].message.content")?;
        let obj = extract_json_object(content).context("千问回复里找不到 JSON 对象")?;
        let v: serde_json::Value = serde_json::from_str(obj).context("裁决 JSON 解析失败")?;
        let verdict = v["verdict"].as_str().context("裁决 JSON 缺 verdict")?;
        Ok(parse_verdict(verdict))
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
    fn extract_json_object_tolerates_fences_and_prose() {
        assert_eq!(
            extract_json_object("```json\n{\"verdict\":\"x\"}\n```"),
            Some("{\"verdict\":\"x\"}")
        );
        assert_eq!(extract_json_object("没有 JSON"), None);
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
