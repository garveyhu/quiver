//! 熔断器(DESIGN §23 P3 "熔断"):连续失败到阈值就跳闸,暂停派活;冷却一段后半开放一次
//! 试探,成功则复位、失败则重新跳闸。防"坏环境/坏前提"下经理无脑烧钱重试。
//!
//! 纯状态机:不读时钟,调用方把 `now_ms` 传进来(便于测试控时)。

/// 熔断器三态(对外观测用;内部由 `opened_at` + 冷却推导)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 正常放行。
    Closed,
    /// 已跳闸:冷却中,拒绝放行。
    Open,
    /// 冷却已过:放一次试探(成功复位 / 失败重新跳闸)。
    HalfOpen,
}

/// 连续失败熔断器。`threshold` 次连续失败跳闸,跳闸后 `cooldown_ms` 内拒绝,之后半开。
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    threshold: u32,
    cooldown_ms: i64,
    failures: u32,
    opened_at: Option<i64>,
}

impl CircuitBreaker {
    /// `threshold` = 跳闸所需连续失败数(≥1);`cooldown_ms` = 跳闸后冷却时长。
    pub fn new(threshold: u32, cooldown_ms: i64) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown_ms,
            failures: 0,
            opened_at: None,
        }
    }

    /// 当前态(`now_ms` 用于判断 Open 是否已冷却到 HalfOpen)。
    pub fn state(&self, now_ms: i64) -> CircuitState {
        match self.opened_at {
            None => CircuitState::Closed,
            Some(t) if now_ms - t >= self.cooldown_ms => CircuitState::HalfOpen,
            Some(_) => CircuitState::Open,
        }
    }

    /// 现在能否放行一次尝试:Closed / HalfOpen 放行,Open(冷却中)拒绝。
    pub fn allow(&self, now_ms: i64) -> bool {
        self.state(now_ms) != CircuitState::Open
    }

    /// 记一次成功:清零失败、复位为 Closed。
    pub fn on_success(&mut self) {
        self.failures = 0;
        self.opened_at = None;
    }

    /// 记一次失败(`now_ms`):连续失败计数 +1,达阈值则(重新)跳闸、刷新冷却起点。
    pub fn on_failure(&mut self, now_ms: i64) {
        self.failures += 1;
        if self.failures >= self.threshold {
            self.opened_at = Some(now_ms);
        }
    }

    /// 当前连续失败数(观测用)。
    pub fn failure_count(&self) -> u32 {
        self.failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_until_threshold_then_trips() {
        let mut cb = CircuitBreaker::new(3, 1000);
        assert!(cb.allow(0));
        cb.on_failure(0);
        cb.on_failure(0);
        assert!(cb.allow(0), "未到阈值仍放行");
        assert_eq!(cb.state(0), CircuitState::Closed);
        cb.on_failure(0); // 第 3 次 → 跳闸
        assert_eq!(cb.state(0), CircuitState::Open);
        assert!(!cb.allow(0), "跳闸后冷却中拒绝");
    }

    #[test]
    fn half_opens_after_cooldown() {
        let mut cb = CircuitBreaker::new(1, 1000);
        cb.on_failure(0); // threshold=1 → 立刻跳闸
        assert_eq!(cb.state(500), CircuitState::Open, "冷却中");
        assert!(!cb.allow(500));
        assert_eq!(cb.state(1000), CircuitState::HalfOpen, "冷却到点 → 半开");
        assert!(cb.allow(1000), "半开放一次试探");
    }

    #[test]
    fn success_resets_and_failed_trial_reopens() {
        let mut cb = CircuitBreaker::new(1, 1000);
        cb.on_failure(0);
        // 半开试探成功 → 复位 Closed。
        assert_eq!(cb.state(1000), CircuitState::HalfOpen);
        cb.on_success();
        assert_eq!(cb.state(1000), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
        // 再次失败 → 重新跳闸,冷却起点刷新到新 now。
        cb.on_failure(2000);
        assert_eq!(cb.state(2500), CircuitState::Open, "新冷却窗");
        assert_eq!(cb.state(3000), CircuitState::HalfOpen);
    }
}
