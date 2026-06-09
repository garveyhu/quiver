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
        facts.truncate(fact_limit);
        let recent_episodes = self.episodes_for_project(project, episode_limit as i64)?;
        Ok(Brief {
            project: project.to_string(),
            facts,
            recent_episodes,
        })
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

    #[test]
    fn brief_empty_project_renders_placeholders() {
        let store = MemoryStore::open_in_memory().unwrap();
        let b = store.brief("/empty", 5, 5).unwrap();
        assert!(b.facts.is_empty() && b.recent_episodes.is_empty());
        let text = b.to_text();
        assert!(text.contains("(暂无)"));
    }
}
