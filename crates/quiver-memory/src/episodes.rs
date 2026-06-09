//! Episode records (DESIGN §6.2): one row per run, bound to its commit + verify
//! result + diff. Mechanical (no AI) — the durable "what happened" the librarian
//! later distills into facts (§6.4). Append-only.

use rusqlite::params;

use crate::MemoryStore;

/// A new episode to record. All run-provenance fields are optional except the
/// project it belongs to and when it was recorded — a run may be in-flight (no
/// commit yet) or never have merged.
#[derive(Clone, Debug, Default)]
pub struct NewEpisode {
    pub project: String,
    /// The agent node that produced it (orchestration.db id); `None` in the P0
    /// single-manager era.
    pub node_id: Option<String>,
    pub task_id: Option<String>,
    pub commit_sha: Option<String>,
    /// The merge-train sequence (§6.4 ordering key); `None` until merged.
    pub merge_seq: Option<i64>,
    pub verify_result: Option<String>,
    pub diff_stat: Option<String>,
    pub summary: Option<String>,
    pub created_at: i64,
}

/// A stored episode row (read shape).
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeRecord {
    pub id: i64,
    pub project: String,
    pub node_id: Option<String>,
    pub task_id: Option<String>,
    pub commit_sha: Option<String>,
    pub merge_seq: Option<i64>,
    pub verify_result: Option<String>,
    pub diff_stat: Option<String>,
    pub summary: Option<String>,
    pub created_at: i64,
}

impl MemoryStore {
    /// Append an episode; returns its new row id (usable as a fact's
    /// `source_episode_id`). Append-only — episodes are never updated or deleted.
    pub fn record_episode(&self, ep: &NewEpisode) -> anyhow::Result<i64> {
        let conn = self.conn.lock().expect("memory store lock");
        conn.execute(
            "INSERT INTO episode
                (project, node_id, task_id, commit_sha, merge_seq, verify_result,
                 diff_stat, summary, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                ep.project,
                ep.node_id,
                ep.task_id,
                ep.commit_sha,
                ep.merge_seq,
                ep.verify_result,
                ep.diff_stat,
                ep.summary,
                ep.created_at,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// The most-recent episodes for `project`, newest first, capped at `limit`.
    pub fn episodes_for_project(
        &self,
        project: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<EpisodeRecord>> {
        let conn = self.conn.lock().expect("memory store lock");
        let mut stmt = conn.prepare(
            "SELECT id, project, node_id, task_id, commit_sha, merge_seq,
                    verify_result, diff_stat, summary, created_at
             FROM episode WHERE project = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![project, limit], |row| {
                Ok(EpisodeRecord {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    node_id: row.get(2)?,
                    task_id: row.get(3)?,
                    commit_sha: row.get(4)?,
                    merge_seq: row.get(5)?,
                    verify_result: row.get(6)?,
                    diff_stat: row.get(7)?,
                    summary: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_episode_round_trips_newest_first() {
        let store = MemoryStore::open_in_memory().unwrap();
        let id1 = store
            .record_episode(&NewEpisode {
                project: "/r".into(),
                commit_sha: Some("abc".into()),
                verify_result: Some("passed".into()),
                created_at: 1,
                ..Default::default()
            })
            .unwrap();
        let id2 = store
            .record_episode(&NewEpisode {
                project: "/r".into(),
                commit_sha: Some("def".into()),
                created_at: 2,
                ..Default::default()
            })
            .unwrap();
        assert_ne!(id1, id2, "each episode gets a distinct id");

        let eps = store.episodes_for_project("/r", 10).unwrap();
        assert_eq!(eps.len(), 2);
        // Newest first.
        assert_eq!(eps[0].commit_sha.as_deref(), Some("def"));
        assert_eq!(eps[1].verify_result.as_deref(), Some("passed"));
        // Other project is isolated.
        assert!(store.episodes_for_project("/other", 10).unwrap().is_empty());
    }
}
