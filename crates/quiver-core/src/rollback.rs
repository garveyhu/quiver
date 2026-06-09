//! 回滚 / 还原点(DESIGN §15 回滚)。
//!
//! 每次合并落定时记一个**还原点**(`merge_seq` + `commit_sha`),整夜跑下来就有一串可回退
//! 的锚点。出问题时"回到 seq N":解析出该回退到的目标 commit,以及哪些后续还原点会被撤销。
//!
//! 纯账本 + 解析,可测;真正把分支/worktree reset 到目标 commit 是 git 集成刀。

/// 一个还原点:某次合并落定记下的可回退锚点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePoint {
    /// 合并序号(单调递增,§5 saga/合并钉的同源序号)。
    pub merge_seq: u64,
    pub commit_sha: String,
    pub label: String,
    pub created_at_ms: i64,
}

/// 回滚计划:回退到 `target` 的 commit,撤销 `undone`(merge_seq 在 target 之后的还原点)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPlan {
    pub target: RestorePoint,
    pub undone: Vec<RestorePoint>,
}

/// 还原点日志:按 `merge_seq` 单调追加。
#[derive(Debug, Default)]
pub struct RestoreLog {
    points: Vec<RestorePoint>,
}

impl RestoreLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// 记一个还原点。`merge_seq` 必须严格大于上一个(单调),否则拒收返回 `false`。
    pub fn record(&mut self, point: RestorePoint) -> bool {
        if let Some(last) = self.points.last() {
            if point.merge_seq <= last.merge_seq {
                return false;
            }
        }
        self.points.push(point);
        true
    }

    /// 最新还原点。
    pub fn latest(&self) -> Option<&RestorePoint> {
        self.points.last()
    }

    /// 解析"回到 `to_seq`":目标 = `merge_seq ≤ to_seq` 的最新还原点;`undone` = `merge_seq
    /// > to_seq` 的还原点(会被撤销)。无任何点 ≤ to_seq(即想退到第一个点之前)→ `None`。
    pub fn resolve_rollback(&self, to_seq: u64) -> Option<RollbackPlan> {
        let target = self
            .points
            .iter()
            .filter(|p| p.merge_seq <= to_seq)
            .max_by_key(|p| p.merge_seq)?
            .clone();
        let undone = self
            .points
            .iter()
            .filter(|p| p.merge_seq > to_seq)
            .cloned()
            .collect();
        Some(RollbackPlan { target, undone })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(seq: u64, sha: &str) -> RestorePoint {
        RestorePoint {
            merge_seq: seq,
            commit_sha: sha.to_string(),
            label: format!("merge {seq}"),
            created_at_ms: seq as i64,
        }
    }

    fn log() -> RestoreLog {
        let mut l = RestoreLog::new();
        l.record(pt(1, "aaa"));
        l.record(pt(3, "bbb"));
        l.record(pt(5, "ccc"));
        l
    }

    #[test]
    fn record_enforces_monotonic_seq() {
        let mut l = RestoreLog::new();
        assert!(l.record(pt(1, "a")));
        assert!(l.record(pt(2, "b")));
        assert!(!l.record(pt(2, "dup")), "非递增 seq 拒收");
        assert!(!l.record(pt(1, "back")), "回退 seq 拒收");
        assert_eq!(l.len(), 2);
        assert_eq!(l.latest().unwrap().commit_sha, "b");
    }

    #[test]
    fn resolve_targets_latest_at_or_before_and_lists_undone() {
        let l = log();
        // 回到 seq 3:目标是 seq3(bbb),撤销 seq5(ccc)。
        let plan = l.resolve_rollback(3).unwrap();
        assert_eq!(plan.target.commit_sha, "bbb");
        assert_eq!(plan.undone.iter().map(|p| p.merge_seq).collect::<Vec<_>>(), vec![5]);
    }

    #[test]
    fn resolve_between_points_picks_earlier() {
        let l = log();
        // 回到 seq 4:无 4,取 ≤4 的最新 = seq3;撤销 seq5。
        let plan = l.resolve_rollback(4).unwrap();
        assert_eq!(plan.target.merge_seq, 3);
        assert_eq!(plan.undone.len(), 1);
    }

    #[test]
    fn resolve_before_first_point_is_none() {
        let l = log();
        assert!(l.resolve_rollback(0).is_none(), "退不到第一个还原点之前");
    }
}
