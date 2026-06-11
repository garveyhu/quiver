//! 经理预览 IPC(DESIGN §5/§21):用免费 RuleBrain 跑一拍,展示「经理此刻会怎么决策」。
//! 只看不动 —— 不 spawn、不花钱、不碰 run/调度路径。真正的经理控制循环在 [`crate::manager`]。

use tauri::State;

use crate::{now_ms, AppState};

/// AI 经理在「此刻真实局面」下会做的决策预览(DESIGN §5/§21):组装真实 ManagerContext
/// (在途/排队/在途上限/预算剩余),用**免费的 RuleBrain** 跑一拍。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerPreviewDto {
    inflight: usize,
    queued: usize,
    max_inflight: usize,
    budget_remaining_usd: f64,
    /// RuleBrain 的决策(§21 tagged action);真接千问大脑是后续刀。
    decision: quiver_orchestrator::Decision,
}

#[tauri::command]
pub async fn manager_preview(state: State<'_, AppState>) -> Result<ManagerPreviewDto, String> {
    use quiver_orchestrator::{ManagerBrain, ManagerContext, RuleBrain};
    let store = state.store()?;
    let running = store
        .list_tasks(None, Some("running"))
        .map_err(|e| format!("{e:#}"))?
        .len();
    let queued = store
        .list_tasks(None, Some("queued"))
        .map_err(|e| format!("{e:#}"))?
        .len();
    let settings = store.get_settings().map_err(|e| format!("{e:#}"))?;
    const DAY_MS: i64 = 86_400_000;
    let spent = store.cost_since(now_ms() - DAY_MS).map_err(|e| format!("{e:#}"))?;
    // 有夜预算 → 剩余=上限-今夜花费(夹 0);没设 → 视为充裕(不因预算挡)。
    let budget_remaining = settings
        .nightly_budget_usd
        .map(|cap| (cap - spent).max(0.0))
        .unwrap_or(1_000_000.0);
    let ctx = ManagerContext {
        inflight: running,
        queued,
        max_inflight: settings.max_workers.max(0) as usize,
        budget_remaining_usd: budget_remaining,
        brief: String::new(),
        // 预览在经理循环之外,看不到(也无需看)循环私有的复核队列/队首细节。
        pending_reviews: vec![],
        next_task: None,
        team: vec![],
        autonomous_goal: String::new(),
    };
    let decision = RuleBrain.decide(&ctx).await.map_err(|e| format!("{e:#}"))?;
    Ok(ManagerPreviewDto {
        inflight: ctx.inflight,
        queued: ctx.queued,
        max_inflight: ctx.max_inflight,
        budget_remaining_usd: ctx.budget_remaining_usd,
        decision,
    })
}
