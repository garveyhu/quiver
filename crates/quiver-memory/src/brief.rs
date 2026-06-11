//! The manager brief (DESIGN §6 简报): the compact memory snapshot the manager
//! reads to understand a project before deciding — current-truth facts (most
//! important first) plus the most recent episodes (what just happened). P1 builds
//! it from the simple importance/recency order; §6.7 hybrid recall ranking and a
//! query-focused brief are later slices.

use crate::{EpisodeRecord, FactRecord, MemoryStore};

/// A project's memory snapshot for context injection (§6).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub project: String,
    /// Current-truth facts (`invalid_at IS NULL`), most important first, capped.
    pub facts: Vec<FactRecord>,
    /// The most recent episodes (newest first), capped.
    pub recent_episodes: Vec<EpisodeRecord>,
}

impl Brief {
    /// Render as a compact text block for injecting into a manager's context. A
    /// neutral, stable shape (the AI manager isn't built yet — P0 uses a Rust
    /// strategy — so this is the seam, kept simple and deterministic).
    pub fn to_text(&self) -> String {
        let mut out = format!("# 项目记忆简报: {}\n", self.project);

        out.push_str(&format!("\n## 当前事实 ({})\n", self.facts.len()));
        if self.facts.is_empty() {
            out.push_str("(暂无)\n");
        } else {
            for f in &self.facts {
                out.push_str(&format!(
                    "- [{}|重要度{}] {}\n",
                    f.trust, f.importance, f.text
                ));
            }
        }

        out.push_str(&format!("\n## 近期 episode ({})\n", self.recent_episodes.len()));
        if self.recent_episodes.is_empty() {
            out.push_str("(暂无)\n");
        } else {
            for e in &self.recent_episodes {
                let verdict = e.verify_result.as_deref().unwrap_or("?");
                let summary = e.summary.as_deref().unwrap_or("");
                out.push_str(&format!("- [{verdict}] {summary}\n"));
            }
        }
        out
    }
}

impl MemoryStore {
    /// Assemble a [`Brief`] for `project`: up to `fact_limit` current-truth facts
    /// (importance/recency order) + up to `episode_limit` most-recent episodes.
    pub fn brief(
        &self,
        project: &str,
        fact_limit: usize,
        episode_limit: usize,
    ) -> anyhow::Result<Brief> {
        let mut facts = self.current_facts(project)?;
        // §6 可信度感知选取:current_facts 是纯 importance 序;进 brief 的有限名额前,按
        // (importance + 可信度档)综合分重排 —— 别让低可信「员工汇报」靠高 importance 挤掉
        // 「权威/机械」硬事实,让经理读到的简报优先是可放心依赖的(配合经理按可信度分级采信)。
        facts.sort_by(|a, b| {
            let key = |f: &FactRecord| f.importance + trust_rank(&f.trust) * 2;
            key(b).cmp(&key(a)).then(b.recorded_at.cmp(&a.recorded_at))
        });
        facts.truncate(fact_limit);
        let recent_episodes = self.episodes_for_project(project, episode_limit as i64)?;
        Ok(Brief {
            project: project.to_string(),
            facts,
            recent_episodes,
        })
    }
}

/// §6.2 可信度档的序数权重(高=更可信),用于 brief 选取加权。与 facts::recall 的 trust_weight 同序:
/// 权威 > 已验证·* > 员工汇报 > 不可信/unknown。
fn trust_rank(trust: &str) -> i64 {
    match trust {
        "权威" => 5,
        "已验证·机械" | "已验证·印证" => 4,
        "员工汇报" => 2,
        _ => 1, // 不可信 / unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewEpisode, NewFact};

    fn fact(project: &str, text: &str, importance: i64) -> NewFact {
        NewFact {
            project: project.into(),
            scope: None,
            kind: "decision".into(),
            text: text.into(),
            entities: None,
            entity: None,
            importance: Some(importance),
            valid_at: None,
            recorded_at: importance, // vary so ordering is deterministic in tests
            provenance: None,
            trust: "员工汇报".into(),
            source_commit: None,
            source_episode_id: None,
        }
    }

    #[test]
    fn brief_caps_and_orders_and_isolates_project() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert_fact(&fact("/r", "top", 9)).unwrap();
        store.insert_fact(&fact("/r", "mid", 5)).unwrap();
        store.insert_fact(&fact("/r", "low", 1)).unwrap();
        store.insert_fact(&fact("/other", "elsewhere", 9)).unwrap();
        store
            .record_episode(&NewEpisode {
                project: "/r".into(),
                verify_result: Some("verified".into()),
                summary: Some("did a thing".into()),
                created_at: 1,
                ..Default::default()
            })
            .unwrap();

        let b = store.brief("/r", 2, 5).unwrap();
        // fact_limit=2 keeps the two most important; other project excluded.
        let texts: Vec<&str> = b.facts.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, vec!["top", "mid"]);
        assert_eq!(b.recent_episodes.len(), 1);
        assert_eq!(b.recent_episodes[0].verify_result.as_deref(), Some("verified"));

        // to_text mentions the project, a fact, and the episode verdict.
        let text = b.to_text();
        assert!(text.contains("/r"));
        assert!(text.contains("top"));
        assert!(text.contains("verified"));
    }

    /// 带可信度的事实构造:同 fact() 但指定 trust 档。
    fn fact_trust(project: &str, text: &str, importance: i64, trust: &str) -> NewFact {
        NewFact { trust: trust.into(), ..fact(project, text, importance) }
    }

    #[test]
    fn brief_prefers_trust_over_raw_importance_in_limited_slots() {
        let store = MemoryStore::open_in_memory().unwrap();
        // 高 importance 的「员工汇报」(低可信) vs 低 importance 的「权威」(高可信)。
        store.insert_fact(&fact_trust("/r", "员工说用 RocksDB", 9, "员工汇报")).unwrap();
        store.insert_fact(&fact_trust("/r", "CEO 定:用 SQLite", 6, "权威")).unwrap();
        // 只有 1 个名额 → 综合分(importance+可信度档×2):权威 6+10=16 > 员工汇报 9+4=13。
        let b = store.brief("/r", 1, 5).unwrap();
        assert_eq!(b.facts.len(), 1);
        assert!(b.facts[0].text.contains("SQLite"), "高可信的权威事实优先进有限名额,不被低可信带偏");
    }

    #[test]
    fn brief_empty_project_renders_placeholders() {
        let store = MemoryStore::open_in_memory().unwrap();
        let b = store.brief("/empty", 5, 5).unwrap();
        assert!(b.facts.is_empty() && b.recent_episodes.is_empty());
        let text = b.to_text();
        assert!(text.contains("(暂无)"));
    }
}
