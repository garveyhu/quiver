//! 观测 / 指标聚合(DESIGN §10–12,OTel GenAI 风格)。
//!
//! 把多次 run 的样本聚成一组指标:**验收率**(=自治度核心指标,§12 SWE-bench/自治度)、
//! 总花费 / 总 tokens、时延 p50 / p95。纯聚合,可测;采集样本(从账本/episode 读)和导出
//! OTel 是后续集成刀。

/// 一次 run 的可观测样本。
#[derive(Debug, Clone, Copy)]
pub struct RunSample {
    pub cost_usd: f64,
    pub tokens: u64,
    pub duration_ms: u64,
    /// verify 是否通过(自治交付的关键信号)。
    pub verified: bool,
}

/// 一组 run 聚合出的指标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub runs: u64,
    pub verified: u64,
    pub failed: u64,
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub p50_duration_ms: u64,
    pub p95_duration_ms: u64,
}

impl Metrics {
    /// 验收率(通过 / 总数),无 run 时为 0。§12 自治度核心。
    pub fn verify_rate(&self) -> f64 {
        if self.runs == 0 {
            0.0
        } else {
            self.verified as f64 / self.runs as f64
        }
    }

    /// 单次平均花费,无 run 时为 0。
    pub fn avg_cost_usd(&self) -> f64 {
        if self.runs == 0 {
            0.0
        } else {
            self.total_cost_usd / self.runs as f64
        }
    }
}

/// 最近排名法百分位:`p` ∈ [0,100],`sorted` 升序。空切片返回 0。
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    let rank = ((p / 100.0) * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted[idx]
}

/// 把样本聚成 [`Metrics`]。
pub fn aggregate(samples: &[RunSample]) -> Metrics {
    let runs = samples.len() as u64;
    let verified = samples.iter().filter(|s| s.verified).count() as u64;
    let total_cost_usd = samples.iter().map(|s| s.cost_usd).sum();
    let total_tokens = samples.iter().map(|s| s.tokens).sum();
    let mut durations: Vec<u64> = samples.iter().map(|s| s.duration_ms).collect();
    durations.sort_unstable();
    Metrics {
        runs,
        verified,
        failed: runs - verified,
        total_cost_usd,
        total_tokens,
        p50_duration_ms: percentile(&durations, 50.0),
        p95_duration_ms: percentile(&durations, 95.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(cost: f64, tokens: u64, dur: u64, ok: bool) -> RunSample {
        RunSample { cost_usd: cost, tokens, duration_ms: dur, verified: ok }
    }

    #[test]
    fn empty_is_all_zero() {
        let m = aggregate(&[]);
        assert_eq!(m.runs, 0);
        assert_eq!(m.verify_rate(), 0.0);
        assert_eq!(m.avg_cost_usd(), 0.0);
        assert_eq!(m.p95_duration_ms, 0);
    }

    #[test]
    fn aggregates_totals_and_rates() {
        let m = aggregate(&[
            s(0.10, 100, 10, true),
            s(0.20, 200, 20, true),
            s(0.30, 300, 30, false),
            s(0.40, 400, 40, true),
            s(0.50, 500, 100, false),
        ]);
        assert_eq!(m.runs, 5);
        assert_eq!(m.verified, 3);
        assert_eq!(m.failed, 2);
        assert!((m.total_cost_usd - 1.5).abs() < 1e-9);
        assert_eq!(m.total_tokens, 1500);
        assert!((m.verify_rate() - 0.6).abs() < 1e-9);
        assert!((m.avg_cost_usd() - 0.3).abs() < 1e-9);
        // durations sorted [10,20,30,40,100]: p50→30, p95→100。
        assert_eq!(m.p50_duration_ms, 30);
        assert_eq!(m.p95_duration_ms, 100);
    }
}
