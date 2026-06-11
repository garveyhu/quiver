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
    /// 拆活(§5 协作):把一个复杂目标拆成多个子任务入队,交给多个员工分工并行。子任务进队列,
    /// 后续由经理逐个 spawn(按并发/专长)。这是"多 agent 协同完成一件复杂事"的入口。
    Plan {
        subtasks: Vec<String>,
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
    /// 第几轮(§5 双向协作):首跑 0,经理每 continue 一次 +1。让经理知道已经迭代几轮了,
    /// 该收手还是再给一轮指导 —— 也是防"反复 continue 救不动"的依据。
    #[serde(default)]
    pub round: u32,
    /// worker 主动请示的问题(§5 双向协作 worker→经理):终态 needs_input 时带上,经理看到后
    /// continue 回答它。普通完工 → None。
    #[serde(default)]
    pub question: Option<String>,
    /// worker 这一轮**实际产出的摘要**(改了什么 diff_stat + 验证结果)——让经理复核时看产出、
    /// 而非凭终态标签盲裁:产出对路就 Deliver,方向偏/没覆盖到就 Continue 给指导,跑歪了就 Block。
    #[serde(default)]
    pub summary: Option<String>,
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
    /// 团队成员的专长清单(§14 聪明协作),如 `["员工 2 · 测试", "员工 3 · 前端"]`。经理
    /// 派活时据此在任务描述里点明所需专长 → 系统派给对的人(run.rs 专长匹配)。
    #[serde(default)]
    pub team: Vec<String>,
    /// 自治目标(§5 主动自治):CEO 的高层方向。非空 + 队列空时,经理应**主动** plan 推进它的
    /// 下一批任务(参考记忆别重复已做的),而非 noop 退出 —— 这是「绝对自治」(主动驱动)与
    /// 「绝对听话」(被动等派活)的分界。空 = 被动模式,队列空就停。
    #[serde(default)]
    pub autonomous_goal: String,
    /// 已为 `autonomous_goal` 做过的子任务(标题 + 结局),最近若干 —— 让经理主动 plan 时知道「这个
    /// 目标已经推进到哪了」:别重复已做的、在已有基础上递进、做得差不多了就理性收尾(escalate/noop)而
    /// 非无限铺活。空 = 还没为这个目标做过事。
    #[serde(default)]
    pub goal_progress: Vec<String>,
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

/// 每个并发名额的保守预算余量(§9 配额感知):预算不足以按这个余量支撑满并发时,经理
/// 收敛并发、把剩余预算留给在途的跑完,而非并发把钱烧光。real 任务一单通常 $0.05–0.5,
/// 取 0.5 偏保守(宁可慢、不超支)。可调;以后做成 settings 的预算策略旋钮(§7)。
pub const BUDGET_PER_SLOT_USD: f64 = 0.5;

/// 极简 Rust 策略大脑(P0 风格,无 AI):**先裁后派** —— 有完工待复核的先出裁决
/// (verified→交付,其余→拦下);然后按 §9 配额感知决定并发:预算紧时收敛(甚至串行),
/// 有预算 + 未到收敛后的上限 + 有排队 → 派活;否则不动。
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
        // §9 配额感知:预算够稳稳支撑几个并发名额(保守余量),就是这一拍的有效上限。
        // 有预算但偏紧 → 收敛(至少留 1,有预算就能跑一个);夹在配置的 max_inflight 内。
        let affordable = (ctx.budget_remaining_usd / BUDGET_PER_SLOT_USD).floor() as usize;
        let cap = ctx.max_inflight.min(affordable.max(1));
        let has_capacity = ctx.inflight < cap;
        if has_budget && has_capacity && ctx.queued > 0 {
            // 收敛了(cap < 配置上限)就在理由里点明,决策流可见"经理因预算偏紧而保守"。
            let reason = if cap < ctx.max_inflight {
                format!("预算偏紧(剩 ${:.2}),收敛并发到 {cap}、把余量留给在途", ctx.budget_remaining_usd)
            } else {
                "有预算、在途未满、有排队".to_string()
            };
            Ok(Decision::Spawn {
                prompt: QUEUE_NEXT_PLACEHOLDER.to_string(),
                reason: Some(reason),
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
                status: "verified".into(), round: 0, question: None, summary: None,
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
                status: "failed".into(), round: 0, question: None, summary: None,
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
            ..ManagerContext::default()
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

    #[tokio::test]
    async fn rule_brain_budget_aware_throttles_concurrency() {
        let brain = RuleBrain;
        // 配置允许 4 并发,但预算只够 ~2 个名额(0.5/名额) → 收敛到 cap=2:
        // inflight=1 < 2 仍派(带"预算偏紧"理由),inflight=2 到顶不派。
        let tight = ManagerContext {
            inflight: 1,
            queued: 5,
            max_inflight: 4,
            budget_remaining_usd: 1.0, // affordable = 2
            ..ManagerContext::default()
        };
        match brain.decide(&tight).await.unwrap() {
            Decision::Spawn { reason, .. } => {
                assert!(reason.unwrap().contains("收敛"), "偏紧时理由点明收敛");
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
        // 已到收敛后的上限(inflight=2 >= cap=2) → 不再扇出,把余量留给在途。
        let at_tight_cap = ManagerContext { inflight: 2, ..tight.clone() };
        assert_eq!(brain.decide(&at_tight_cap).await.unwrap(), Decision::Noop);
        // 极紧(预算 < 一个名额余量)→ 串行:cap=1,inflight=1 即到顶。
        let serial = ManagerContext { inflight: 1, budget_remaining_usd: 0.3, ..tight };
        assert_eq!(brain.decide(&serial).await.unwrap(), Decision::Noop, "极紧时串行");
        // 但极紧 + 空闲(inflight=0)仍跑一个(有预算就不该饿死队列)。
        let serial_idle = ManagerContext {
            inflight: 0,
            queued: 5,
            max_inflight: 4,
            budget_remaining_usd: 0.3,
            ..ManagerContext::default()
        };
        assert!(matches!(brain.decide(&serial_idle).await.unwrap(), Decision::Spawn { .. }));
    }
}
