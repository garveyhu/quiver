//! 监督升级阶梯 + 只减预算(DESIGN §23 P4)。
//!
//! 一个任务连续失败时,经理沿阶梯往上爬:**重试**(同法再来)→ **同级**(换个法子/换个
//! worker)→ **升级给人**。每爬一级是一次决策,要收"决策税"(从预算扣),且**预算只减不增**
//! —— 防 agent 在坏前提上无限烧钱。两者都是纯逻辑,可测。

/// 升级阶梯的一级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    /// 同法重试(便宜,适合瞬时失败)。
    Retry,
    /// 换个法子 / 换个 worker 重做(同级横向)。
    Sibling,
    /// 升级给人(自治到头了)。
    Escalate,
}

/// 监督升级阶梯:重试用完爬同级,同级用完升级给人。
#[derive(Debug, Clone)]
pub struct EscalationLadder {
    max_retries: u32,
    max_siblings: u32,
    retries: u32,
    siblings: u32,
}

impl EscalationLadder {
    pub fn new(max_retries: u32, max_siblings: u32) -> Self {
        Self {
            max_retries,
            max_siblings,
            retries: 0,
            siblings: 0,
        }
    }

    /// 记一次失败,返回下一步该爬到哪级。重试额度先用,然后同级,都用完恒返 [`Rung::Escalate`]。
    pub fn escalate(&mut self) -> Rung {
        if self.retries < self.max_retries {
            self.retries += 1;
            Rung::Retry
        } else if self.siblings < self.max_siblings {
            self.siblings += 1;
            Rung::Sibling
        } else {
            Rung::Escalate
        }
    }

    /// 重试 + 同级额度是否都已用尽(下一步只会是升级给人)。
    pub fn exhausted(&self) -> bool {
        self.retries >= self.max_retries && self.siblings >= self.max_siblings
    }
}

/// 监督预算(§"决策税 + 只减预算"):花费/决策税只减不增,夹到 0。
#[derive(Debug, Clone)]
pub struct Budget {
    remaining_usd: f64,
}

impl Budget {
    pub fn new(total_usd: f64) -> Self {
        Self {
            remaining_usd: total_usd.max(0.0),
        }
    }

    pub fn remaining(&self) -> f64 {
        self.remaining_usd
    }

    /// 预算是否已耗尽(经理该停手 / 升级)。
    pub fn is_exhausted(&self) -> bool {
        self.remaining_usd <= 0.0
    }

    /// 还负担得起 `cost` 吗(价值闸:派活前先问)。
    pub fn can_afford(&self, cost: f64) -> bool {
        cost <= self.remaining_usd
    }

    /// 扣一笔(实际花费或决策税);**只减**,夹到 0,返回扣后余额。负数当 0 处理。
    pub fn charge(&mut self, amount: f64) -> f64 {
        self.remaining_usd = (self.remaining_usd - amount.max(0.0)).max(0.0);
        self.remaining_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_climbs_retry_then_sibling_then_escalate() {
        let mut l = EscalationLadder::new(2, 1);
        assert_eq!(l.escalate(), Rung::Retry);
        assert_eq!(l.escalate(), Rung::Retry);
        assert!(!l.exhausted());
        assert_eq!(l.escalate(), Rung::Sibling);
        assert!(l.exhausted(), "重试+同级都用完");
        assert_eq!(l.escalate(), Rung::Escalate);
        assert_eq!(l.escalate(), Rung::Escalate, "到顶后恒升级给人");
    }

    #[test]
    fn ladder_with_no_budget_for_retries_goes_straight_to_escalate() {
        let mut l = EscalationLadder::new(0, 0);
        assert_eq!(l.escalate(), Rung::Escalate);
    }

    #[test]
    fn budget_only_decreases_and_clamps_at_zero() {
        let mut b = Budget::new(1.0);
        assert!(b.can_afford(0.5));
        assert_eq!(b.charge(0.3), 0.7);
        assert_eq!(b.charge(1.0), 0.0, "扣过头夹到 0,不变负");
        assert!(b.is_exhausted());
        assert!(!b.can_afford(0.01));
        // 负数扣款当 0(不会偷偷加回去)。
        assert_eq!(b.charge(-5.0), 0.0);
    }

    #[test]
    fn budget_new_clamps_negative_total() {
        assert_eq!(Budget::new(-3.0).remaining(), 0.0);
    }
}
