//! 经理决策 → 前端工作台的投影(§5/§10):把一拍 [`Step`] 拼成前端形、落库 + emit,让 CEO
//! 在工作台亲眼看到经理这拍拍了什么决策、因此调动了哪个真任务。决策的展示/持久化逻辑单拎
//! 一处,与执行 effect 的核心循环解耦。

use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use quiver_orchestrator::{Decision, Effect, Step};
use quiver_store::Store;

use super::{count_queued, ProjectManager, MANAGER_DECISION_CHANNEL};

/// 一拍决策推给前端工作台的形(camelCase 直喂 React)。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagerDecisionEvent {
    project: String,
    /// 决策序号(§5 去重钥匙,也是工作台时间线的单调序)。
    seq: u64,
    /// 决策动作:spawn/continue/deliver/block/escalate/refresh_memory/noop。
    action: String,
    /// 经理给的理由(有则)。
    reason: Option<String>,
    /// 这拍作用/调动的抽象节点(有则)。
    node_id: Option<String>,
    /// 抽象节点翻译成的真任务(有则)——让人看到"经理因此调动了哪个活"。
    task_id: Option<String>,
    /// effect 是否真落地(非 Nothing:被满载/未知节点等校验拒掉则 false)。
    executed: bool,
    /// spawn 时派的**具体任务文本** —— 让"派活"不再抽象,看得到派的是什么活。
    task_prompt: Option<String>,
    /// 这拍后的实时局面 + 决策依据(预算/并发上限),让人看出经理基于什么判断。
    inflight: usize,
    queued: usize,
    max_inflight: usize,
    budget_remaining_usd: f64,
    ts_ms: i64,
}

/// 把一拍 [`Step`] 拼成 [`ManagerDecisionEvent`] 并 emit 到 [`MANAGER_DECISION_CHANNEL`]。
#[allow(clippy::too_many_arguments)]
pub(super) async fn emit_decision(
    app: &AppHandle,
    store: &Arc<Store>,
    project_key: &str,
    step: &Step,
    node_id: Option<String>,
    task_id: Option<String>,
    task_prompt: Option<String>,
    budget_remaining_usd: f64,
    pm: &Arc<ProjectManager>,
) {
    let (action, reason) = describe(&step.decision);
    let ev = ManagerDecisionEvent {
        project: project_key.to_string(),
        seq: step.seq,
        action,
        reason,
        node_id,
        task_id,
        executed: !matches!(step.effect, Effect::Nothing),
        task_prompt,
        inflight: pm.orch.lock().await.inflight_len(),
        queued: count_queued(store, project_key),
        max_inflight: pm.max_inflight,
        budget_remaining_usd,
        ts_ms: crate::now_ms(),
    };
    // §10/§20 决策落库(best-effort):决策流持久化,重启不丢、工作台可回看历史。
    let _ = store.record_decision(&quiver_store::DecisionRecord {
        project: ev.project.clone(),
        seq: ev.seq as i64,
        action: ev.action.clone(),
        reason: ev.reason.clone(),
        node_id: ev.node_id.clone(),
        task_id: ev.task_id.clone(),
        task_prompt: ev.task_prompt.clone(),
        executed: ev.executed,
        inflight: ev.inflight as i64,
        queued: ev.queued as i64,
        max_inflight: ev.max_inflight as i64,
        budget_remaining_usd: ev.budget_remaining_usd,
        ts_ms: ev.ts_ms,
    });
    let _ = app.emit(MANAGER_DECISION_CHANNEL, &ev);
}

/// 决策 → (action 标签, 理由)。理由对没有 reason 字段的动作给 `None`。
pub(super) fn describe(decision: &Decision) -> (String, Option<String>) {
    match decision {
        Decision::Spawn { reason, .. } => ("spawn".into(), reason.clone()),
        Decision::Plan { subtasks, reason } => (
            "plan".into(),
            Some(reason.clone().unwrap_or_else(|| format!("拆成 {} 个子任务分工", subtasks.len()))),
        ),
        Decision::Continue { .. } => ("continue".into(), None),
        Decision::Deliver { .. } => ("deliver".into(), None),
        Decision::Block { reason, .. } => ("block".into(), Some(reason.clone())),
        Decision::Escalate { reason } => ("escalate".into(), Some(reason.clone())),
        Decision::RefreshMemory => ("refresh_memory".into(), None),
        Decision::Noop => ("noop".into(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_maps_action_and_reason() {
        // 带 reason 的动作透传理由;没有 reason 字段的给 None。
        assert_eq!(describe(&Decision::Noop), ("noop".to_string(), None));
        let (action, reason) =
            describe(&Decision::Spawn { prompt: "x".into(), reason: Some("有排队".into()) });
        assert_eq!(action, "spawn");
        assert_eq!(reason, Some("有排队".to_string()));
        let (action, reason) =
            describe(&Decision::Block { node_id: "n".into(), reason: "测试没过".into() });
        assert_eq!(action, "block");
        assert_eq!(reason, Some("测试没过".to_string()));
        assert_eq!(describe(&Decision::Escalate { reason: "拿不准".into() }).0, "escalate");
        assert_eq!(describe(&Decision::Deliver { node_id: "n".into() }), ("deliver".to_string(), None));
        assert_eq!(describe(&Decision::RefreshMemory).0, "refresh_memory");
    }
}
