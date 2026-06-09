//! AI 经理编排(DESIGN §5 / §21)。
//!
//! 经理每"拍"看一眼 [`ManagerContext`](在途/排队/预算/记忆简报),产出一个
//! [`Decision`](§21 结构化决策)。决策由可注入的 [`ManagerBrain`] 给出 —— 测试用确定性
//! [`FakeBrain`](免费),真跑用 LLM(§21 `--json-schema`,后续刀)。saga 去重钥匙 /
//! 栅栏令牌对账 / 有界在途状态机 在此之上,也是后续刀。
//!
//! 本 crate 是**纯逻辑**:不碰 tauri,也还不直接依赖 runner/store/memory —— 经理拿到的
//! 是已经压扁的 [`ManagerContext`],产出的 [`Decision`] 由上层(app/core)去执行。

mod engine;
mod flow;
pub use engine::{Effect, Orchestrator, Step};
pub use flow::{Fence, InFlight, SagaLedger};

use serde::{Deserialize, Serialize};

/// 经理一拍的决策(§21)。`action` 标签 + snake_case,既给 LLM 当 json-schema 输出形,
/// 也直接当 IPC/wire 形(camelCase 字段在序列化层另配,这里先 snake)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Decision {
    /// 派新活:开一个 worker 跑 `prompt`。
    Spawn {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// 续跑某个在途 worker(追加指令)。
    Continue { node_id: String, prompt: String },
    /// 交付:某 worker 成果验收通过,推进交付(合并/发货)。
    Deliver { node_id: String },
    /// 拦下:某 worker 成果有问题,挡住别交付。
    Block { node_id: String, reason: String },
    /// 升级给人:超出自治能力,等人拍板。
    Escalate { reason: String },
    /// 刷新记忆:重读简报/召回再决策。
    RefreshMemory,
    /// 这一拍什么都不做(在途够多 / 预算紧)。
    Noop,
}

/// 经理这一拍看到的、已压扁的局面(§5)。随阶段增长(在途明细、最近 episode、风险信号…)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagerContext {
    /// 在途 worker 数(§5.6 有界在途的当前值)。
    pub inflight: usize,
    /// 排队待派的任务数。
    pub queued: usize,
    /// 同时在途上限(§5.6);经理不该超过它。
    pub max_inflight: usize,
    /// 预算剩余(美元,§10);经理在这之内决策。
    pub budget_remaining_usd: f64,
    /// 记忆简报文本(§6),给经理理解项目现状。
    pub brief: String,
}

/// 经理大脑:看局面产出一个决策。真实现调 LLM(§21);测试用 [`FakeBrain`]。
pub trait ManagerBrain {
    fn decide(&self, ctx: &ManagerContext) -> anyhow::Result<Decision>;
}

/// 确定性大脑(测试 / simulate):永远返回构造时给定的决策。
pub struct FakeBrain(pub Decision);

impl ManagerBrain for FakeBrain {
    fn decide(&self, _ctx: &ManagerContext) -> anyhow::Result<Decision> {
        Ok(self.0.clone())
    }
}

/// 极简 Rust 策略大脑(P0 风格,无 AI):有预算 + 在途未满 + 有排队 → 派活;否则不动。
/// 在 AI 经理上线前兜底,也是 [`ManagerBrain`] 注入缝的最小可用实现。
pub struct RuleBrain;

impl ManagerBrain for RuleBrain {
    fn decide(&self, ctx: &ManagerContext) -> anyhow::Result<Decision> {
        let has_budget = ctx.budget_remaining_usd > 0.0;
        let has_capacity = ctx.inflight < ctx.max_inflight;
        if has_budget && has_capacity && ctx.queued > 0 {
            Ok(Decision::Spawn {
                prompt: "(从队列取下一个任务)".to_string(),
                reason: Some("有预算、在途未满、有排队".to_string()),
            })
        } else {
            Ok(Decision::Noop)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_serializes_to_tagged_snake_case() {
        // §21 契约:action 标签 + snake_case。
        let d = Decision::Spawn { prompt: "读 README".into(), reason: None };
        let j = serde_json::to_value(&d).unwrap();
        assert_eq!(j["action"], "spawn");
        assert_eq!(j["prompt"], "读 README");
        assert!(j.get("reason").is_none(), "None reason 不序列化");

        let block = Decision::Block { node_id: "n1".into(), reason: "测试没过".into() };
        assert_eq!(serde_json::to_value(&block).unwrap()["action"], "block");
    }

    #[test]
    fn decision_round_trips() {
        for d in [
            Decision::Continue { node_id: "n1".into(), prompt: "继续".into() },
            Decision::Deliver { node_id: "n2".into() },
            Decision::Escalate { reason: "拿不准".into() },
            Decision::RefreshMemory,
            Decision::Noop,
        ] {
            let j = serde_json::to_string(&d).unwrap();
            let back: Decision = serde_json::from_str(&j).unwrap();
            assert_eq!(d, back, "Decision 应能 JSON 往返");
        }
    }

    #[test]
    fn fake_brain_returns_fixed_decision() {
        let brain = FakeBrain(Decision::Noop);
        assert_eq!(brain.decide(&ManagerContext::default()).unwrap(), Decision::Noop);
    }

    #[test]
    fn rule_brain_spawns_only_with_budget_capacity_and_queue() {
        let brain = RuleBrain;
        // 有预算 + 在途未满 + 有排队 → 派活
        let ctx = ManagerContext {
            inflight: 1,
            queued: 3,
            max_inflight: 4,
            budget_remaining_usd: 5.0,
            brief: String::new(),
        };
        assert!(matches!(brain.decide(&ctx).unwrap(), Decision::Spawn { .. }));
        // 在途已满 → 不动
        let full = ManagerContext { inflight: 4, ..ctx.clone() };
        assert_eq!(brain.decide(&full).unwrap(), Decision::Noop);
        // 没排队 → 不动
        let empty = ManagerContext { queued: 0, ..ctx.clone() };
        assert_eq!(brain.decide(&empty).unwrap(), Decision::Noop);
        // 没预算 → 不动
        let broke = ManagerContext { budget_remaining_usd: 0.0, ..ctx };
        assert_eq!(brain.decide(&broke).unwrap(), Decision::Noop);
    }
}
