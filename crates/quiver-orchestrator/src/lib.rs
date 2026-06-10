//! AI 经理编排(DESIGN §5 / §21)。
//!
//! 经理每"拍"看一眼 [`ManagerContext`](在途/排队/预算/记忆简报),产出一个
//! [`Decision`](§21 结构化决策)。决策由可注入的 [`ManagerBrain`] 给出 —— 测试/演示用
//! 确定性 [`FakeBrain`]/[`RuleBrain`](免费),真自治用 **claude 引擎大脑**
//! (§0/§4「思考全程用 claude」,`ClaudeBrain`,后续刀)。saga 去重钥匙 / 栅栏令牌对账 /
//! 有界在途状态机 在此之上,也是后续刀。
//!
//! 本 crate 是**纯逻辑**:不碰 tauri,也还不直接依赖 runner/store/memory —— 经理拿到的
//! 是已经压扁的 [`ManagerContext`],产出的 [`Decision`] 由上层(app/core)去执行。

mod engine;
mod flow;
mod gate;
mod ladder;
mod personnel;
mod submanager;
pub use engine::{Effect, Orchestrator, Step};
pub use flow::{Fence, InFlight, SagaLedger};
pub use gate::{GateVerdict, ValueGate};
pub use ladder::{Budget, EscalationLadder, Rung};
pub use personnel::{tier_for, WorkerRecord, WorkerTier};
pub use submanager::ManagerNode;

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

/// 一个等经理复核的完工 worker(§5 复核裁决):谁(node)、跑的哪个活(task)、终态如何。
/// 执行层在 worker 完成时入队;经理看到后出 Deliver/Block 裁决,裁决时出队。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingReview {
    pub node_id: String,
    pub task_id: String,
    /// worker 终态标签:verified / failed / needs_rebase / done …
    pub status: String,
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
    /// 完工待经理复核裁决的 worker(§5):经理应**先裁后派**。
    #[serde(default)]
    pub pending_reviews: Vec<PendingReview>,
    /// 队列下一个任务的原文(§5 A2 拆活):真经理派活时可见,可原样派、也可结合记忆
    /// 改写/细化(spawn.prompt 即给 worker 的最终任务描述)。`None`=队列空。
    #[serde(default)]
    pub next_task: Option<String>,
}

/// [`RuleBrain`] spawn 决策的占位 prompt:表示"原样认领队列下一个任务"(执行层据此用
/// 任务原文跑)。真经理给出**非**占位的 prompt 时,执行层用经理改写后的文本派工(§5 A2)。
pub const QUEUE_NEXT_PLACEHOLDER: &str = "(从队列取下一个任务)";

/// 经理大脑:看局面产出一个决策。真自治用 claude 引擎([`ClaudeBrain`],§4 异步 IO);
/// 测试/演示用 [`FakeBrain`]/[`RuleBrain`]。`decide` 是 async —— 真大脑要等 claude 想完。
#[async_trait::async_trait]
pub trait ManagerBrain: Send + Sync {
    async fn decide(&self, ctx: &ManagerContext) -> anyhow::Result<Decision>;
}

/// 确定性大脑(测试 / simulate):永远返回构造时给定的决策。
pub struct FakeBrain(pub Decision);

#[async_trait::async_trait]
impl ManagerBrain for FakeBrain {
    async fn decide(&self, _ctx: &ManagerContext) -> anyhow::Result<Decision> {
        Ok(self.0.clone())
    }
}

/// 极简 Rust 策略大脑(P0 风格,无 AI):**先裁后派** —— 有完工待复核的先出裁决
/// (verified→交付,其余→拦下);然后 有预算 + 在途未满 + 有排队 → 派活;否则不动。
/// 在 AI 经理上线前兜底,也是 [`ManagerBrain`] 注入缝的最小可用实现。
pub struct RuleBrain;

#[async_trait::async_trait]
impl ManagerBrain for RuleBrain {
    async fn decide(&self, ctx: &ManagerContext) -> anyhow::Result<Decision> {
        // 复核优先(§5 裁决):完工的 worker 等着裁,比派新活急 —— 这是"经理替你回
        // 其他 AI 的决策"的核心动作:看产物终态,拍交付还是拦下。
        if let Some(r) = ctx.pending_reviews.first() {
            return Ok(if r.status == "verified" || r.status == "done" {
                Decision::Deliver { node_id: r.node_id.clone() }
            } else {
                Decision::Block {
                    node_id: r.node_id.clone(),
                    reason: format!("终态 {} 未达交付标准,留产物等人工", r.status),
                }
            });
        }
        let has_budget = ctx.budget_remaining_usd > 0.0;
        let has_capacity = ctx.inflight < ctx.max_inflight;
        if has_budget && has_capacity && ctx.queued > 0 {
            Ok(Decision::Spawn {
                prompt: QUEUE_NEXT_PLACEHOLDER.to_string(),
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

    #[tokio::test]
    async fn fake_brain_returns_fixed_decision() {
        let brain = FakeBrain(Decision::Noop);
        assert_eq!(brain.decide(&ManagerContext::default()).await.unwrap(), Decision::Noop);
    }

    #[tokio::test]
    async fn rule_brain_reviews_before_spawning() {
        let brain = RuleBrain;
        // 有排队也有预算,但有完工待复核 → 先裁不派:verified → 交付。
        let ctx = ManagerContext {
            queued: 3,
            max_inflight: 4,
            budget_remaining_usd: 5.0,
            pending_reviews: vec![PendingReview {
                node_id: "n1".into(),
                task_id: "t1".into(),
                status: "verified".into(),
            }],
            ..ManagerContext::default()
        };
        assert_eq!(
            brain.decide(&ctx).await.unwrap(),
            Decision::Deliver { node_id: "n1".into() }
        );
        // failed → 拦下,带原因。
        let failed = ManagerContext {
            pending_reviews: vec![PendingReview {
                node_id: "n2".into(),
                task_id: "t2".into(),
                status: "failed".into(),
            }],
            ..ctx.clone()
        };
        match brain.decide(&failed).await.unwrap() {
            Decision::Block { node_id, reason } => {
                assert_eq!(node_id, "n2");
                assert!(reason.contains("failed"));
            }
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rule_brain_spawns_only_with_budget_capacity_and_queue() {
        let brain = RuleBrain;
        // 有预算 + 在途未满 + 有排队 → 派活
        let ctx = ManagerContext {
            inflight: 1,
            queued: 3,
            max_inflight: 4,
            budget_remaining_usd: 5.0,
            brief: String::new(),
            pending_reviews: vec![],
            next_task: None,
        };
        assert!(matches!(brain.decide(&ctx).await.unwrap(), Decision::Spawn { .. }));
        // 在途已满 → 不动
        let full = ManagerContext { inflight: 4, ..ctx.clone() };
        assert_eq!(brain.decide(&full).await.unwrap(), Decision::Noop);
        // 没排队 → 不动
        let empty = ManagerContext { queued: 0, ..ctx.clone() };
        assert_eq!(brain.decide(&empty).await.unwrap(), Decision::Noop);
        // 没预算 → 不动
        let broke = ManagerContext { budget_remaining_usd: 0.0, ..ctx };
        assert_eq!(brain.decide(&broke).await.unwrap(), Decision::Noop);
    }
}
