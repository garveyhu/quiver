//! 价值闸 + 配额(DESIGN §23 P4 "配额感知 / 价值闸")。
//!
//! 派活前过一道闸:**付得起吗**(预算)、**值不值**(价值阈值)、**本窗配额满没**(配额感知)。
//! 防经理见任务就派、把预算烧在低价值活上。纯策略,可测。缓存优先(命中就复用、不重跑)
//! 需要结果缓存,是后续刀。

use crate::Budget;

/// 价值闸裁决。
#[derive(Debug, Clone, PartialEq)]
pub enum GateVerdict {
    /// 派 —— 值得且付得起、配额够。
    Admit,
    /// 缓一缓 —— 值得也付得起,但本窗配额用满了,等下个窗。
    Defer { reason: String },
    /// 别派 —— 付不起,或价值低于阈值。
    Reject { reason: String },
}

/// 价值闸:按 预算 / 任务价值 / 配额 决定派不派。`value` 是调用方给的任务价值分
/// (优先级 / 预期收益),`est_cost` 是预估花费。
#[derive(Debug, Clone)]
pub struct ValueGate {
    quota_per_window: u32,
    spawned_this_window: u32,
    min_value: f64,
}

impl ValueGate {
    /// `quota_per_window` = 每窗最多派几个;`min_value` = 低于此价值不派。
    pub fn new(quota_per_window: u32, min_value: f64) -> Self {
        Self {
            quota_per_window,
            spawned_this_window: 0,
            min_value,
        }
    }

    /// 派活前问一句:给定预算、预估成本、任务价值,该不该派。
    pub fn evaluate(&self, budget: &Budget, est_cost: f64, value: f64) -> GateVerdict {
        if !budget.can_afford(est_cost) {
            GateVerdict::Reject {
                reason: format!("预算不够:需 ${est_cost:.2},剩 ${:.2}", budget.remaining()),
            }
        } else if value < self.min_value {
            GateVerdict::Reject {
                reason: format!("价值 {value:.2} 低于阈值 {:.2}", self.min_value),
            }
        } else if self.spawned_this_window >= self.quota_per_window {
            GateVerdict::Defer {
                reason: format!("本窗配额已满({}/{})", self.spawned_this_window, self.quota_per_window),
            }
        } else {
            GateVerdict::Admit
        }
    }

    /// 记一次实际派活(占一个配额名额)。
    pub fn record_spawn(&mut self) {
        self.spawned_this_window += 1;
    }

    /// 进入新配额窗(清零本窗计数)。
    pub fn reset_window(&mut self) {
        self.spawned_this_window = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> ValueGate {
        ValueGate::new(2, 1.0)
    }

    #[test]
    fn admits_when_affordable_valuable_and_under_quota() {
        let b = Budget::new(10.0);
        assert_eq!(gate().evaluate(&b, 1.0, 5.0), GateVerdict::Admit);
    }

    #[test]
    fn rejects_when_unaffordable() {
        let b = Budget::new(0.5);
        assert!(matches!(gate().evaluate(&b, 1.0, 5.0), GateVerdict::Reject { .. }));
    }

    #[test]
    fn rejects_when_value_below_threshold() {
        let b = Budget::new(10.0);
        assert!(matches!(gate().evaluate(&b, 1.0, 0.5), GateVerdict::Reject { .. }));
    }

    #[test]
    fn defers_when_quota_full() {
        let b = Budget::new(10.0);
        let mut g = gate();
        g.record_spawn();
        g.record_spawn(); // quota = 2, now full
        assert!(matches!(g.evaluate(&b, 1.0, 5.0), GateVerdict::Defer { .. }));
        g.reset_window();
        assert_eq!(g.evaluate(&b, 1.0, 5.0), GateVerdict::Admit, "新窗放行");
    }
}
