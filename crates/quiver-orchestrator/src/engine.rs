//! 一拍编排循环(§5):把 [`ManagerBrain`] 的决策、[`InFlight`] 有界在途/栅栏、
//! [`SagaLedger`] 去重串起来,产出一个待上层执行的 [`Effect`]。
//!
//! 纯逻辑:[`Orchestrator`] 不真 spawn 进程、不碰 store —— 它只维护编排不变量(并发上限、
//! 栅栏、决策序号),把"该干啥"以 [`Effect`] 交给执行层(app/core/runner)去做,完成再
//! 经 [`on_complete`](Orchestrator::on_complete) 回流。

use crate::flow::{Fence, InFlight, SagaLedger};
use crate::{Decision, ManagerBrain, ManagerContext};

/// 校验/分配后交给执行层的动作。比 [`Decision`] 多了 orchestrator 分配的 `node_id`/`fence`,
/// 且去掉了被状态校验拒掉的情形(满载 Spawn、未知节点 Continue)。
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// 开 worker:用分配好的 `node_id` + 栅栏 `fence` 跑 `prompt`。
    Spawn { node_id: String, fence: Fence, prompt: String },
    /// 给在途 worker 追加指令。
    Continue { node_id: String, prompt: String },
    /// 交付某节点成果(合并/发货)。
    Deliver { node_id: String },
    /// 拦下某节点成果。
    Block { node_id: String, reason: String },
    /// 升级给人。
    Escalate { reason: String },
    /// 刷新记忆后再决策。
    Refresh,
    /// 无需执行:Noop,或决策被状态校验拒掉(满载 Spawn / 未知节点 Continue)。
    Nothing,
}

/// 一拍的产物:决策序号 `seq`(去重钥匙)、经理给的 `decision`、校验后待执行的 `effect`。
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub seq: u64,
    pub decision: Decision,
    pub effect: Effect,
}

/// 编排器:持有有界在途 + saga 账本 + node 计数器。一拍 [`tick`](Self::tick) 一个决策。
pub struct Orchestrator {
    inflight: InFlight,
    saga: SagaLedger,
    node_counter: u64,
}

impl Orchestrator {
    pub fn new(max_inflight: usize) -> Self {
        Self {
            inflight: InFlight::new(max_inflight),
            saga: SagaLedger::new(),
            node_counter: 0,
        }
    }

    /// 当前在途 worker 数。
    pub fn inflight_len(&self) -> usize {
        self.inflight.len()
    }

    /// 跑一拍:经理看 `ctx` 出决策,按编排状态校验/分配,产出 [`Step`]。每拍消耗一个决策序号;
    /// 真正落地的决策(产出非 Nothing 的 effect)记进 saga 账本(幂等)。
    pub fn tick(
        &mut self,
        brain: &dyn ManagerBrain,
        ctx: &ManagerContext,
    ) -> anyhow::Result<Step> {
        let seq = self.saga.next_seq();
        let decision = brain.decide(ctx)?;
        let effect = match &decision {
            Decision::Spawn { prompt, .. } => {
                let node_id = format!("node-{}", self.node_counter);
                match self.inflight.admit(node_id.clone()) {
                    Some(fence) => {
                        self.node_counter += 1;
                        Effect::Spawn { node_id, fence, prompt: prompt.clone() }
                    }
                    None => Effect::Nothing, // 满载 → 拒掉(经理不该派,兜底)
                }
            }
            Decision::Continue { node_id, prompt } => {
                if self.inflight.fence_of(node_id).is_some() {
                    Effect::Continue { node_id: node_id.clone(), prompt: prompt.clone() }
                } else {
                    Effect::Nothing // 未知/已下线节点 → 拒掉
                }
            }
            Decision::Deliver { node_id } => Effect::Deliver { node_id: node_id.clone() },
            Decision::Block { node_id, reason } => {
                Effect::Block { node_id: node_id.clone(), reason: reason.clone() }
            }
            Decision::Escalate { reason } => Effect::Escalate { reason: reason.clone() },
            Decision::RefreshMemory => Effect::Refresh,
            Decision::Noop => Effect::Nothing,
        };
        if effect != Effect::Nothing {
            self.saga.mark_applied(seq);
        }
        Ok(Step { seq, decision, effect })
    }

    /// Worker 完成回流:栅栏匹配则释放在途名额(§5.5 对账)。返回是否被接受
    /// (过期/重复/未知节点返回 false)。
    pub fn on_complete(&mut self, node_id: &str, fence: Fence) -> bool {
        self.inflight.complete(node_id, fence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeBrain;

    fn ctx() -> ManagerContext {
        ManagerContext::default()
    }

    #[test]
    fn tick_spawn_admits_and_assigns_node_and_fence() {
        let mut o = Orchestrator::new(2);
        let brain = FakeBrain(Decision::Spawn { prompt: "读 README".into(), reason: None });
        let step = o.tick(&brain, &ctx()).unwrap();
        assert_eq!(step.seq, 0);
        match step.effect {
            Effect::Spawn { node_id, fence, prompt } => {
                assert_eq!(node_id, "node-0");
                assert_eq!(fence, 1);
                assert_eq!(prompt, "读 README");
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
        assert_eq!(o.inflight_len(), 1);
    }

    #[test]
    fn tick_spawn_rejected_when_full() {
        let mut o = Orchestrator::new(1);
        let brain = FakeBrain(Decision::Spawn { prompt: "x".into(), reason: None });
        assert!(matches!(o.tick(&brain, &ctx()).unwrap().effect, Effect::Spawn { .. }));
        // 满载 → 第二拍 Spawn 被拒为 Nothing。
        assert_eq!(o.tick(&brain, &ctx()).unwrap().effect, Effect::Nothing);
        assert_eq!(o.inflight_len(), 1);
    }

    #[test]
    fn on_complete_frees_slot_for_next_spawn() {
        let mut o = Orchestrator::new(1);
        let brain = FakeBrain(Decision::Spawn { prompt: "x".into(), reason: None });
        let Effect::Spawn { node_id, fence, .. } = o.tick(&brain, &ctx()).unwrap().effect else {
            panic!("expected Spawn");
        };
        assert!(o.on_complete(&node_id, fence), "正确栅栏释放名额");
        assert_eq!(o.inflight_len(), 0);
        // 名额释放后能再 Spawn。
        assert!(matches!(o.tick(&brain, &ctx()).unwrap().effect, Effect::Spawn { .. }));
    }

    #[test]
    fn tick_continue_unknown_node_is_rejected() {
        let mut o = Orchestrator::new(2);
        let brain = FakeBrain(Decision::Continue { node_id: "ghost".into(), prompt: "go".into() });
        assert_eq!(o.tick(&brain, &ctx()).unwrap().effect, Effect::Nothing);
    }

    #[test]
    fn tick_noop_does_nothing_but_consumes_seq() {
        let mut o = Orchestrator::new(2);
        let brain = FakeBrain(Decision::Noop);
        let s0 = o.tick(&brain, &ctx()).unwrap();
        let s1 = o.tick(&brain, &ctx()).unwrap();
        assert_eq!(s0.effect, Effect::Nothing);
        assert_eq!(s0.seq, 0);
        assert_eq!(s1.seq, 1, "每拍消耗一个决策序号");
    }
}
