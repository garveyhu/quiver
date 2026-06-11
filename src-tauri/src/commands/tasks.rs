//! 任务板 IPC(DESIGN §11/§12):列任务、拖拽排序、取消、读事件日志,以及入队 / 打回 / 合并 /
//! 单发运行四条「让活动起来」的路径。入队按 §5 自治开关分流(经理循环 / 旧 scheduler)。

use tauri::{AppHandle, State};

use quiver_store::{NewTask, StoredEvent, TaskRecord};

use crate::run::{run_one_task, RunMode};
use crate::{current_project, now_ms, AppState, EmitTaskUpdate};

/// All persisted tasks for the bulletin-board UI (DESIGN §11, v1.0 module 5),
/// ordered by board position then time. Optional `project` / `status` filters.
#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    project: Option<String>,
    status: Option<String>,
) -> Result<Vec<TaskRecord>, String> {
    let store = state.store()?;
    store
        .list_tasks(project.as_deref(), status.as_deref())
        .map_err(|e| format!("{e:#}"))
}

/// Move a task to a new board `position` (drag-to-reorder).
#[tauri::command]
pub fn reorder_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    position: i64,
) -> Result<(), String> {
    let store = state.store()?;
    store
        .reorder_task(&id, position, now_ms())
        .map_err(|e| format!("{e:#}"))?;
    // Tell the board to re-read so the new order shows immediately.
    let _ = app.emit_task_update();
    Ok(())
}

/// Cancel a running task by killing its agent's whole PROCESS GROUP, not just the
/// agent pid (DESIGN §23 P3 "真急停 killpg"). The runner spawns each agent as its
/// own process-group leader (pgid == pid via `process_group(0)`), so signalling
/// the negative pid reaps the agent AND any children it spawned — never leaving an
/// orphan. SIGKILL is decisive (cancel must stop work now); the dead agent's
/// stdout EOF still routes the run through the normal §8.4 cleanup path.
fn kill_pid(pid: u32) {
    #[cfg(unix)]
    {
        // SAFETY: a plain kill(2) syscall. A negative pid targets the process
        // group whose id is the absolute value — here the agent's own group.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("kill")
            .arg("-KILL")
            .arg(pid.to_string())
            .status();
    }
}

/// Cancel a task. QUEUED → remove its commission from the board. RUNNING /
/// VERIFYING → kill its agent process (it stops + cleans up via the normal path).
/// Already-finished tasks can't be cancelled.
#[tauri::command]
pub fn cancel_task_cmd(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let store = state.store()?;
    let task = store
        .get_task(&id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在或已被移除".to_string())?;
    match task.status.as_str() {
        "queued" => {
            store.delete_task(&id).map_err(|e| format!("{e:#}"))?;
        }
        "running" | "verifying" => {
            let pid = state
                .running_pids
                .lock()
                .ok()
                .and_then(|m| m.get(&id).copied());
            match pid {
                Some(pid) => {
                    kill_pid(pid);
                    if let Ok(mut m) = state.running_pids.lock() {
                        m.remove(&id);
                    }
                }
                None => return Err("找不到运行中的进程（可能刚结束）".to_string()),
            }
        }
        _ => return Err("该委托已结束，无法取消".to_string()),
    }
    let _ = app.emit_task_update();
    Ok(())
}

/// The full ordered event log for one task — the §11 source of truth, for Phase
/// D's Logbook replay.
#[tauri::command]
pub fn get_task_events(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<StoredEvent>, String> {
    let store = state.store()?;
    store.events_for_task(&task_id).map_err(|e| format!("{e:#}"))
}

/// Pin a task to the bulletin board (status `queued`) and kick the concurrent
/// scheduler, which will run it (and any other queued tasks) up to `maxWorkers`
/// at a time. Returns the created [`TaskRecord`] so the UI can flash the new
/// commission card immediately. This is the Phase-C "加入公告板" enqueue path —
/// distinct from the single-shot `run_task_cmd`.
#[tauri::command]
pub async fn enqueue_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    mode: RunMode,
) -> Result<TaskRecord, String> {
    let project = current_project(&state)?;
    let project_str = project.display().to_string();
    let store = state.store()?;

    let task_id = format!("task-{}", now_ms());
    store
        .enqueue_task(&NewTask {
            id: task_id.clone(),
            project: project_str.clone(),
            prompt,
            mode: mode.label().to_string(),
            status: "queued".to_string(),
            created_at: now_ms(),
        })
        .map_err(|e| format!("{e:#}"))?;

    // Read the live settings: max-workers cap + the §5 autonomous switch.
    let settings = store.get_settings().ok();
    let max_workers = settings
        .as_ref()
        .map(|s| s.max_workers as usize)
        .unwrap_or(1)
        .max(1);
    let autonomous = settings.as_ref().map(|s| s.autonomous).unwrap_or(false);
    if autonomous {
        // 自治模式:经理控制循环驱动调度 —— 经理拍决策、决策真的调动 worker(DESIGN §5)。
        // 经理大脑由循环每拍按人事部配置现场选(rule 免费/claude 真想),这里不注入。
        state
            .manager
            .ensure_running(app.clone(), store.clone(), project, max_workers)
            .await;
    } else {
        // 旧模式:scheduler 无脑流水线(有名额+有排队就跑),作为可回退的默认。
        state
            .scheduler
            .ensure_running(app.clone(), store.clone(), project, max_workers)
            .await;
    }

    // Flash the board + return the new card.
    let _ = app.emit_task_update();
    store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务刚入队却读不到，请重试".to_string())
}

/// §12 打回重做(组织回流):把一个已终结(失败/被经理拦下)的任务作为**新任务**重新入队。
/// 新 task_id —— 事件日志主键是 (task_id,seq),复用老行重跑会撞 seq;老行保留作档案。
/// **带上下文返工(§5.3/A5)**:老任务的会话句柄复制给新任务 → worker 经 `--resume` 在
/// 原会话里续跑(记得自己改过什么、为什么没过),配上返工指引,不是裸重跑;经理简报里
/// 还带着「经理·拦下(原因)」的 episode,记忆双保险。
#[tauri::command]
pub async fn requeue_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    answer: Option<String>,
) -> Result<TaskRecord, String> {
    let store = state.store()?;
    let old = store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在".to_string())?;
    if matches!(old.status.as_str(), "queued" | "running" | "verifying") {
        return Err("任务还在进行中,不能打回重做".to_string());
    }
    let old_session = store.task_session_id(&old.id).ok().flatten();
    // §5 CEO→经理→worker 双向闭环:经理升级(escalate)是因为缺 CEO 才能给的决策/信息。CEO 打回
    // 时若补充了答案 → 注入重做的活,让 worker 这次带着 CEO 的信息干,而非缺着同样的信息再卡一次。
    let ans = answer.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let prompt = match (old_session.is_some(), ans) {
        // 有补充:不管有没有老会话,都把 CEO 的答案摆在最前(worker 据此修复)。
        (_, Some(a)) => format!("(返工)CEO 补充了信息:{a}。请据此修复后重新交付。原任务:{}", old.prompt),
        // 续会话返工:worker 记得上次的上下文,指引点明这次要干嘛。
        (true, None) => format!("(返工)上次运行的成果未通过验收,请修复问题后重新交付。原任务:{}", old.prompt),
        (false, None) => old.prompt,
    };
    let new = enqueue_task_cmd(app, state, prompt, RunMode::from_label(&old.mode)).await?;
    if let Some(sid) = old_session {
        // 把老会话句柄挂到新任务行 → run_one_task 读到即走 runner.resume(已有链路)。
        let _ = store.set_task_session_id(&new.id, &sid, now_ms());
    }
    Ok(new)
}

/// §12 验收台·CEO 接受合并:把一个**已通过验收(verified)且有分支**的任务合进 main。走和
/// 经理交付同一条合并列车(merge_and_reverify 自带护栏:冲突 / 合并后重验红 → main 还原、留
/// 分支待人工)。simulate 任务无分支 → 拒绝。这是 real 流程里"CEO 看完产物点头合并"的入口。
#[tauri::command]
pub async fn merge_task_cmd(state: State<'_, AppState>, task_id: String) -> Result<String, String> {
    use quiver_core::git::GitGuard;
    use quiver_core::merge::{merge_and_reverify, MergeDecision, MergeLock};
    use quiver_core::verify::VerifyCommand;

    let store = state.store()?;
    let task = store
        .get_task(&task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "任务不存在".to_string())?;
    if task.status != "verified" {
        return Err(format!("只能合并已通过验收的任务(当前状态:{})", task.status));
    }
    let branch = task
        .branch
        .filter(|b| !b.trim().is_empty())
        .ok_or_else(|| "该任务没有待合并的分支(simulate 任务无分支,real 任务才有产物分支)".to_string())?;

    let project = current_project(&state)?;
    let verify_cmd = store
        .get_settings()
        .ok()
        .map(|s| s.verify_command)
        .filter(|c| !c.trim().is_empty())
        .map(VerifyCommand::shell)
        .unwrap_or_else(|| VerifyCommand::shell("exit 0"));
    let msg = format!("quiver: merge {branch} (CEO 接受)");
    // 独立 guard/merge_lock:手动接受多在非自治时(经理没在跑);merge_and_reverify 的护栏
    // 保证即便撞车也不会污染 main。自治+手动并发共享 guard 留后续硬化。
    let guard = GitGuard::new(project);
    let merge_lock = MergeLock::new();
    let now = now_ms();
    match merge_and_reverify(&merge_lock, &guard, &branch, "main", &msg, &verify_cmd).await {
        Ok(report) if report.decision == MergeDecision::Merged => {
            let _ = store.update_task_status(&task_id, "merged", now);
            Ok("已合进 main".to_string())
        }
        Ok(_) => {
            let _ = store.update_task_status(&task_id, "needs_rebase", now);
            Err("有冲突或合并后重验未过,main 已护栏还原,任务留分支待人工".to_string())
        }
        Err(e) => Err(format!("合并出错:{e:#}")),
    }
}

/// Run ONE task immediately against the picked repo and stream its events to the
/// UI (legacy single-shot path, kept for the original quick-run control). The
/// task row is enqueued `running` up front, shares a fresh per-call git guard,
/// and its lifecycle is persisted by [`run_one_task`].
#[tauri::command]
pub async fn run_task_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    mode: RunMode,
) -> Result<(), String> {
    let project = current_project(&state)?;
    let project_str = project.display().to_string();
    let store = state.store()?;

    let task_id = format!("task-{}", now_ms());
    let _ = store.enqueue_task(&NewTask {
        id: task_id.clone(),
        project: project_str,
        prompt: prompt.clone(),
        mode: mode.label().to_string(),
        status: "running".to_string(),
        created_at: now_ms(),
    });
    let _ = app.emit_task_update();

    let guard = quiver_core::git::GitGuard::new(project);
    run_one_task(&app, &store, &guard, task_id, prompt, mode).await;
    let _ = app.emit_task_update();
    Ok(())
}
