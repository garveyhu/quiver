//! 子经理递归(DESIGN §5.7):经理可切一块预算给**子经理**去管一组相关任务,子经理有自己
//! 的(更小)预算 + 在途上限,可再生孙经理。关键不变量:**预算守恒** —— 切给子经理是把额度
//! 从父挪到子(不是凭空增发),整棵树的剩余预算 = 各节点未分出的之和。纯结构层,可测;
//! 真正在树上跑 [`Orchestrator`](crate::Orchestrator) 的 tick 是后续集成刀。

use crate::Budget;

/// 经理树的一个节点(父经理 / 子经理 / 孙经理,同构)。
#[derive(Debug)]
pub struct ManagerNode {
    budget: Budget,
    max_inflight: usize,
    children: Vec<ManagerNode>,
}

impl ManagerNode {
    /// 建一个经理(预算 `budget_usd`、在途上限 `max_inflight`)。
    pub fn new(budget_usd: f64, max_inflight: usize) -> Self {
        Self {
            budget: Budget::new(budget_usd),
            max_inflight,
            children: Vec::new(),
        }
    }

    /// 本节点自己未分出的剩余预算。
    pub fn remaining(&self) -> f64 {
        self.budget.remaining()
    }

    pub fn max_inflight(&self) -> usize {
        self.max_inflight
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// 切 `alloc_usd` 预算给一个新子经理(在途上限 `child_inflight`)。把额度从父挪到子
    /// (父剩余减、子拿到),预算守恒。父不够 / 非正额 → `None`。返回子经理下标。
    pub fn spawn_child(&mut self, alloc_usd: f64, child_inflight: usize) -> Option<usize> {
        if alloc_usd <= 0.0 || !self.budget.can_afford(alloc_usd) {
            return None;
        }
        self.budget.charge(alloc_usd);
        self.children.push(ManagerNode::new(alloc_usd, child_inflight));
        Some(self.children.len() - 1)
    }

    /// 取第 `i` 个子经理(可变,用来给它派活 / 让它再生孙经理)。
    pub fn child_mut(&mut self, i: usize) -> Option<&mut ManagerNode> {
        self.children.get_mut(i)
    }

    /// 本节点直接花一笔(只减,经 [`Budget`]),返回扣后本节点剩余。
    pub fn charge(&mut self, amount: f64) -> f64 {
        self.budget.charge(amount)
    }

    /// 整棵子树的剩余预算总和(层级汇总上报:本节点未分出的 + 各子树剩余)。
    pub fn total_remaining(&self) -> f64 {
        self.budget.remaining()
            + self.children.iter().map(|c| c.total_remaining()).sum::<f64>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_child_moves_budget_and_conserves_total() {
        let mut root = ManagerNode::new(10.0, 4);
        let i = root.spawn_child(3.0, 2).expect("够预算");
        assert_eq!(i, 0);
        assert_eq!(root.remaining(), 7.0, "父挪出 3");
        assert_eq!(root.child_mut(0).unwrap().remaining(), 3.0, "子拿到 3");
        assert_eq!(root.total_remaining(), 10.0, "整树预算守恒");
    }

    #[test]
    fn spawn_child_rejects_over_budget_or_nonpositive() {
        let mut root = ManagerNode::new(5.0, 4);
        assert_eq!(root.spawn_child(100.0, 2), None, "超出父预算");
        assert_eq!(root.spawn_child(0.0, 2), None, "非正额");
        assert_eq!(root.child_count(), 0);
    }

    #[test]
    fn recursion_and_spending_keep_rollup_correct() {
        let mut root = ManagerNode::new(10.0, 4);
        root.spawn_child(3.0, 2).unwrap();
        root.charge(2.0); // 父自己花 2 → 父剩 5
        // 子再生孙:从子的 3 里切 1。
        root.child_mut(0).unwrap().spawn_child(1.0, 1).unwrap();
        assert_eq!(root.remaining(), 5.0);
        // total = 父 5 + 子(2 + 孙 1)= 8。
        assert_eq!(root.total_remaining(), 8.0);
    }
}
