//! Running one task end-to-end: resolve the agent binary, stream its events live
//! to the UI, persist every event + the terminal status/cost/branch.
//!
//! This is the shared core both the legacy single-shot path and the Phase-C
//! queue scheduler call. It is deliberately decoupled from Tauri's `State` /
//! command machinery: it takes a plain `&AppHandle`, an `Arc<Store>`, and a
//! shared `Arc<GitGuard>` (so concurrent workers on one project serialize their
//! shared-`.git` metadata ops through the §6 mutex), making it safe to invoke
//! from inside a `tokio::spawn`ed worker.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use quiver_core::event::{AgentEvent, AgentEventPayload};
use quiver_core::git::GitGuard;
use quiver_core::supervisor::{
    run_task_streaming, Cleanup, FinishStatus, RunOptions, RunOutcome, TaskSpec,
};
use quiver_core::verify::VerifyCommand;
use quiver_memory::NewEpisode;
use quiver_store::{NewEvent, NewRun, Store};

/// The Tauri event channel the UI subscribes to (see `useSupervisor.ts`).
const AGENT_EVENT_CHANNEL: &str = "agent-event";

// 按职责拆出的子模块(单一职责):烧钱红线守门、agent 二进制解析、智能派活。re-export 让外部的
// `crate::run::X` 引用一字不改(7 个外部符号经此保持路径不变)。
mod agent_bin;
mod dispatch;
mod env_guard;

pub(crate) use agent_bin::{resolve_agent_bin, resolve_real_claude, which_in_path};
pub(crate) use dispatch::worker_perf;
pub(crate) use env_guard::{assert_subscription_env, SUBSCRIPTION_ENV_KEYS};

/// The run mode chosen in the UI (§4.2). `Simulate` is the free default;
/// `Real` spawns the official `claude` binary on the subscription route.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    Simulate,
    Real,
}

impl RunMode {
    /// The persisted label for a run mode.
    pub fn label(self) -> &'static str {
        match self {
            RunMode::Simulate => "simulate",
            RunMode::Real => "real",
        }
    }

    /// Parse the persisted label back into a mode. Anything that is not exactly
    /// `"real"` is treated as `simulate` (the safe, free default) — so a corrupt
    /// or unknown stored value never silently spends real credit.
    pub fn from_label(label: &str) -> Self {
        match label {
            "real" => RunMode::Real,
            _ => RunMode::Simulate,
        }
    }
}

/// What a finished run yields for the history record (DESIGN §11).
pub struct RunSummary {
    pub status: String,
    pub cost_usd: Option<f64>,
    /// agent result 报的 tokens(§10-12 观测),无则 None。
    pub tokens: Option<i64>,
    /// agent result 报的真实耗时(ms),无则 None(get_metrics 退回墙钟代理)。
    pub duration_ms: Option<i64>,
    pub branch: Option<String>,
    /// Worktree HEAD sha at run end (§6.2) — bound onto the episode.
    pub commit_sha: Option<String>,
    /// `git diff --shortstat <base>` for the attempt (§6.2) — bound onto the episode.
    pub diff_stat: Option<String>,
}

/// A terminal event synthesized AFTER `run_task` returns, carrying the §5.2
/// `FinishStatus`, the run's summed cost, and — for real-mode runs left
/// un-merged — the attempt branch the work lives on. Mirrors the wire envelope
/// so the UI renders it in the same list (matched by `kind: "finished"`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishedEvent {
    task_id: String,
    seq: u64,
    ts_ms: i64,
    runner: &'static str,
    kind: &'static str,
    status: String,
    cost_usd: Option<f64>,
    branch: Option<String>,
    /// The verify-gate output (tail) when a run was `VerifyFailed` — lets the UI
    /// show *why* it failed. `None` for passed/other outcomes.
    verify_output: Option<String>,
}

/// Run one already-enqueued task to completion against a SHARED git guard,
/// persisting its lifecycle exactly like the single-shot path: stream every
/// event live + append to the §11 log, then update the task row's status, cost,
/// and branch, and append a `run_history` summary.
///
/// The task row is assumed to already exist in `running` state (the scheduler's
/// `claim_next_queued` flips it before calling here; the single-shot command
/// enqueues it `running`). On error the row is moved to `failed` and a sanitized
/// message is logged — never propagated as a panic out of the spawned worker.
pub async fn run_one_task(
    app: &AppHandle,
    store: &Store,
    guard: &GitGuard,
    task_id: String,
    prompt: String,
    mode: RunMode,
) {
    let project = guard.repo().display().to_string();
    match run_streaming(app, Some(store), guard, task_id.clone(), prompt.clone(), mode).await {
        Ok(summary) => {
            let _ = store.update_task_status(&task_id, &summary.status, crate::now_ms());
            let _ = store.set_task_cost_branch(
                &task_id,
                summary.cost_usd,
                summary.branch.as_deref(),
                crate::now_ms(),
            );
            // §10-12 观测:落 agent 报的 tokens / 真实耗时,供 get_metrics 精确聚合。
            let _ = store.set_task_metrics(
                &task_id,
                summary.tokens,
                summary.duration_ms,
                crate::now_ms(),
            );
            // Record a §6.2 episode (before `project`/`prompt`/`status` move into the
            // run-history row below). Mechanical: what happened + its terminal status.
            record_episode(
                app,
                &project,
                &task_id,
                &summary.status,
                &prompt,
                summary.commit_sha.as_deref(),
                summary.diff_stat.as_deref(),
            );
            let _ = store.record_run(&NewRun {
                project,
                prompt,
                mode: mode.label().to_string(),
                status: summary.status,
                cost_usd: summary.cost_usd,
                branch: summary.branch,
                created_at: crate::now_ms(),
            });
        }
        Err(_e) => {
            // A run-level failure (e.g. agent binary not resolvable) is a task
            // outcome, not a crash: mark the card failed so the board reflects it
            // and the freed slot moves on. The sanitized detail is not surfaced
            // here (no live error event from a finished worker) — the card status
            // carries it.
            let _ = store.update_task_status(&task_id, "failed", crate::now_ms());
            record_episode(app, &project, &task_id, "failed", &prompt, None, None);
        }
    }
}

/// Record a §6.2 episode for a finished run into durable agent memory. Best-effort:
/// memory is additive to the run loop, so a missing store or a write error never
/// affects the task outcome. `commit_sha` + `diff_stat` bind the episode to its
/// commit and change size (§6.2 机械绑 git).
fn record_episode(
    app: &AppHandle,
    project: &str,
    task_id: &str,
    verify_result: &str,
    summary: &str,
    commit_sha: Option<&str>,
    diff_stat: Option<&str>,
) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let Some(mem) = state.memory.get() else {
        return;
    };
    let _ = mem.record_episode(&NewEpisode {
        project: project.to_string(),
        task_id: Some(task_id.to_string()),
        commit_sha: commit_sha.map(str::to_string),
        diff_stat: diff_stat.map(str::to_string),
        verify_result: Some(verify_result.to_string()),
        summary: Some(summary.to_string()),
        created_at: crate::now_ms(),
        ..Default::default()
    });
}

/// The streaming run core: resolve the binary + options for the mode, run the
/// task with a per-event callback that (1) tracks cost, (2) appends to the §11
/// log, (3) emits live to the UI, then synthesize + persist + emit the terminal
/// `finished` cap. Returns the [`RunSummary`] for the caller to persist on the
/// task row + history.
pub async fn run_streaming(
    app: &AppHandle,
    store: Option<&Store>,
    guard: &GitGuard,
    task_id: String,
    prompt: String,
    mode: RunMode,
) -> anyhow::Result<RunSummary> {
    let settings = store
        .map(|s| s.get_settings())
        .transpose()?
        .unwrap_or_default();

    // The verify-gate command (DESIGN §7), shared by both modes: run the user's
    // configured command in the worktree before the merge decision. Empty (the
    // default) → always-pass (`exit 0`), preserving the original behavior until a
    // gate command is set. Simulate honors it too, so the gate can be rehearsed
    // for free (and a red verify shows as VerifyFailed without spending credits).
    let verify = if settings.verify_command.trim().is_empty() {
        VerifyCommand::shell("exit 0")
    } else {
        VerifyCommand::shell(settings.verify_command.clone())
    };

    // §14 按人追溯:对**所有模式**都定下派给哪个员工(同 task_id 稳定分配),记到任务行 ——
    // 追溯室/员工工作台据此显示"由员工X干"。real 分支复用它的模型/预算/轮数配置。
    let worker_role = store.and_then(|s| dispatch::pick_worker_role(s, &task_id, &prompt));
    if let (Some(s), Some(role)) = (store, worker_role.as_ref()) {
        let _ = s.set_task_worker_role(&task_id, &role.name, crate::now_ms());
    }

    let (agent_bin, mut options) = match mode {
        RunMode::Simulate => {
            // Make `fakeDelayMs` take effect: the runner forwards
            // QUIVER_FAKE_DELAY_MS to the `fake-claude` child via its env
            // allowlist. SAFETY: set on the parent process env just before the
            // spawn; the value is inert for any non-fake binary, and concurrent
            // simulate workers all want the SAME saved pacing, so a benign race
            // on this write only ever (re)writes the same value.
            std::env::set_var("QUIVER_FAKE_DELAY_MS", settings.fake_delay_ms.to_string());
            (resolve_agent_bin(&settings, mode)?, RunOptions::default())
        }
        RunMode::Real => {
            let bin = resolve_agent_bin(&settings, mode)?;
            assert_subscription_env()?;
            // 人事部员工角色配置接到真实 run(§14,配置不是摆设):用上面已定的那个员工
            // (worker_role,同 task_id 稳定分配)的模型/预算/轮数。回退:无员工角色 → 全局 settings。
            let model = worker_role
                .as_ref()
                .map(|r| r.model.clone())
                .unwrap_or_else(|| settings.model.clone());
            let mut extra_args = vec![
                "--permission-mode".to_string(),
                "acceptEdits".to_string(),
                "--model".to_string(),
                model,
            ];
            if let Some(turns) = worker_role.as_ref().and_then(|r| r.max_turns) {
                extra_args.push("--max-turns".to_string());
                extra_args.push(turns.to_string());
            }
            if let Some(budget) = worker_role.as_ref().and_then(|r| r.budget_usd) {
                extra_args.push("--max-budget-usd".to_string());
                extra_args.push(format!("{budget}"));
            }
            (
                bin,
                RunOptions {
                    // SAFETY: no sandbox yet (Phase 6). Never auto-merge a real
                    // agent's work into the user's `main` — leave it on a branch.
                    keep_branch: true,
                    extra_args,
                    ..Default::default()
                },
            )
        }
    };

    // §14 丰富配置生效:把这个员工的身份(专长)+ CEO 给 ta 的工作准则(system_prompt)注入任务
    // 开头 —— 员工带着自己的专长和性格干活,而不是千篇一律。配了不生效就是假配置。
    let role_intro = worker_role
        .as_ref()
        .map(|r| {
            let mut lines = Vec::new();
            let sp = r.specialty.trim();
            if !sp.is_empty() && sp != "通用" {
                lines.push(format!("你是「{}」,专长是「{sp}」,按你的专长把这件事做到位。", r.name));
            }
            let cfg = r.system_prompt.trim();
            if !cfg.is_empty() {
                lines.push(format!("CEO 给你的工作准则(优先遵守):{cfg}"));
            }
            if lines.is_empty() { String::new() } else { format!("{}\n\n", lines.join("\n")) }
        })
        .unwrap_or_default();
    // §6 记忆驱动 worker:把项目沉淀的记忆(约定/教训/有效做法)也喂给干活的人,不只喂经理 ——
    // worker 按项目风格干、别重蹈覆辙。只取当前事实(6 条,按 importance 排:约定/教训冒头),不带
    // episode 流水。记忆是加性依赖,读不到就空、绝不挡干活。
    let project_key = guard.repo().display().to_string();
    let mem_brief = {
        let facts = app
            .try_state::<crate::AppState>()
            .and_then(|st| st.memory.get().and_then(|m| m.brief(&project_key, 8, 0).ok()))
            .map(|b| b.facts)
            .unwrap_or_default();
        // §6 按种类分组注入:约定(必守)/教训(避坑)/有效做法(照做)/现状,让 worker 一眼看清每条记忆
        // 的性质 —— 配合记忆官的分类提炼,记忆对干活更有指导性,而非一锅平铺让 worker 自己猜性质。
        let group = |kind: &str| -> String {
            facts
                .iter()
                .filter(|f| f.kind == kind)
                .map(|f| format!("  - [{}] {}\n", f.trust, f.text))
                .collect()
        };
        let conventions = group("约定");
        let lessons = group("教训");
        let practices = group("有效做法");
        let others: String = facts
            .iter()
            .filter(|f| !matches!(f.kind.as_str(), "约定" | "教训" | "有效做法"))
            .map(|f| format!("  - [{}] {}\n", f.trust, f.text))
            .collect();
        let mut body = String::new();
        if !conventions.is_empty() {
            body.push_str(&format!("【必守约定】\n{conventions}"));
        }
        if !lessons.is_empty() {
            body.push_str(&format!("【踩过的坑,避开】\n{lessons}"));
        }
        if !practices.is_empty() {
            body.push_str(&format!("【验证管用的做法】\n{practices}"));
        }
        if !others.is_empty() {
            body.push_str(&format!("【项目现状 / 其他】\n{others}"));
        }
        if body.trim().is_empty() {
            String::new()
        } else {
            format!(
                "[项目记忆] 这个项目沉淀的经验,干活时遵循、别重蹈覆辙(每条带可信度:「权威 / 已验证」\
                 是硬事实务必照做,「员工汇报」参考即可、与现实冲突时以实际为准):\n{body}\n"
            )
        }
    };
    // §5 双向协作(worker→经理):告诉 worker 卡住别硬猜 —— 遇到该上级拍板的点写 NEEDS_INPUT,
    // 经理会给指示;能自己合理决定的正常做完,不必事事请示。
    let prompt = format!(
        "{role_intro}{mem_brief}{prompt}\n\n[协作约定] 遇到需要上级拍板的点(架构选择、模糊或缺失的需求、重大取舍),\
         或缺关键信息做不下去时,别擅自硬做或瞎猜 —— 在输出末尾单独起一行写 \
         `NEEDS_INPUT: <你的具体问题>`,经理看到会给你指示后你再继续。能自己合理决定的就正常做完。"
    );
    let task = TaskSpec {
        id: task_id,
        prompt,
    };

    // If this task already has a persisted backend session (reconcile requeued an
    // interrupted run), resume it instead of starting fresh — the agent continues
    // its prior context via `--resume` (DESIGN §4/§23 P0). A brand-new task has no
    // session_id yet, so this is None and the run spawns normally.
    if let Some(store) = store {
        if let Ok(Some(sid)) = store.task_session_id(&task.id) {
            options.resume_session = Some(sid);
        }
    }

    let mut last_cost: Option<f64> = None;
    let mut last_tokens: Option<i64> = None;
    let mut last_duration: Option<i64> = None;
    let tid = task.id.clone();
    let result = run_task_streaming(
        guard,
        &task,
        &agent_bin,
        &verify,
        options,
        |event: &AgentEvent| {
            if let AgentEventPayload::Result {
                cost_usd,
                tokens,
                duration_ms,
                ..
            } = &event.payload
            {
                last_cost = *cost_usd;
                last_tokens = tokens.map(|t| t as i64);
                last_duration = duration_ms.map(|d| d as i64);
            }
            // Persist the backend session handle the instant the worker reports it
            // (the §5.3 init line), so a crash/restart can resume this task with
            // --resume instead of re-running from scratch (DESIGN §4/§6/§23 P0).
            if let (
                Some(store),
                AgentEventPayload::WorkerStarted {
                    session_id: Some(sid),
                    ..
                },
            ) = (store, &event.payload)
            {
                let _ = store.set_task_session_id(&tid, sid, crate::now_ms());
            }
            if let Some(store) = store {
                persist_event(store, event);
            }
            let _ = app.emit(AGENT_EVENT_CHANNEL, event);
        },
        // on_started(pid): register the running task's child PID so `cancel_task_cmd`
        // can stop it (kill → stdout EOF → the normal cleanup path runs).
        |pid: u32| {
            if let Some(st) = app.try_state::<crate::AppState>() {
                if let Ok(mut m) = st.running_pids.lock() {
                    m.insert(tid.clone(), pid);
                }
            }
            // 也持久化进 task 表:内存 running_pids 在 app 崩溃时丢失,DB 里的 PID 让重启后的
            // reconcile 能 kill 上个会话遗留的孤儿 worker 进程(§23 崩溃恢复卫生)。
            if let Some(store) = store {
                let _ = store.set_task_pid(&tid, pid as i64, crate::now_ms());
            }
        },
    )
    .await;
    // Task ended (ok or err): drop it from the running registry so a later cancel
    // can't kill an unrelated reused PID.
    if let Some(st) = app.try_state::<crate::AppState>() {
        if let Ok(mut m) = st.running_pids.lock() {
            m.remove(&task.id);
        }
    }
    let outcome: RunOutcome = result?;

    let branch = match outcome.cleanup {
        Cleanup::PreservedBranch => Some(outcome.branch.clone()),
        _ => None,
    };
    let mut status_label = finish_status_label(outcome.status).to_string();
    // §5 双向协作(worker→经理):worker 在产出里写了 `NEEDS_INPUT: <问题>` → 它卡住要请示,
    // 覆盖终态为 needs_input、把问题存进 task,经理看到会 continue 给指示(而非当普通完工裁决)。
    if let Some(q) = extract_needs_input(&outcome.events) {
        status_label = "needs_input".to_string();
        if let Some(store) = store {
            let _ = store.set_task_question(&outcome.task_id, &q, crate::now_ms());
        }
    }
    // Only surface the verify output when the gate is what failed (so the UI can
    // show why); for passed/other outcomes it's noise.
    let verify_output = if matches!(outcome.status, FinishStatus::VerifyFailed) {
        outcome.verify_output.clone()
    } else {
        None
    };
    let finished_task_id = outcome.task_id.clone();
    let finished_seq = outcome.events.len() as u64;
    let finished_ts = crate::now_ms();
    let finished = FinishedEvent {
        task_id: outcome.task_id,
        seq: finished_seq,
        ts_ms: finished_ts,
        runner: "claude_cli",
        kind: "finished",
        status: status_label.clone(),
        cost_usd: last_cost,
        branch: branch.clone(),
        verify_output,
    };
    if let Some(store) = store {
        if let Ok(payload) = serde_json::to_string(&finished) {
            let _ = store.append_event(&NewEvent {
                task_id: &finished_task_id,
                seq: finished_seq,
                ts_ms: finished_ts,
                runner: "claude_cli",
                kind: "finished",
                payload_json: &payload,
            });
        }
    }
    app.emit(AGENT_EVENT_CHANNEL, &finished)
        .map_err(|e| anyhow::anyhow!("emit finished failed: {e}"))?;

    Ok(RunSummary {
        status: status_label,
        cost_usd: last_cost,
        tokens: last_tokens,
        duration_ms: last_duration,
        branch,
        commit_sha: outcome.commit_sha,
        diff_stat: outcome.diff_stat,
    })
}

/// Append one live `AgentEvent` to the §11 source-of-truth log (best-effort).
fn persist_event(store: &Store, event: &AgentEvent) {
    let Ok(value) = serde_json::to_value(event) else {
        return;
    };
    let payload_json = value.to_string();
    let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown");
    let runner = value
        .get("runner")
        .and_then(|v| v.as_str())
        .unwrap_or("claude_cli");
    let _ = store.append_event(&NewEvent {
        task_id: &event.task_id,
        seq: event.seq,
        ts_ms: event.ts_ms,
        runner,
        kind,
        payload_json: &payload_json,
    });
}

fn finish_status_label(status: FinishStatus) -> &'static str {
    match status {
        FinishStatus::Verified => "verified",
        FinishStatus::VerifyFailed => "verify_failed",
        FinishStatus::Failed => "failed",
        FinishStatus::NeedsRebase => "needs_rebase",
    }
}

/// 从 worker 的事件流里找它主动写的 `NEEDS_INPUT: <问题>`(§5 双向协作 worker→经理)。取最后
/// 一处(worker 可能边做边改主意,最后那次请示最准),问题取该标记后的同一行。没有 → None。
fn extract_needs_input(events: &[AgentEvent]) -> Option<String> {
    const TAG: &str = "NEEDS_INPUT:";
    let mut found = None;
    for ev in events {
        if let AgentEventPayload::OutputChunk { text } = &ev.payload {
            // 只认**单独起一行**的 NEEDS_INPUT(worker 真请示) —— 不误匹配注入的协作约定里那句
            // 行内示例「写 `NEEDS_INPUT: <你的具体问题>`」(它行首是别的字),占位 <...> 也排除。
            for line in text.lines() {
                if let Some(q) = line.trim().strip_prefix(TAG) {
                    let q = q.trim();
                    if !q.is_empty() && !q.starts_with('<') {
                        found = Some(q.to_string());
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_core::event::RunnerKind;

    fn chunk(text: &str) -> AgentEvent {
        AgentEvent {
            task_id: "t".into(),
            seq: 0,
            ts_ms: 0,
            runner: RunnerKind::ClaudeCli,
            payload: AgentEventPayload::OutputChunk { text: text.into() },
        }
    }

    #[test]
    fn extract_needs_input_only_real_single_line_asks() {
        // §5 双向协作:worker 单独一行写的 NEEDS_INPUT 才算真请示 → 提取问题。
        let asked = [chunk("做了一半,有个取舍。\nNEEDS_INPUT: 用方案 A 还是 B?")];
        assert_eq!(extract_needs_input(&asked).as_deref(), Some("用方案 A 还是 B?"));
        // 注入的协作约定里那句行内示例(行首是别的字)+ 占位 <...> → **不**误判成请示。
        let convention = [chunk("在末尾单独起一行写 `NEEDS_INPUT: <你的具体问题>`,经理会回")];
        assert_eq!(extract_needs_input(&convention), None);
        // 正常完成、没请示 → None。
        assert_eq!(extract_needs_input(&[chunk("已完成排序函数并通过测试")]), None);
    }

    #[test]
    fn run_mode_deserializes_lowercase() {
        let m: RunMode = serde_json::from_str("\"simulate\"").unwrap();
        assert_eq!(m, RunMode::Simulate);
        let m: RunMode = serde_json::from_str("\"real\"").unwrap();
        assert_eq!(m, RunMode::Real);
    }

    #[test]
    fn run_mode_label_round_trips() {
        assert_eq!(RunMode::from_label("real"), RunMode::Real);
        assert_eq!(RunMode::from_label("simulate"), RunMode::Simulate);
        // Unknown / corrupt label falls back to the safe free default.
        assert_eq!(RunMode::from_label("garbage"), RunMode::Simulate);
        assert_eq!(RunMode::Real.label(), "real");
        assert_eq!(RunMode::Simulate.label(), "simulate");
    }

}
