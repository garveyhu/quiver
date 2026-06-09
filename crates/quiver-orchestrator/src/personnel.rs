//! 人事 / 升级阶梯(DESIGN §13–14):按 worker 的履历(跑量 + 验收率)给它定**信任档**,
//! 经理派活时偏好高档 worker、给更难的活;掉链子就降档。纯逻辑,可测;履历从账本/episode
//! 累积、按档调度 是后续集成刀。

/// 样本不足阈值:跑够这么多次才敢评档,否则一律试用(避免一两次运气定生死)。
const MIN_SAMPLE: u32 = 3;
/// 升骨干 / 熟手的验收率门槛。
const SENIOR_RATE: f64 = 0.9;
const TRUSTED_RATE: f64 = 0.7;

/// 一个 worker 的履历(§13)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkerRecord {
    runs: u32,
    verified: u32,
    total_cost_usd: f64,
}

impl WorkerRecord {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一次 run 的结果(验收是否通过、花费)。
    pub fn record(&mut self, verified: bool, cost_usd: f64) {
        self.runs += 1;
        if verified {
            self.verified += 1;
        }
        self.total_cost_usd += cost_usd.max(0.0);
    }

    pub fn runs(&self) -> u32 {
        self.runs
    }

    pub fn verified(&self) -> u32 {
        self.verified
    }

    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_usd
    }

    /// 验收率(通过 / 总数),无 run 时为 0。
    pub fn verify_rate(&self) -> f64 {
        if self.runs == 0 {
            0.0
        } else {
            self.verified as f64 / self.runs as f64
        }
    }
}

/// 信任档(§14 升级阶梯)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerTier {
    /// 试用:样本不足或验收率低 —— 只派低风险活、严审。
    Probation,
    /// 熟手:验收率达标 —— 常规活。
    Trusted,
    /// 骨干:验收率很高 —— 难活 / 更多自治。
    Senior,
}

/// 按履历定档(§14):样本不足 → 试用;够样本后按验收率分骨干 / 熟手 / 试用。
pub fn tier_for(rec: &WorkerRecord) -> WorkerTier {
    if rec.runs < MIN_SAMPLE {
        return WorkerTier::Probation;
    }
    let rate = rec.verify_rate();
    if rate >= SENIOR_RATE {
        WorkerTier::Senior
    } else if rate >= TRUSTED_RATE {
        WorkerTier::Trusted
    } else {
        WorkerTier::Probation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(runs: u32, verified: u32) -> WorkerRecord {
        let mut r = WorkerRecord::new();
        for i in 0..runs {
            r.record(i < verified, 0.1);
        }
        r
    }

    #[test]
    fn fresh_worker_is_on_probation() {
        assert_eq!(tier_for(&WorkerRecord::new()), WorkerTier::Probation);
        // 样本不足(<3)即便全过也不急着升。
        assert_eq!(tier_for(&rec(2, 2)), WorkerTier::Probation);
    }

    #[test]
    fn high_rate_with_enough_samples_is_senior() {
        assert_eq!(tier_for(&rec(5, 5)), WorkerTier::Senior); // 1.0
        assert_eq!(tier_for(&rec(10, 9)), WorkerTier::Senior); // 0.9
    }

    #[test]
    fn mid_rate_is_trusted_low_rate_is_probation() {
        assert_eq!(tier_for(&rec(4, 3)), WorkerTier::Trusted); // 0.75
        assert_eq!(tier_for(&rec(3, 2)), WorkerTier::Probation); // 0.67 < 0.7
    }

    #[test]
    fn record_tracks_rate_and_cost() {
        let r = rec(4, 3);
        assert_eq!(r.runs(), 4);
        assert_eq!(r.verified(), 3);
        assert!((r.verify_rate() - 0.75).abs() < 1e-9);
        assert!((r.total_cost_usd() - 0.4).abs() < 1e-9);
    }
}
