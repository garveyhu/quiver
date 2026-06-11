//! 观测面 IPC(DESIGN §8/§10-12):XP/等级统计、运行指标、独立审计、经理决策日志。
//! 全是只读聚合(或不改 gate 的独立审计),不碰 run/merge/verify 路径。

use tauri::State;

use quiver_store::MetricSample;

use crate::{current_project, now_ms, AppState};

/// 当前项目最近的经理决策(decision_log,最新在前;§10 复盘)。工作台打开时回填决策流
/// 历史 —— live 事件之前发生过什么,重启也不丢。
#[tauri::command]
pub fn get_decisions(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<quiver_store::DecisionRecord>, String> {
    let project = current_project(&state)?.display().to_string();
    let store = state.store()?;
    store
        .decisions_for_project(&project, limit.unwrap_or(40).clamp(1, 200))
        .map_err(|e| format!("{e:#}"))
}

/// Aggregate progression stats for the XP / level HUD (read-only, all projects).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    total: i64,
    verified: i64,
    failed: i64,
    cost_usd: f64,
    xp: i64,
    level: i64,
    /// Rolling-window spend (all projects), mirroring the §10 budget gate so the
    /// HUD「今夜花费」and the budget banner agree with what actually pauses the queue.
    spent_day: f64,
    spent_month: f64,
    /// Rolling-24h verified/failed counts — the「昨晚」morning report shows these so
    /// its tally reflects the night, not all-time.
    verified_day: i64,
    failed_day: i64,
}

/// Compute XP + level from the task table. Pure read-only aggregate — does not
/// touch the run / merge / verify path. XP = verified·100 + failed·20; level
/// grows on a sqrt curve so early wins level fast, later ones taper.
#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> Result<Stats, String> {
    let store = state.store()?;
    let (total, verified, failed, cost_usd) = store.task_stats().map_err(|e| format!("{e:#}"))?;
    let xp = verified * 100 + failed * 20;
    let level = 1 + ((xp as f64) / 100.0).sqrt().floor() as i64;
    const DAY_MS: i64 = 86_400_000;
    let now = now_ms();
    let spent_day = store.cost_since(now - DAY_MS).map_err(|e| format!("{e:#}"))?;
    let spent_month = store.cost_since(now - 30 * DAY_MS).map_err(|e| format!("{e:#}"))?;
    let (_, verified_day, failed_day, _) = store
        .task_stats_since(now - DAY_MS)
        .map_err(|e| format!("{e:#}"))?;
    Ok(Stats {
        total,
        verified,
        failed,
        cost_usd,
        xp,
        level,
        spent_day,
        spent_month,
        verified_day,
        failed_day,
    })
}

/// 观测指标(DESIGN §10-12)的 wire 形:由 [`quiver_core::metrics::Metrics`] 投影成
/// camelCase,额外带派生的 `verifyRate` / `avgCostUsd`,给验收台 / 自治度面板用。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDto {
    runs: u64,
    verified: u64,
    failed: u64,
    total_cost_usd: f64,
    total_tokens: u64,
    p50_duration_ms: u64,
    p95_duration_ms: u64,
    /// 验收率(自治度核心指标)。
    verify_rate: f64,
    avg_cost_usd: f64,
}

/// 聚合所有项目里**已结束**任务(verified/done/failed)的运行指标(§10-12)。
/// 每个任务一条样本:花费取 `cost_usd`,时长用 `updated_at - created_at` 墙钟代理
/// (per-task tokens 暂未单独入库,记 0),验收 = 状态属 verified/done。纯读聚合,不碰调度。
#[tauri::command]
pub fn get_metrics(state: State<'_, AppState>) -> Result<MetricsDto, String> {
    let store = state.store()?;
    let samples = store.metric_samples().map_err(|e| format!("{e:#}"))?;
    Ok(metrics_from_samples(&samples))
}

/// 把任务样本映射成运行样本并聚合(§10-12)。抽成纯函数便于单测;tauri 命令只读 store 后调它。
/// 只算已结束任务;tokens/时长优先用 agent 报的真值,缺则退回 0 / 墙钟代理(updated-created)。
fn metrics_from_samples(samples: &[MetricSample]) -> MetricsDto {
    use quiver_core::metrics::{aggregate, RunSample};
    let runs: Vec<RunSample> = samples
        .iter()
        .filter(|s| matches!(s.status.as_str(), "verified" | "done" | "failed"))
        .map(|s| RunSample {
            cost_usd: s.cost_usd.unwrap_or(0.0),
            tokens: s.tokens.unwrap_or(0).max(0) as u64,
            duration_ms: s
                .duration_ms
                .unwrap_or((s.updated_at - s.created_at).max(0))
                .max(0) as u64,
            verified: matches!(s.status.as_str(), "verified" | "done"),
        })
        .collect();
    let m = aggregate(&runs);
    MetricsDto {
        runs: m.runs,
        verified: m.verified,
        failed: m.failed,
        total_cost_usd: m.total_cost_usd,
        total_tokens: m.total_tokens,
        p50_duration_ms: m.p50_duration_ms,
        p95_duration_ms: m.p95_duration_ms,
        verify_rate: m.verify_rate(),
        avg_cost_usd: m.avg_cost_usd(),
    }
}

/// 一次独立审计的结果(DESIGN §8)。`audited=false` 表示没东西可审(无分支 / 没配 verify)。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditResult {
    audited: bool,
    passed: Option<bool>,
    reason: String,
}

/// §8 独立审计:把某任务的成果分支**完整克隆到干净副本**、重跑配置的 verify 命令,看是否
/// 仍通过 —— 在 worker 碰不到的副本里重跑,挫败它在自己 worktree 里改弱测试骗验收。
/// 用户可触发;无独立分支(simulate / 已合并)或没配 verify 命令则短路不审。不改 gate 决策。
#[tauri::command]
pub fn audit_task(state: State<'_, AppState>, task_id: String) -> Result<AuditResult, String> {
    use quiver_core::audit::clean_clone_verify;
    use quiver_core::verify::VerifyCommand;
    let store = state.store()?;
    let task = store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在".to_string())?;
    let Some(branch) = task.branch else {
        return Ok(AuditResult {
            audited: false,
            passed: None,
            reason: "任务无独立分支(simulate / 已合并),无可审计".to_string(),
        });
    };
    let settings = store.get_settings().map_err(|e| format!("{e:#}"))?;
    if settings.verify_command.trim().is_empty() {
        return Ok(AuditResult {
            audited: false,
            passed: None,
            reason: "未配置 verify 命令,无法独立审计".to_string(),
        });
    }
    let project = current_project(&state)?;
    // 唯一临时目录(git clone 要求目标不存在);用纳秒时间戳防撞。
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dest = std::env::temp_dir().join(format!("quiver-audit-{task_id}-{nonce}"));
    let verify = VerifyCommand::shell(settings.verify_command);
    let result = clean_clone_verify(&project, &branch, &verify, &dest);
    let _ = std::fs::remove_dir_all(&dest); // 审完清理临时副本
    let passed = result.map_err(|e| format!("独立审计执行失败:{e:#}"))?;
    Ok(AuditResult {
        audited: true,
        passed: Some(passed),
        reason: if passed {
            "干净克隆重跑 verify 通过".to_string()
        } else {
            "干净克隆重跑 verify 未通过(疑似 worktree 内作弊 / 真红)".to_string()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        status: &str,
        cost: Option<f64>,
        tokens: Option<i64>,
        duration: Option<i64>,
        created: i64,
        updated: i64,
    ) -> MetricSample {
        MetricSample {
            cost_usd: cost,
            tokens,
            duration_ms: duration,
            created_at: created,
            updated_at: updated,
            status: status.into(),
        }
    }

    #[test]
    fn metrics_counts_only_terminal_samples() {
        let samples = vec![
            sample("verified", Some(0.10), Some(100), Some(500), 0, 1000),
            sample("done", Some(0.20), Some(200), None, 0, 3000), // 无真时长 → 墙钟代理 3000
            sample("failed", Some(0.30), Some(300), Some(2000), 0, 2000),
            sample("running", Some(0.99), Some(999), Some(9000), 0, 9000), // 在跑,不计
            sample("queued", None, None, None, 0, 0),                      // 排队,不计
        ];
        let m = metrics_from_samples(&samples);
        assert_eq!(m.runs, 3, "只算 verified/done/failed");
        assert_eq!(m.verified, 2);
        assert_eq!(m.failed, 1);
        assert!((m.total_cost_usd - 0.60).abs() < 1e-9);
        assert_eq!(m.total_tokens, 600, "tokens 用 agent 报的真值之和");
        assert!((m.verify_rate - 2.0 / 3.0).abs() < 1e-9);
        // 时长 [500, 2000, 3000(代理)] 排序后 p50 = 2000。
        assert_eq!(m.p50_duration_ms, 2000);
    }

    #[test]
    fn empty_samples_zero_metrics() {
        let m = metrics_from_samples(&[]);
        assert_eq!(m.runs, 0);
        assert_eq!(m.verify_rate, 0.0);
    }
}
