//! 权能分权 · Rule of Two(DESIGN §7–8,参考 Meta 的 Agent Rule of Two)。
//!
//! 一个 agent 会话若**同时**具备这三轴,就有"提示注入 → 拿敏感数据 → 外泄/改状态"的完整
//! 攻击链。Rule of Two:不可信会话**至多占两轴**,第三轴必须砍掉(或加人审)。
//!
//! - 不可信输入(`untrusted_input`):读外部网页 / 第三方仓库 / issue 文本等。
//! - 敏感访问(`sensitive_access`):能碰密钥 / 私有数据 / 生产系统。
//! - 外部副作用(`side_effects`):能写沙箱外 / 出网 / push / 发消息。
//!
//! 纯策略检查:[`Capabilities::check`] 在授予 worker 权能前跑,违反就拒。

/// 一个 worker 会话的三轴权能。默认三轴全无(最收紧)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// 处理不可信输入(外部网页 / 第三方仓库 / issue 文本…)。
    pub untrusted_input: bool,
    /// 接触敏感系统 / 数据(密钥、私有数据、生产)。
    pub sensitive_access: bool,
    /// 能改外部状态 / 外联(写沙箱外、出网、push、发消息)。
    pub side_effects: bool,
}

impl Capabilities {
    /// 占了几轴。
    pub fn count(&self) -> u32 {
        self.untrusted_input as u32 + self.sensitive_access as u32 + self.side_effects as u32
    }

    /// 是否违反 Rule of Two(三轴全占)。
    pub fn violates_rule_of_two(&self) -> bool {
        self.untrusted_input && self.sensitive_access && self.side_effects
    }

    /// 授予前校验:三轴全占则拒(指出该砍哪轴),否则放行。
    pub fn check(&self) -> anyhow::Result<()> {
        if self.violates_rule_of_two() {
            anyhow::bail!(
                "违反 Rule of Two:不可信输入 + 敏感访问 + 外部副作用 三轴不可同时授予一个会话;\
                 砍掉一轴(如禁网/去敏感访问)或拆成多个受限会话 + 加人审。"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_axes_violates() {
        let c = Capabilities {
            untrusted_input: true,
            sensitive_access: true,
            side_effects: true,
        };
        assert_eq!(c.count(), 3);
        assert!(c.violates_rule_of_two());
        assert!(c.check().is_err(), "三轴全占必须被拒");
    }

    #[test]
    fn any_two_axes_is_allowed() {
        let pairs = [
            Capabilities { untrusted_input: true, sensitive_access: true, side_effects: false },
            Capabilities { untrusted_input: true, sensitive_access: false, side_effects: true },
            Capabilities { untrusted_input: false, sensitive_access: true, side_effects: true },
        ];
        for c in pairs {
            assert_eq!(c.count(), 2);
            assert!(!c.violates_rule_of_two());
            assert!(c.check().is_ok(), "两轴以内放行");
        }
    }

    #[test]
    fn default_is_fully_locked_down() {
        let c = Capabilities::default();
        assert_eq!(c.count(), 0);
        assert!(c.check().is_ok());
    }
}
