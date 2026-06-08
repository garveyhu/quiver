//! The bulletin-board queue scheduler (v1.0 spec module 5, Phase C).
//!
//! Phase B ran a SINGLE task per `run_task_cmd` invocation. This module turns the
//! `task` table into a live queue that runs **up to `maxWorkers` tasks
//! CONCURRENTLY**, each in its own `tokio::spawn`ed worker, against the user's
//! picked repo. The `quiver-core` `GitGuard` (one mutex serializing shared-`.git`
//! metadata ops) + the merge lock make concurrent worktrees safe — so this layer
//! only has to (a) cap concurrency and (b) hand each `queued` task to exactly one
//! worker.
//!
//! ## Design
//!
//! - **One dispatcher loop per project.** Enqueueing a task ensures a dispatcher
//!   is running for that project (idempotent — a second enqueue while one runs is
//!   a no-op). The dispatcher repeatedly tries to acquire a concurrency permit
//!   (a `Semaphore` sized to the live `maxWorkers`) and then atomically claims the
//!   next `queued` task (`Store::claim_next_queued`, which flips it to `running`
//!   in one transaction so two ticks can't grab the same row). For each claimed
//!   task it `tokio::spawn`s a worker that runs the task to completion, persists
//!   the outcome, and drops the permit — freeing a slot for the next queued task.
//!   When the queue is empty AND no worker is in flight, the dispatcher exits.
//!
//! - **Shared `GitGuard` per project.** All concurrent workers on one project
//!   share ONE `Arc<GitGuard>` so the §6 metadata mutex actually serializes their
//!   shared-`.git` ops. Different projects get different guards (they have
//!   separate `.git`).
//!
//! - **Live UI.** Every status transition emits a `task-updated` event so the
//!   board re-reads `list_tasks`; the per-task AgentEvent stream already flows on
//!   the `agent-event` channel tagged by `task_id`, which the 工坊 keys on — so up
//!   to `maxWorkers` archers animate at once with no extra plumbing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Semaphore};

use quiver_core::git::GitGuard;
use quiver_store::{Settings, Store};

use crate::run::{run_one_task, RunMode};

/// Whether the §10 budget gate should pause the queue, given ROLLING-window
/// spends: `spent_day` = last 24h (vs the nightly budget), `spent_month` ≈ last
/// 30d (vs the monthly credit cap). A cap that is `None` or ≤ 0 is "unset" and
/// never trips — so with no caps configured the gate is inert (queue unchanged).
/// Rolling windows are deliberately used over an all-time sum so a periodic cap
/// resets as old spend ages out, instead of pausing forever once exceeded.
fn over_budget(settings: &Settings, spent_day: f64, spent_month: f64) -> bool {
    let hit = |cap: Option<f64>, spent: f64| cap.map_or(false, |c| c > 0.0 && spent >= c);
    hit(settings.nightly_budget_usd, spent_day) || hit(settings.monthly_credit_cap_usd, spent_month)
}

/// The Tauri event channel the board subscribes to: emitted on every task
/// lifecycle transition (enqueue / claim / finish / cancel) so the UI re-reads
/// `list_tasks`. The payload is the project path the change happened in.
pub const TASK_EVENT_CHANNEL: &str = "task-updated";

/// Shared scheduler state, owned by `AppState`. One [`ProjectQueue`] per project
/// path (its dispatcher + concurrency permits), created lazily on first enqueue.
#[derive(Default)]
pub struct Scheduler {
    projects: Mutex<HashMap<String, Arc<ProjectQueue>>>,
}

/// Per-project queue runtime: the shared git guard all its workers serialize on,
/// a concurrency-permit semaphore (sized to the saved `maxWorkers`), and a
/// "dispatcher running" flag so only one drain loop exists per project.
struct ProjectQueue {
    project: PathBuf,
    guard: Arc<GitGuard>,
    permits: Arc<Semaphore>,
    dispatcher_running: Mutex<bool>,
}

impl Scheduler {
    /// Ensure a task pinned to the board for `project` gets picked up: lazily
    /// create the project's queue runtime, then (re)start its dispatcher if it is
    /// not already draining. Cheap + idempotent — safe to call on every enqueue.
    ///
    /// `max_workers` is the live cap from settings; it sizes the per-project
    /// semaphore the first time the project is seen. (Changing the cap takes
    /// effect for projects whose queue is created after the change — a running
    /// dispatcher keeps its existing permit budget for the rest of its drain.)
    pub async fn ensure_running(
        &self,
        app: AppHandle,
        store: Arc<Store>,
        project: PathBuf,
        max_workers: usize,
    ) {
        let key = project.display().to_string();
        let queue = {
            let mut projects = self.projects.lock().await;
            projects
                .entry(key.clone())
                .or_insert_with(|| {
                    let cap = max_workers.max(1);
                    Arc::new(ProjectQueue {
                        project: project.clone(),
                        guard: Arc::new(GitGuard::new(project.clone())),
                        permits: Arc::new(Semaphore::new(cap)),
                        dispatcher_running: Mutex::new(false),
                    })
                })
                .clone()
        };
        Self::spawn_dispatcher_if_idle(app, store, queue).await;
    }

    /// Start the drain loop for `queue` unless one is already running. The flag
    /// is checked-and-set under a mutex so concurrent enqueues spawn at most one
    /// dispatcher.
    async fn spawn_dispatcher_if_idle(app: AppHandle, store: Arc<Store>, queue: Arc<ProjectQueue>) {
        {
            let mut running = queue.dispatcher_running.lock().await;
            if *running {
                return;
            }
            *running = true;
        }
        let queue_for_task = queue.clone();
        tokio::spawn(async move {
            dispatch_loop(app, store, queue_for_task.clone()).await;
            // Mark idle so a future enqueue can restart the loop.
            *queue_for_task.dispatcher_running.lock().await = false;
        });
    }

    /// Re-kick every known project's dispatcher (no-op for ones already running).
    /// Called after a settings change so a queue the §10 budget gate paused
    /// resumes when the cap is raised — without needing a fresh enqueue. Each
    /// restarted dispatcher re-evaluates the gate, so if still over budget it
    /// simply exits again. Idempotent + cheap.
    pub async fn resume_all(&self, app: AppHandle, store: Arc<Store>) {
        let queues: Vec<Arc<ProjectQueue>> = {
            let projects = self.projects.lock().await;
            projects.values().cloned().collect()
        };
        for queue in queues {
            Self::spawn_dispatcher_if_idle(app.clone(), store.clone(), queue).await;
        }
    }

    /// Crash recovery on launch (DESIGN §23 P0). A previous session may have died
    /// mid-run, leaving tasks stuck `running` (no live worker owns them) and orphan
    /// worktree dirs a killed agent held. This:
    ///   1. requeues every `running` task → `queued` (its `session_id` is kept for
    ///      a future `--resume`),
    ///   2. for each project with pending work, sweeps orphan worktrees (safe now
    ///      that those processes are long dead) and (re)starts its dispatcher.
    /// Returns how many tasks were requeued. Best-effort: a per-project error never
    /// aborts recovery of the others.
    pub async fn reconcile(&self, app: AppHandle, store: Arc<Store>, max_workers: usize) -> usize {
        let requeued = store.requeue_running_tasks(crate::now_ms()).unwrap_or(0);
        let projects = store.projects_with_pending_tasks().unwrap_or_default();
        for project in projects {
            let path = PathBuf::from(&project);
            let _ = GitGuard::new(path.clone()).sweep_orphan_worktrees().await;
            self.ensure_running(app.clone(), store.clone(), path, max_workers)
                .await;
        }
        requeued
    }
}

/// Drain `queue` until it is empty: acquire a permit (blocks while all
/// `maxWorkers` slots are busy), atomically claim the next `queued` task, and
/// spawn a worker that holds the permit for the task's lifetime. Exit when no
/// queued task remains — the in-flight workers (each holding a permit) finish on
/// their own spawned tasks and emit their own terminal updates.
async fn dispatch_loop(app: AppHandle, store: Arc<Store>, queue: Arc<ProjectQueue>) {
    let project_key = queue.project.display().to_string();
    loop {
        // Wait for a free slot. `acquire_owned` hands the permit to the worker so
        // it is released exactly when the worker's task completes.
        let permit = match queue.permits.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => return, // semaphore closed — shutting down.
        };

        // §10 budget gate: if the project has spent up to a configured cap, pause
        // — stop claiming rather than overspend. cap=None (the default) → inert,
        // the queue behaves exactly as before. Conservative failure mode: a read
        // error errs toward NOT pausing (keeps work flowing), but an over-cap
        // reading pauses. A future enqueue (or raising the cap then re-pinning)
        // restarts the dispatcher, which re-checks here.
        if let Ok(settings) = store.get_settings() {
            const DAY_MS: i64 = 86_400_000;
            let now = crate::now_ms();
            let spent_day = store.cost_since(now - DAY_MS).unwrap_or(0.0);
            let spent_month = store.cost_since(now - 30 * DAY_MS).unwrap_or(0.0);
            if over_budget(&settings, spent_day, spent_month) {
                drop(permit);
                return;
            }
        }

        // Atomically claim the next queued task (flips it to `running`). If the
        // queue is empty, drop the permit and stop draining.
        let claimed = match store.claim_next_queued(&project_key, crate::now_ms()) {
            Ok(Some(task)) => task,
            Ok(None) => {
                drop(permit);
                return;
            }
            Err(_) => {
                drop(permit);
                return;
            }
        };

        // A task just flipped queued → running: tell the board.
        let _ = app.emit(TASK_EVENT_CHANNEL, &project_key);

        let app_worker = app.clone();
        let store_worker = store.clone();
        let guard = queue.guard.clone();
        let task_id = claimed.id.clone();
        let prompt = claimed.prompt.clone();
        let mode = RunMode::from_label(&claimed.mode);
        let project_for_worker = project_key.clone();

        tokio::spawn(async move {
            // The permit is moved in and dropped when this worker ends, freeing a
            // slot for the dispatcher to claim the next queued task.
            let _permit = permit;
            run_one_task(&app_worker, &store_worker, &guard, task_id, prompt, mode).await;
            // The terminal status is already persisted by `run_one_task`; notify
            // the board so the finished card + freed slot are reflected live.
            let _ = app_worker.emit(TASK_EVENT_CHANNEL, &project_for_worker);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(monthly: Option<f64>, nightly: Option<f64>) -> Settings {
        Settings {
            monthly_credit_cap_usd: monthly,
            nightly_budget_usd: nightly,
            ..Settings::default()
        }
    }

    #[test]
    fn over_budget_inert_when_no_caps() {
        assert!(!over_budget(&caps(None, None), 9999.0, 9999.0));
        // 0 / negative caps are treated as "unset" — the gate stays inert.
        assert!(!over_budget(&caps(Some(0.0), Some(0.0)), 9999.0, 9999.0));
    }

    #[test]
    fn over_budget_trips_nightly_on_day_window() {
        let s = caps(None, Some(5.0));
        assert!(over_budget(&s, 5.0, 0.0)); // 24h spend reached the nightly cap
        assert!(!over_budget(&s, 4.99, 9999.0)); // under nightly; the month window is irrelevant
    }

    #[test]
    fn over_budget_trips_monthly_on_month_window() {
        let s = caps(Some(100.0), None);
        assert!(over_budget(&s, 0.0, 100.0)); // 30d spend reached the monthly cap
        assert!(!over_budget(&s, 9999.0, 99.0)); // under monthly; the day window is irrelevant
    }
}
