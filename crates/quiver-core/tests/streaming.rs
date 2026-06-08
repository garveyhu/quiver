//! Integration test for TRUE live event streaming (Problem 1).
//!
//! Drives `run_task_streaming` against the scripted `fake-claude` double with a
//! non-zero per-line pacing delay (`QUIVER_FAKE_DELAY_MS`) and asserts the
//! `on_event` callback fires INCREMENTALLY over wall-clock time — the first event
//! lands well before the last — rather than all at once after the run finishes.
//! This is the regression guard for the original batch-emit bug.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use quiver_core::git::GitGuard;
use quiver_core::supervisor::{run_task_streaming, FinishStatus, RunOptions, TaskSpec};
use quiver_core::verify::VerifyCommand;

/// Resolve the `fake-claude` binary from the integration test's own exe path
/// (`<target>/<profile>/deps/streaming-<hash>` → `<target>/<profile>/fake-claude`).
fn fake_claude_bin() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    let bin = dir.join(format!("fake-claude{}", std::env::consts::EXE_SUFFIX));
    assert!(
        bin.exists(),
        "fake-claude binary not found at {} — run via `cargo test`",
        bin.display()
    );
    bin
}

/// A temp git repo with an initial commit on `main`.
fn temp_repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?} failed");
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "test@quiver.local"]);
    run(&["config", "user.name", "Quiver Test"]);
    std::fs::write(path.join("README.md"), "# temp repo\n").expect("write readme");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "initial commit"]);
    dir
}

#[tokio::test]
async fn events_arrive_incrementally_not_all_at_end() {
    // SAFETY: tests in this binary are serialized by the runtime; this is the
    // only place that touches QUIVER_FAKE_DELAY_MS. A 250ms per-line delay keeps
    // the test quick (~1s for the happy script's 4 events) while leaving a clear
    // time gap between the first and last callback.
    unsafe {
        std::env::set_var("QUIVER_FAKE_DELAY_MS", "250");
    }

    let repo = temp_repo();
    let wt_root = tempfile::TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
    let bin = fake_claude_bin();
    let verify = VerifyCommand::shell("exit 0");
    let task = TaskSpec {
        id: "live-1".to_string(),
        prompt: "summarize the readme".to_string(),
    };

    let start = Instant::now();
    let timeline: Arc<Mutex<Vec<u128>>> = Arc::new(Mutex::new(Vec::new()));
    let timeline_cb = Arc::clone(&timeline);

    let out = run_task_streaming(
        &guard,
        &task,
        &bin,
        &verify,
        RunOptions::default(),
        move |_ev| {
            timeline_cb
                .lock()
                .expect("timeline lock")
                .push(start.elapsed().as_millis());
        },
        |_pid| {},
    )
    .await
    .expect("streaming run ok");

    assert_eq!(out.status, FinishStatus::Verified);

    let timeline = Arc::try_unwrap(timeline)
        .expect("no other refs")
        .into_inner()
        .expect("timeline inner");

    assert!(
        timeline.len() >= 4,
        "expected several live events, got {}",
        timeline.len()
    );

    let first = timeline.first().copied().expect("first event time");
    let last = timeline.last().copied().expect("last event time");

    // The decisive live-ness assertion: the last event lands meaningfully later
    // than the first. With a 250ms/line delay across 4 events the spread is
    // ~750ms; require a conservative >=400ms so the batch-emit bug (all events at
    // the same instant) fails this test.
    assert!(
        last - first >= 400,
        "events did not arrive incrementally: first@{first}ms last@{last}ms (spread {}ms)",
        last - first
    );

    // The first event must also land BEFORE the run returns by a clear margin —
    // i.e. the UI would have seen it mid-run, not in a post-run dump.
    let total = start.elapsed().as_millis();
    assert!(
        first + 300 <= total,
        "first event@{first}ms was not clearly before run end@{total}ms"
    );

    // Reset so other tests in this binary see the default pacing.
    unsafe {
        std::env::remove_var("QUIVER_FAKE_DELAY_MS");
    }
}
