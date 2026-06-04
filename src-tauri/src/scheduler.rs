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
use quiver_store::Store;

use crate::run::{run_one_task, RunMode};

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
