//! 验收台(DESIGN §11–12):一个 worker 的成果能不能交付,在这里汇总裁决 —— 把 **verify
//! 结果** + **独立审计**([`scan_test_tampering`](crate::audit::scan_test_tampering) 的
//! tamper signals)+ **改动量**(diff_stat)合成一张验收单,给 Pass / Block(带理由)。
//!
//! 纯裁决:不跑 verify、不算 diff(那是 supervisor/git 的事),只对结果下结论。任一硬信号
//! 触发即 Block —— 交付门槛"宁可错杀":验收没过、测试被改弱、或根本没改东西,都不放行。

use crate::audit::TamperScan;

/// 验收裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acceptance {
    /// 通过,可交付。
    Pass,
    /// 拦下,附理由(可多条);交给经理决定重试 / 升级。
    Block { reasons: Vec<String> },
}

impl Acceptance {
    pub fn is_pass(&self) -> bool {
        matches!(self, Acceptance::Pass)
    }
}

/// 汇总裁决:`verify_passed` = verify 闸是否通过;`tamper` = 测试篡改扫描;
/// `files_changed` = diff 改动文件数。任一硬信号即 Block。
pub fn judge_acceptance(verify_passed: bool, tamper: &TamperScan, files_changed: u32) -> Acceptance {
    let mut reasons = Vec::new();
    if !verify_passed {
        reasons.push("verify 未通过".to_string());
    }
    if !tamper.is_clean() {
        reasons.push(format!("独立审计发现测试疑似被改弱:{}", tamper.signals.join("; ")));
    }
    if files_changed == 0 {
        reasons.push("没有改动任何文件(空交付)".to_string());
    }
    if reasons.is_empty() {
        Acceptance::Pass
    } else {
        Acceptance::Block { reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> TamperScan {
        TamperScan::default()
    }

    fn tampered() -> TamperScan {
        TamperScan { signals: vec!["test.rs: 新增跳过标记".to_string()] }
    }

    #[test]
    fn passes_when_verified_clean_and_changed() {
        assert_eq!(judge_acceptance(true, &clean(), 3), Acceptance::Pass);
    }

    #[test]
    fn blocks_on_failed_verify() {
        let a = judge_acceptance(false, &clean(), 3);
        assert!(!a.is_pass());
        match a {
            Acceptance::Block { reasons } => assert!(reasons.iter().any(|r| r.contains("verify"))),
            _ => panic!(),
        }
    }

    #[test]
    fn blocks_on_test_tampering_even_if_verify_passed() {
        let a = judge_acceptance(true, &tampered(), 3);
        match a {
            Acceptance::Block { reasons } => assert!(reasons.iter().any(|r| r.contains("改弱"))),
            _ => panic!("篡改测试必须拦,哪怕 verify 过"),
        }
    }

    #[test]
    fn blocks_empty_delivery_and_collects_all_reasons() {
        let a = judge_acceptance(false, &tampered(), 0);
        match a {
            Acceptance::Block { reasons } => assert_eq!(reasons.len(), 3, "三条理由都收齐"),
            _ => panic!(),
        }
    }
}
