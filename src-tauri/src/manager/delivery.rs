//! 决策落地后的副作用(§5.7/§6):交付时把成果分支合进 main(合并列车 + 合并后重验,守
//! 「main 永不坏」),以及把决策/失败沉淀进记忆 —— 决策留痕成 episode(时间线)、自愈救不动
//! 的失败沉淀成「教训」事实(经理下次召回避开)。记忆是加性依赖,写失败绝不影响编排。

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use quiver_core::merge::{merge_and_reverify, MergeDecision, RebaseReason};
use quiver_core::verify::VerifyCommand;
use quiver_memory::NewFact;
use quiver_orchestrator::Decision;
use quiver_store::{NewTask, Store, TaskRecord};

/// 自治「在最新 main 上重做」的重试上限:并行 worker 改同一文件 → 合并冲突,重做一两次基本就能
/// 错开(前面的合并已落地)。限次防：重做又撞并行、无限循环。
const MAX_REBASE_RETRY: u32 = 2;

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
        Ok(report) => {
            // main 已被护栏还原。原任务标 needs_rebase 留痕(绝不自动解冲突)。但在「绝对自治」下,纯
            // 冲突(worker 基于的 main 已被并行 worker 推进)不该死等人工 —— 否则多 worker 并行改同一
            // 文件时,只有一个能合、其余子任务永远卡 needs_rebase、目标永不完整(real 实测:3 子任务
            // 只成 2)。所以自治 + 纯冲突 → **在最新 main 上重做**:重新入队这个任务,worker 基于已落地
            // 的 main 重做(那时不再冲突)。注意是「重做」不是「自动解冲突」。重验红(A+B 一起红)需人看,
            // 不自动重做;非自治也留人工(现状)。
            let _ = store.update_task_status(task_id, "needs_rebase", crate::now_ms());
            let autonomous = store.get_settings().ok().map(|s| s.autonomous).unwrap_or(false);
            let pure_conflict = matches!(
                report.reason,
                Some(RebaseReason::ProbeConflict | RebaseReason::MergeConflict)
            );
            if autonomous && pure_conflict && requeue_for_rebase(store, task_id) {
                pm.wake.notify_one();
            }
        }
        Err(_) => { /* 合并管线本身出错:不改状态,留分支,best-effort */ }
    }
}

/// 自治「在最新 main 上重做」:把纯冲突的任务重新入队一份(挂回同一父目标,worker 基于已推进的 main
/// 重做、不再冲突)。用 id 里的 rebase 代数计数,超 [`MAX_REBASE_RETRY`] 不再重做(防重做又撞并行的
/// 无限循环)。成功入队返回 true。
fn requeue_for_rebase(store: &Arc<Store>, task_id: &str) -> bool {
    let Some(task) = store.get_task(task_id).ok().flatten() else {
        return false;
    };
    // id 形如 task-rebase-{gen}-… → 解析重做代数,否则视为第 0 代。
    let gen = task_id
        .strip_prefix("task-rebase-")
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    if gen >= MAX_REBASE_RETRY {
        return false;
    }
    let now = crate::now_ms();
    let id = format!("task-rebase-{}-{now}-{}", gen + 1, crate::next_id_seq());
    let ok = store
        .enqueue_task(&NewTask {
            id: id.clone(),
            project: task.project.clone(),
            prompt: task.prompt.clone(),
            mode: task.mode.clone(),
            status: "queued".to_string(),
            created_at: now,
        })
        .is_ok();
    if ok {
        if let Some(g) = task.parent_goal.as_deref() {
            let _ = store.set_task_parent_goal(&id, g, now);
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_task(store: &Arc<Store>, id: &str, goal: &str) {
        store
            .enqueue_task(&NewTask {
                id: id.to_string(),
                project: "p".to_string(),
                prompt: "做 X".to_string(),
                mode: "real".to_string(),
                status: "running".to_string(),
                created_at: 1,
            })
            .unwrap();
        store.set_task_parent_goal(id, goal, 1).unwrap();
    }

    #[test]
    fn requeue_for_rebase_reenqueues_under_same_goal_and_caps_generations() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        seed_task(&store, "task-x", "目标 G");
        // 第 0 代纯冲突 → 在最新 main 上重做:重新入队一份(第 1 代),挂回同一父目标、同一 prompt。
        assert!(requeue_for_rebase(&store, "task-x"));
        let redo = store
            .list_tasks(Some("p"), Some("queued"))
            .unwrap()
            .into_iter()
            .find(|t| t.id.starts_with("task-rebase-1-"))
            .expect("第 1 代重做任务应入队");
        assert_eq!(redo.prompt, "做 X", "重做同一任务");
        assert_eq!(redo.parent_goal.as_deref(), Some("目标 G"), "挂回父目标,追溯树不断");
        // 第 1 代又冲突 → 第 2 代(到上限 MAX_REBASE_RETRY=2)。
        assert!(requeue_for_rebase(&store, &redo.id));
        let gen2 = store
            .list_tasks(Some("p"), Some("queued"))
            .unwrap()
            .into_iter()
            .find(|t| t.id.starts_with("task-rebase-2-"))
            .expect("第 2 代重做任务应入队");
        // 第 2 代再冲突 → 超上限,不再重做(防重做又撞并行的无限循环)。
        assert!(!requeue_for_rebase(&store, &gen2.id), "超 MAX_REBASE_RETRY 不再重做");
    }

    #[test]
    fn requeue_for_rebase_missing_task_is_false() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        assert!(!requeue_for_rebase(&store, "task-nonexistent"));
    }
}
