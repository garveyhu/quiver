//! 编排流控:有界在途(§5.6)+ 栅栏令牌对账(§5.5)+ saga 决策序号去重。
//!
//! 都是**纯内存状态机** —— 持久化(store 的 saga_step / fence 列)由上层映射,这里只管
//! 不变量:并发不超上限、过期/重复完成被栅栏挡掉、同一决策重放是 no-op。

use std::collections::{HashMap, HashSet};

/// 栅栏令牌:每次 admit 一个 worker 发一枚单调递增的票(§5.5)。完成时必须出示同一枚,
/// 否则视为过期/重复(僵尸或重试残留),拒收 —— 防重复交付。
pub type Fence = u64;

/// 有界在途追踪器(§5.6):同时在途的 worker 数不超过 `max`;每个在途节点持一枚栅栏令牌。
#[derive(Debug)]
pub struct InFlight {
    max: usize,
    next_fence: Fence,
    nodes: HashMap<String, Fence>,
}

impl InFlight {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            next_fence: 1,
            nodes: HashMap::new(),
        }
    }

    /// 当前在途数。
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 在途是否已满(§5.6 经理不该再 spawn)。
    pub fn is_full(&self) -> bool {
        self.nodes.len() >= self.max
    }

    /// 接纳一个新 worker:未满则记下并发一枚栅栏令牌,满了返回 `None`(§5.5/§5.6)。
    /// 同一 `node_id` 重复 admit 会换发新令牌(旧的随之失效)。
    pub fn admit(&mut self, node_id: impl Into<String>) -> Option<Fence> {
        let node_id = node_id.into();
        if !self.nodes.contains_key(&node_id) && self.nodes.len() >= self.max {
            return None;
        }
        let fence = self.next_fence;
        self.next_fence += 1;
        self.nodes.insert(node_id, fence);
        Some(fence)
    }

    /// 接收一次完成:只有 `fence` 与该节点当前令牌一致才认(§5.5 对账)。一致则释放在途
    /// 名额、返回 `true`;过期/重复/未知节点返回 `false`(被挡掉,不释放、不重复交付)。
    pub fn complete(&mut self, node_id: &str, fence: Fence) -> bool {
        if self.nodes.get(node_id) == Some(&fence) {
            self.nodes.remove(node_id);
            true
        } else {
            false
        }
    }

    /// 某节点当前的栅栏令牌(无则 `None`)。
    pub fn fence_of(&self, node_id: &str) -> Option<Fence> {
        self.nodes.get(node_id).copied()
    }
}

/// saga 决策账本:发单调递增的决策序号(§5 去重钥匙),并记哪些已落地 —— 同一决策重放
/// 是 no-op(崩溃恢复后重放账本不会重复执行)。
#[derive(Debug, Default)]
pub struct SagaLedger {
    next_seq: u64,
    applied: HashSet<u64>,
}

impl SagaLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// 下一个决策序号(单调递增,从 0 起)= 这条决策的去重钥匙。
    pub fn next_seq(&mut self) -> u64 {
        let s = self.next_seq;
        self.next_seq += 1;
        s
    }

    /// 标记决策 `seq` 已落地;若此前已落地返回 `false`(幂等重放:重复落地是 no-op)。
    pub fn mark_applied(&mut self, seq: u64) -> bool {
        self.applied.insert(seq)
    }

    pub fn is_applied(&self, seq: u64) -> bool {
        self.applied.contains(&seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflight_bounds_concurrency() {
        let mut f = InFlight::new(2);
        assert_eq!(f.admit("a"), Some(1));
        assert_eq!(f.admit("b"), Some(2));
        assert!(f.is_full());
        assert_eq!(f.admit("c"), None, "满了不再接纳");
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn fence_rejects_stale_completion_and_frees_on_match() {
        let mut f = InFlight::new(2);
        let fa = f.admit("a").unwrap();
        // 过期/错误令牌被挡掉,不释放名额。
        assert!(!f.complete("a", fa + 999), "错误栅栏令牌被拒");
        assert_eq!(f.len(), 1);
        assert!(!f.complete("ghost", 1), "未知节点被拒");
        // 正确令牌:认账并释放名额。
        assert!(f.complete("a", fa));
        assert_eq!(f.len(), 0);
        // 名额释放后能再接纳。
        assert!(f.admit("c").is_some());
    }

    #[test]
    fn readmit_rotates_fence_and_invalidates_old() {
        let mut f = InFlight::new(2);
        let old = f.admit("a").unwrap();
        let new = f.admit("a").unwrap(); // 重新接纳同名节点 → 换新令牌
        assert_ne!(old, new);
        assert!(!f.complete("a", old), "旧令牌已失效");
        assert!(f.complete("a", new), "新令牌有效");
    }

    #[test]
    fn saga_seq_is_monotonic_and_replay_idempotent() {
        let mut s = SagaLedger::new();
        assert_eq!(s.next_seq(), 0);
        assert_eq!(s.next_seq(), 1);
        assert_eq!(s.next_seq(), 2);
        assert!(s.mark_applied(1), "首次落地");
        assert!(!s.mark_applied(1), "重放同一决策是 no-op");
        assert!(s.is_applied(1));
        assert!(!s.is_applied(2));
    }
}
