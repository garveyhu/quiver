//! 决策落地后的副作用(§5.7/§6):交付时把成果分支合进 main(合并列车 + 合并后重验,守
//! 「main 永不坏」),以及把决策/失败沉淀进记忆 —— 决策留痕成 episode(时间线)、自愈救不动
//! 的失败沉淀成「教训」事实(经理下次召回避开)。记忆是加性依赖,写失败绝不影响编排。

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use quiver_core::merge::{merge_and_reverify, MergeDecision};
use quiver_core::verify::VerifyCommand;
use quiver_memory::NewFact;
use quiver_orchestrator::Decision;
use quiver_store::{Store, TaskRecord};

use super::ProjectManager;

/// 把一拍真落地的决策写成记忆 episode(§6 C1)。best-effort:记忆是加性依赖,写失败
/// 绝不影响编排。summary 全中文自含语义(简报/时间线直接可读)。
pub(super) fn record_decision_episode(
    app: &AppHandle,
    project_key: &str,
    decision: &Decision,
    node_id: Option<&str>,
    task_id: Option<&str>,
    task_prompt: Option<&str>,
) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let Some(mem) = state.memory.get() else {
        return;
    };
    let clip = |s: &str| {
        let t: String = s.chars().take(40).collect();
        if t.len() < s.len() { format!("{t}…") } else { t }
    };
    let task = task_prompt.map(|p| format!("「{}」", clip(p))).unwrap_or_default();
    let summary = match decision {
        Decision::Spawn { reason, .. } => format!(
            "经理·派活{task}{}",
            reason.as_deref().map(|r| format!("({r})")).unwrap_or_default()
        ),
        Decision::Plan { subtasks, reason } => format!(
            "经理·拆活成 {} 个子任务分工{}",
            subtasks.len(),
            reason.as_deref().map(|r| format!("({r})")).unwrap_or_default()
        ),
        Decision::Deliver { .. } => format!("经理·交付{task}"),
        Decision::Block { reason, .. } => format!("经理·拦下{task}({reason})"),
        Decision::Escalate { reason } => format!("经理·升级给人({reason})"),
        Decision::Continue { .. } => format!("经理·续跑{task}"),
        Decision::RefreshMemory | Decision::Noop => return, // 无组织动作,不留痕
    };
    let _ = mem.record_episode(&quiver_memory::NewEpisode {
        project: project_key.to_string(),
        node_id: node_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        summary: Some(summary),
        created_at: crate::now_ms(),
        ..Default::default()
    });
}

/// §6 记忆驱动策略演化:一个任务连自愈都救不动(attempt 到上限仍失败)→ 把它沉淀成一条
/// **教训**事实写进记忆。经理下一拍组装 brief 时按 importance 优先看到它,决策能避开/调整
/// 同类活 —— 「从失败学、不重犯」,真正的"记忆 → 决策"闭环。机械绑真实失败(不是 AI 编的)→
/// 高可信「已验证·机械」档。best-effort:任何一步失败只是不记,绝不影响编排。
pub(super) fn record_failure_lesson(app: &AppHandle, project: &str, task: &TaskRecord, status: &str) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let Some(mem) = state.memory.get() else {
        return;
    };
    let status_cn = match status {
        s if s.contains("rebase") => "合并冲突/重验未过",
        s if s.contains("fail") => "验证失败",
        other => other,
    };
    // retry 任务的 prompt 带「(自动重试…)原任务:X」前缀,教训里只留原始任务文本免噪音。
    let raw = task.prompt.rsplit("原任务:").next().unwrap_or(&task.prompt);
    let gist: String = raw.chars().take(60).collect();
    let text = format!(
        "教训:任务「{gist}」试了 {} 次仍以「{status_cn}」收场、自愈救不动。下次接类似的活,\
         先想清楚上次为什么没过、换个思路,别同样硬上。",
        task.attempt
    );
    let _ = mem.insert_fact(&NewFact {
        project: project.to_string(),
        scope: None,
        kind: "教训".to_string(),
        text,
        entities: None,
        entity: None,
        importance: Some(8), // 教训重要,brief 按 importance 排序时优先冒头给经理看
        valid_at: None,
        recorded_at: crate::now_ms(),
        provenance: Some("失败自愈耗尽·机械".to_string()),
        trust: "已验证·机械".to_string(), // 机械绑真实失败,非 AI 杜撰 → 高可信档
        source_commit: None,
        source_episode_id: None,
    });
}

/// §5.7 合并列车:把交付任务的成果分支合进 `main` 并合并后重验,按结果改任务状态。
/// 全局串行(merge_lock);merge_and_reverify 自带护栏(冲突→needs_rebase、重验红→reset
/// main),所以这里只需翻译结果:`merged`(进了 main)/ `needs_rebase`(冲突或重验红,留人工)。
/// best-effort:合并出错只记日志、不改状态(任务仍在分支上,可人工处理),绝不让编排崩。
pub(super) async fn deliver_merge(
    store: &Arc<Store>,
    pm: &Arc<ProjectManager>,
    task_id: &str,
    branch: &str,
) {
    let verify_cmd = store
        .get_settings()
        .ok()
        .map(|s| s.verify_command)
        .filter(|c| !c.trim().is_empty())
        .map(VerifyCommand::shell)
        .unwrap_or_else(|| VerifyCommand::shell("exit 0"));
    let msg = format!("quiver: merge {branch} (经理交付)");
    match merge_and_reverify(&pm.merge_lock, &pm.guard, branch, "main", &msg, &verify_cmd).await {
        Ok(report) if report.decision == MergeDecision::Merged => {
            let _ = store.update_task_status(task_id, "merged", crate::now_ms());
        }
        Ok(_) => {
            // 冲突 / 合并后重验红 —— main 已被护栏还原,任务留分支等人工(绝不自动解决冲突)。
            let _ = store.update_task_status(task_id, "needs_rebase", crate::now_ms());
        }
        Err(_) => { /* 合并管线本身出错:不改状态,留分支,best-effort */ }
    }
}
