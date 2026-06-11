//! worker 生命周期(§5.5/§5.6):起 worker 异步跑、完工回流(入复核队 + 栅栏对账释放名额 +
//! 唤醒经理)、协作汇总(子任务都完成则收尾父目标)、失败自愈(验证失败再试一轮)、孤儿进程
//! 清理。这些都围绕"一个 worker 从起到落"的状态流转,与决策投影/ctx 组装解耦。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use quiver_orchestrator::PendingReview;
use quiver_store::{Store, TaskRecord};

use super::ProjectManager;
use crate::run::{run_one_task, RunMode};
use crate::scheduler::TASK_EVENT_CHANNEL;

/// 失败自愈的尝试上限(§5):attempt 到此就停手、保持 block 等 CEO。2 = 首跑 + 最多自动重试 1 次,
/// 防"红任务无限重试"刷屏 / 烧钱。
pub(super) const MAX_AUTO_RETRY: i64 = 2;

/// 起一个 worker 异步跑 `run_one_task`,完成后回流:释放在途名额(`on_complete` 凭栅栏对账)、
/// 清 node↔task 映射、刷新看板、`notify` 唤醒经理再 tick(§5.6)。
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_worker(
    app: &AppHandle,
    store: &Arc<Store>,
    pm: &Arc<ProjectManager>,
    node_id: String,
    fence: u64,
    task_id: String,
    prompt: String,
    mode: RunMode,
    round: u32,
    project_key: String,
) {
    let app = app.clone();
    let store = store.clone();
    let pm = pm.clone();
    let guard = pm.guard.clone();
    tokio::spawn(async move {
        run_one_task(&app, &store, &guard, task_id.clone(), prompt, mode).await;
        // 完工入待复核队(§5 裁决):带终态进队,**先入队再释放名额** —— 这样经理任何时刻
        // 看到 inflight==0 时复核队列必然已就绪,不会漏裁。读不到终态按 failed 保守裁。
        let status = store
            .get_task(&task_id)
            .ok()
            .flatten()
            .map(|t| t.status)
            .unwrap_or_else(|| "failed".to_string());
        // §5 协作汇总:子任务完成后,若它所属协作目标的所有子任务都完成了 → 标父目标「协作完成」。
        maybe_complete_parent_goal(&store, &project_key, &task_id);
        // §5 双向协作(worker→经理):worker 主动请示(needs_input)→ 带上它的问题给经理看。
        let question = if status == "needs_input" {
            store.task_question(&task_id).ok().flatten()
        } else {
            None
        };
        // §5 复核裁决看产出:读这个 task 刚落的 episode,把 worker **实际改了什么**(diff_stat)+
        // 验证结果摘出来给经理 —— 经理据此看产出再裁决(Deliver/Continue/Block),不凭终态标签盲裁。
        let summary = app
            .try_state::<crate::AppState>()
            .and_then(|st| st.memory.get().and_then(|m| m.latest_episode_for_task(&task_id).ok().flatten()))
            .map(|ep| {
                let diff = ep.diff_stat.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("(无文件改动)");
                let verify = ep.verify_result.as_deref().unwrap_or("-");
                let mut s = format!("产出:{diff};验证:{verify}");
                // verify 失败:把失败输出尾巴也给经理 —— 让它诊断是「测试红(改代码)」还是「环境/权限
                // 问题(escalate 给 CEO)」,而非只看 failed 标签瞎猜(real 实测经理误诊为「结构性问题」)。
                if verify.contains("fail") {
                    if let Some(tail) = ep
                        .summary
                        .as_deref()
                        .and_then(|sm| sm.split("[验证失败输出]").nth(1))
                    {
                        let tail: String = tail.trim().chars().take(220).collect();
                        if !tail.is_empty() {
                            s.push_str(&format!(";失败详情:{tail}"));
                        }
                    }
                }
                s
            });
        pm.reviews.lock().await.push(PendingReview {
            node_id: node_id.clone(),
            task_id,
            status,
            round, // §5 双向协作:这是第几轮(首跑 0,经理每 continue 一次 +1)
            question,
            summary,
        });
        // 回流(§5.5 栅栏对账):栅栏匹配才释放名额,挡掉过期/重复。
        pm.orch.lock().await.on_complete(&node_id, fence);
        pm.node_tasks.lock().await.remove(&node_id);
        let _ = app.emit(TASK_EVENT_CHANNEL, &project_key);
        pm.wake.notify_one();
    });
}

/// §5 协作汇总:一个子任务完成后调。若它属于某协作目标(parent_goal),且该目标下**所有**
/// 子任务都到完成态(verified/merged/done) → 把那个停在 `planned` 的父目标标 `done`,表示
/// "公司协作把这个复杂目标搞定了"。非子任务 / 还有兄弟没完成 → 不动。best-effort。
pub(super) fn maybe_complete_parent_goal(store: &Arc<Store>, project: &str, task_id: &str) {
    let Some(goal) = store
        .get_task(task_id)
        .ok()
        .flatten()
        .and_then(|t| t.parent_goal)
    else {
        return; // 不是子任务
    };
    let Ok(all) = store.list_tasks(Some(project), None) else {
        return;
    };
    let siblings: Vec<_> = all
        .iter()
        .filter(|t| t.parent_goal.as_deref() == Some(goal.as_str()))
        .collect();
    // 终态 = 不再进行中(成功或失败都算"尘埃落定");成功 = 验收过/合并/完成。
    let terminal = |s: &str| {
        matches!(
            s,
            "verified" | "merged" | "done" | "failed" | "verify_failed" | "needs_rebase" | "cancelled"
        )
    };
    let ok = |s: &str| matches!(s, "verified" | "merged" | "done");
    if siblings.is_empty() || !siblings.iter().all(|t| terminal(&t.status)) {
        return; // 还有子任务在进行,先不收尾
    }
    // 子任务都尘埃落定 → 父目标终结,免得卡 planned:全成功→done(协作完成);有失败→
    // needs_rebase(协作部分失败,需要你看)。
    let new_status = if siblings.iter().all(|t| ok(&t.status)) {
        "done"
    } else {
        "needs_rebase"
    };
    if let Some(parent) = all.iter().find(|t| t.prompt == goal && t.status == "planned") {
        let _ = store.update_task_status(&parent.id, new_status, crate::now_ms());
    }
}

/// kill 一个上个会话遗留的孤儿 worker 进程。**先验证该 PID 当前确是 claude 进程**(`ps`
/// 命令名含 claude)再杀 —— PID 会被系统复用,崩溃后那个号可能早归了别的无关进程,绝不能盲杀。
/// best-effort:验证不过 / 杀不掉都静默跳过(顶多孤儿多跑一会到自然结束)。
pub(super) fn kill_orphan_worker(pid: i64) {
    if pid <= 0 {
        return;
    }
    let Ok(out) = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    else {
        return;
    };
    let cmd = String::from_utf8_lossy(&out.stdout);
    // 进程已不在(空输出)→ 无需杀;在、且确是 claude → kill。
    if cmd.to_lowercase().contains("claude") {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// §5 失败自愈:把一个验证失败的任务作为**新任务**重新入队(attempt+1、带返工会话续跑),
/// 经理循环下一拍会把它派出去。best-effort:任何一步失败只是不重试,绝不让编排崩。
/// new_id 用 `task-retry-{attempt}-{now}-{seq}` 避免与原任务及彼此撞键(seq 防同毫秒相撞)。
pub(super) async fn auto_retry(
    store: &Arc<Store>,
    pm: &Arc<ProjectManager>,
    project: &str,
    old: &TaskRecord,
) {
    let now = crate::now_ms();
    let next_attempt = old.attempt + 1;
    let new_id = format!("task-retry-{next_attempt}-{now}-{}", crate::next_id_seq());
    let prompt = format!(
        "(自动重试 第 {} 次)上次运行的成果未通过验收,请修复问题后重新交付。原任务:{}",
        next_attempt - 1,
        old.prompt
    );
    if store
        .enqueue_task(&quiver_store::NewTask {
            id: new_id.clone(),
            project: project.to_string(),
            prompt,
            mode: old.mode.clone(),
            status: "queued".to_string(),
            created_at: now,
        })
        .is_err()
    {
        return;
    }
    let _ = store.set_task_attempt(&new_id, next_attempt, now);
    // 续会话返工:worker 记得上次改过什么(real 有意义;simulate 无 session 也无妨)。
    if let Some(sid) = store.task_session_id(&old.id).ok().flatten() {
        let _ = store.set_task_session_id(&new_id, &sid, now);
    }
    // 唤醒经理循环来 spawn 这个新排队任务(循环 idle 等待时靠这个戳醒)。
    pm.wake.notify_one();
}
