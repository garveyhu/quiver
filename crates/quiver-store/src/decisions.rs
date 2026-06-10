//! 决策日志持久层(DESIGN §20 `decision_log` 裁剪版 / §10 复盘室地基)。
//!
//! 经理每一拍真实决策落一行 —— 决策流不再只是内存事件(重启即丢):工作台打开即可回看
//! 历史,"它当初为啥这么干"有据可查。只追加,不更新不删除。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::Store;

/// 一拍已落库的决策(读形,camelCase 直喂前端决策流)。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRecord {
    pub project: String,
    /// 循环内决策序号(同一循环单调;跨循环会重置,排序用 ts_ms)。
    pub seq: i64,
    pub action: String,
    pub reason: Option<String>,
    pub node_id: Option<String>,
    pub task_id: Option<String>,
    /// 派的具体活(spawn/裁决时有)。
    pub task_prompt: Option<String>,
    pub executed: bool,
    pub inflight: i64,
    pub queued: i64,
    pub max_inflight: i64,
    pub budget_remaining_usd: f64,
    pub ts_ms: i64,
}

impl Store {
    /// 落一拍决策(只追加,best-effort 由调用方兜)。
    pub fn record_decision(&self, d: &DecisionRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO decision_log
                (project, seq, action, reason, node_id, task_id, task_prompt,
                 executed, inflight, queued, max_inflight, budget_remaining_usd, ts_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                d.project,
                d.seq,
                d.action,
                d.reason,
                d.node_id,
                d.task_id,
                d.task_prompt,
                d.executed,
                d.inflight,
                d.queued,
                d.max_inflight,
                d.budget_remaining_usd,
                d.ts_ms,
            ],
        )?;
        Ok(())
    }

    /// 某 project 已落库的最大决策序号(无则 None)。经理循环重启时从 max+1 续编,
    /// 序号跨重启单调(§5.2 去重钥匙不回卷)。
    pub fn max_decision_seq(&self, project: &str) -> anyhow::Result<Option<i64>> {
        let conn = self.conn.lock().expect("store lock");
        let v: Option<i64> = conn.query_row(
            "SELECT MAX(seq) FROM decision_log WHERE project = ?1",
            params![project],
            |r| r.get(0),
        )?;
        Ok(v)
    }

    /// 某 project 最近 `limit` 拍决策(最新在前),工作台打开时回填历史。
    pub fn decisions_for_project(
        &self,
        project: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<DecisionRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT project, seq, action, reason, node_id, task_id, task_prompt,
                    executed, inflight, queued, max_inflight, budget_remaining_usd, ts_ms
             FROM decision_log WHERE project = ?1
             ORDER BY ts_ms DESC, seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], |r| {
            Ok(DecisionRecord {
                project: r.get(0)?,
                seq: r.get(1)?,
                action: r.get(2)?,
                reason: r.get(3)?,
                node_id: r.get(4)?,
                task_id: r.get(5)?,
                task_prompt: r.get(6)?,
                executed: r.get(7)?,
                inflight: r.get(8)?,
                queued: r.get(9)?,
                max_inflight: r.get(10)?,
                budget_remaining_usd: r.get(11)?,
                ts_ms: r.get(12)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(project: &str, seq: i64, action: &str, ts: i64) -> DecisionRecord {
        DecisionRecord {
            project: project.into(),
            seq,
            action: action.into(),
            reason: Some("有排队".into()),
            node_id: Some("node-0".into()),
            task_id: Some("t1".into()),
            task_prompt: Some("做导出".into()),
            executed: true,
            inflight: 1,
            queued: 0,
            max_inflight: 1,
            budget_remaining_usd: 1_000_000.0,
            ts_ms: ts,
        }
    }

    #[test]
    fn record_and_list_newest_first_scoped_by_project() {
        let store = Store::open_in_memory().unwrap();
        store.record_decision(&rec("/a", 0, "spawn", 10)).unwrap();
        store.record_decision(&rec("/a", 1, "deliver", 20)).unwrap();
        store.record_decision(&rec("/b", 0, "noop", 15)).unwrap();

        let a = store.decisions_for_project("/a", 10).unwrap();
        assert_eq!(a.len(), 2, "只取本 project");
        assert_eq!(a[0].action, "deliver", "最新在前");
        assert_eq!(a[1].action, "spawn");
        assert_eq!(a[0].task_prompt.as_deref(), Some("做导出"));
        // limit 生效。
        assert_eq!(store.decisions_for_project("/a", 1).unwrap().len(), 1);
    }
}
