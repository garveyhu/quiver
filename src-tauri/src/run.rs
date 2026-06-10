//! Running one task end-to-end: resolve the agent binary, stream its events live
//! to the UI, persist every event + the terminal status/cost/branch.
//!
//! This is the shared core both the legacy single-shot path and the Phase-C
//! queue scheduler call. It is deliberately decoupled from Tauri's `State` /
//! command machinery: it takes a plain `&AppHandle`, an `Arc<Store>`, and a
//! shared `Arc<GitGuard>` (so concurrent workers on one project serialize their
//! shared-`.git` metadata ops through the §6 mutex), making it safe to invoke
//! from inside a `tokio::spawn`ed worker.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use quiver_core::event::{AgentEvent, AgentEventPayload};
use quiver_core::git::GitGuard;
use quiver_core::supervisor::{
    run_task_streaming, Cleanup, FinishStatus, RunOptions, RunOutcome, TaskSpec,
};
use quiver_core::verify::VerifyCommand;
use quiver_memory::NewEpisode;
use quiver_store::{NewEvent, NewRun, Settings, Store};

/// The Tauri event channel the UI subscribes to (see `useSupervisor.ts`).
const AGENT_EVENT_CHANNEL: &str = "agent-event";

/// Env vars that, if present, mean the `claude` CLI would NOT be on the
/// subscription/OAuth route (§9.3). In `real` (subscription) mode we assert NONE
/// of these are present in the scrubbed child env before spawning — if any is,
/// we refuse to launch rather than silently spend on a key/Bedrock/Vertex route.
const FORBIDDEN_SUBSCRIPTION_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

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
    let worker_role = store.and_then(|s| pick_worker_role(s, &task_id, &prompt));
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
    // §5 双向协作(worker→经理):告诉 worker 卡住别硬猜 —— 遇到该上级拍板的点写 NEEDS_INPUT,
    // 经理会给指示;能自己合理决定的正常做完,不必事事请示。
    let prompt = format!(
        "{role_intro}{prompt}\n\n[协作约定] 遇到需要上级拍板的点(架构选择、模糊或缺失的需求、重大取舍),\
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

/// §9.3 pre-spawn guard: assert NONE of the API-key/Bedrock/Vertex env vars are
/// present, so a real-mode (subscription) run cannot silently take a non-OAuth
/// route. Exposed `pub(crate)` so the environment pre-flight check
/// ([`crate::environment`]) reports the SAME guard the real launch path enforces.
pub(crate) fn assert_subscription_env() -> anyhow::Result<()> {
    assert_no_forbidden_env(|key| std::env::var_os(key).is_some())
}

/// The API-key/Bedrock/Vertex env vars that would route a "real" run around the
/// subscription/OAuth path (§9.3). Exposed so the pre-flight check can name the
/// offending var without re-spelling the list.
pub(crate) const SUBSCRIPTION_ENV_KEYS: &[&str] = FORBIDDEN_SUBSCRIPTION_ENV;

/// Pure core of [`assert_subscription_env`]: error if `is_present` reports any of
/// the [`FORBIDDEN_SUBSCRIPTION_ENV`] vars set.
fn assert_no_forbidden_env(is_present: impl Fn(&str) -> bool) -> anyhow::Result<()> {
    for key in FORBIDDEN_SUBSCRIPTION_ENV {
        if is_present(key) {
            anyhow::bail!(
                "refusing to launch real mode: {key} is set, which would route around \
                 the subscription/OAuth path (§9.3). Unset it and retry."
            );
        }
    }
    Ok(())
}

/// Resolve the agent binary honoring the saved `agent_bin_override` (Phase B)
/// before the per-mode auto-resolution.
/// 给一个任务挑员工(§14 智能协作):**先按专长匹配** —— 任务描述里出现某员工的专长标签
/// (如「测试」「前端」)就派给他;没有专长命中再按 task_id 稳定散开到通用员工。于是
/// "配前端专长的员工接前端活、配测试的接测试活",配置 + 调度接起来。无员工角色 → `None`。
fn pick_worker_role(store: &Store, task_id: &str, prompt: &str) -> Option<quiver_store::AgentRole> {
    let workers: Vec<quiver_store::AgentRole> = store
        .list_roles()
        .ok()?
        .into_iter()
        .filter(|r| r.kind == "worker")
        .collect();
    pick_worker_from(workers, task_id, prompt)
}

/// 纯逻辑(可单测):从一组员工里按"专长命中任务描述 > task_id 稳定散开"挑一个。
fn pick_worker_from(
    workers: Vec<quiver_store::AgentRole>,
    task_id: &str,
    prompt: &str,
) -> Option<quiver_store::AgentRole> {
    if workers.is_empty() {
        return None;
    }
    // 专长匹配:任务描述包含某员工的(非空、非"通用")专长标签 → 派给他。
    if let Some(hit) = workers.iter().find(|r| {
        let s = r.specialty.trim();
        !s.is_empty() && s != "通用" && prompt.contains(s)
    }) {
        return Some(hit.clone());
    }
    // 无专长命中:**优先派给通用员工**(specialty 空 / "通用")—— 把有专长的留给专长活,别让
    // 通用活占着测试/前端员工(否则真来了专长任务、专长员工却在忙通用活)。通用员工里按 task_id
    // 稳定散开;一个通用员工都没有(全配了专长)→ 退回到全体散开。
    let generic: Vec<quiver_store::AgentRole> = workers
        .iter()
        .filter(|r| {
            let s = r.specialty.trim();
            s.is_empty() || s == "通用"
        })
        .cloned()
        .collect();
    let pool = if generic.is_empty() { workers } else { generic };
    let idx = task_id.bytes().map(usize::from).sum::<usize>() % pool.len();
    pool.into_iter().nth(idx)
}

pub(crate) fn resolve_agent_bin(settings: &Settings, mode: RunMode) -> anyhow::Result<PathBuf> {
    if let Some(override_path) = settings
        .agent_bin_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(override_path);
        if is_executable(&p) {
            return Ok(p);
        }
        anyhow::bail!(
            "设置里的 agent 二进制路径不可执行：{}。请修正或清空该项以自动解析。",
            p.display()
        );
    }
    match mode {
        RunMode::Simulate => resolve_fake_claude(),
        RunMode::Real => resolve_real_claude(),
    }
}

/// Resolve the free `fake-claude` test double to an ABSOLUTE path (DESIGN §12).
fn resolve_fake_claude() -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("QUIVER_AGENT_BIN") {
        let p = PathBuf::from(override_path);
        if is_executable(&p) {
            return Ok(p);
        }
        anyhow::bail!(
            "QUIVER_AGENT_BIN points at a non-executable path: {}",
            p.display()
        );
    }

    let exe = std::env::current_exe()?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("current_exe has no parent dir"))?;
    let candidates = [
        exe_dir.join("fake-claude"),
        exe_dir.join("debug").join("fake-claude"),
        exe_dir.join("release").join("fake-claude"),
    ];
    for candidate in candidates {
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "could not resolve the fake-claude agent binary near {} — run `cargo build` \
         so target/<profile>/fake-claude exists, or set QUIVER_AGENT_BIN",
        exe_dir.display()
    )
}

/// Resolve the official `claude` binary to an ABSOLUTE path (DESIGN §12).
/// Exposed `pub(crate)` so the environment pre-flight check
/// ([`crate::environment`]) resolves the binary through the EXACT same path the
/// real launch uses — guaranteeing "the check passed" implies "launch will find
/// claude".
pub(crate) fn resolve_real_claude() -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("QUIVER_CLAUDE_BIN") {
        let p = PathBuf::from(override_path);
        if is_executable(&p) {
            return Ok(p);
        }
        anyhow::bail!(
            "QUIVER_CLAUDE_BIN points at a non-executable path: {}",
            p.display()
        );
    }

    if let Some(p) = which_in_path("claude") {
        return Ok(p);
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let candidates = [
            home.join(".local").join("bin").join("claude"),
            home.join(".claude").join("local").join("claude"),
        ];
        for candidate in candidates {
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    for fixed in ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"] {
        let p = PathBuf::from(fixed);
        if is_executable(&p) {
            return Ok(p);
        }
    }

    anyhow::bail!(
        "could not find the `claude` binary. Install it, or set QUIVER_CLAUDE_BIN \
         to its absolute path."
    )
}

/// First executable `name` found by scanning `PATH` (absolute path), or `None`.
/// Exposed `pub(crate)` for the environment pre-flight check (`git` probe).
pub(crate) fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Is `path` a real, executable file?
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
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

    fn worker(id: &str, specialty: &str) -> quiver_store::AgentRole {
        quiver_store::AgentRole {
            id: id.into(),
            name: id.into(),
            kind: "worker".into(),
            brain: "rule".into(),
            model: "sonnet".into(),
            system_prompt: String::new(),
            budget_usd: None,
            max_turns: None,
            specialty: specialty.into(),
            version: 1,
            updated_at: 0,
        }
    }

    #[test]
    fn pick_worker_prefers_specialty_match() {
        let team = vec![worker("w1", "通用"), worker("w2", "测试"), worker("w3", "前端")];
        // 任务描述含"测试" → 派给测试专长的 w2(不管 task_id)。
        let r = pick_worker_from(team.clone(), "task-abc", "给登录补端到端测试").unwrap();
        assert_eq!(r.id, "w2", "专长命中优先");
        // 含"前端" → w3。
        let r = pick_worker_from(team.clone(), "task-abc", "给看板加前端暗色模式").unwrap();
        assert_eq!(r.id, "w3");
        // 无专长命中 → 优先派通用员工 w1(留专长员工接专长活)+ 按 task_id 稳定(同 id 同人)。
        let a = pick_worker_from(team.clone(), "task-xyz", "清理废弃依赖").unwrap();
        let b = pick_worker_from(team.clone(), "task-xyz", "清理废弃依赖").unwrap();
        assert_eq!(a.id, b.id, "同任务稳定到同一员工");
        assert_eq!(a.specialty, "通用", "无专长任务优先派通用员工,不占测试/前端员工");
        // 全员都有专长(没通用)→ 退回全体散开,不至于派不出去。
        let allspec = vec![worker("s1", "测试"), worker("s2", "前端")];
        assert!(pick_worker_from(allspec, "task-xyz", "清理废弃依赖").is_some());
        // 空团队 → None。
        assert!(pick_worker_from(vec![], "t", "x").is_none());
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

    #[test]
    fn subscription_env_assertion_rejects_api_key() {
        let result = assert_no_forbidden_env(|key| key == "ANTHROPIC_API_KEY");
        assert!(result.is_err(), "an API key present must refuse real mode");
    }

    #[test]
    fn subscription_env_assertion_rejects_bedrock_and_vertex() {
        assert!(assert_no_forbidden_env(|k| k == "CLAUDE_CODE_USE_BEDROCK").is_err());
        assert!(assert_no_forbidden_env(|k| k == "CLAUDE_CODE_USE_VERTEX").is_err());
        assert!(assert_no_forbidden_env(|k| k == "ANTHROPIC_AUTH_TOKEN").is_err());
    }

    #[test]
    fn subscription_env_assertion_passes_when_clean() {
        assert!(assert_no_forbidden_env(|_| false).is_ok());
    }
}
