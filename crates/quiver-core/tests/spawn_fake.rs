//! Integration test for `ClaudeRunner` (DESIGN Phase 0, Task 0.7).
//!
//! Drives the runner against the scripted `fake-claude` test double (free,
//! deterministic — never the real, paid `claude` binary) and asserts the
//! normalized event stream comes out in the expected order.
//!
//! NOTE on binary resolution: the plan suggested `CARGO_BIN_EXE_fake-claude`,
//! but Cargo only exports that variable for the *test package's own* binary
//! targets — not for a sibling workspace binary crate like `fake-claude` (a
//! dev-dependency on it is ignored for lacking a lib target, and the env var is
//! never set). So we resolve the binary from the test runner's own location
//! (`current_exe()` → the cargo target dir → `fake-claude[.exe]`), which is the
//! standard idiom for locating a sibling workspace binary from an integration
//! test. The workspace build guarantees the binary is present.

use std::path::PathBuf;

use quiver_core::event::{AgentEventPayload, RunnerKind};
use quiver_core::runner::AgentRunner;
use quiver_core::runner::claude::ClaudeRunner;

/// Resolve the `fake-claude` binary from the integration test's own exe path.
/// The test runner lives at `<target>/<profile>/deps/spawn_fake-<hash>`, so the
/// sibling binary is `<target>/<profile>/fake-claude`.
fn fake_claude_bin() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop(); // drop the test binary file name
    if dir.ends_with("deps") {
        dir.pop(); // drop `deps`, leaving the profile dir
    }
    let bin = dir.join(format!("fake-claude{}", std::env::consts::EXE_SUFFIX));
    assert!(
        bin.exists(),
        "fake-claude binary not found at {} — run via `cargo test`",
        bin.display()
    );
    bin
}

#[tokio::test]
async fn streams_ordered_events_from_fake_claude() {
    let bin = fake_claude_bin();
    let cwd = std::env::temp_dir();

    let runner = ClaudeRunner::new("task-1");
    let mut rx = runner
        .spawn("do the thing", &cwd, &bin)
        .await
        .expect("spawn fake-claude")
        .events;

    let mut events = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }

    // Envelope: every event stamped with the task id, ClaudeCli, gap-free seq.
    assert!(!events.is_empty(), "expected a non-empty event stream");
    for (i, ev) in events.iter().enumerate() {
        assert_eq!(ev.task_id, "task-1");
        assert_eq!(ev.seq, i as u64);
        assert!(matches!(ev.runner, RunnerKind::ClaudeCli));
    }

    // Ordered payload sequence: WorkerStarted → ToolUse → OutputChunk → Result.
    let kinds: Vec<&AgentEventPayload> = events.iter().map(|e| &e.payload).collect();
    // fake-claude 的 happy 工作弧(读→搜→改→测→报):首 WorkerStarted、尾成功 Result,
    // 其间有工具调用与文本输出。断言**形状**而非固定下标 —— 脚本步骤数会随演进变。
    assert!(
        matches!(kinds[0], AgentEventPayload::WorkerStarted { .. }),
        "first event should be WorkerStarted, got {:?}",
        kinds[0]
    );
    match kinds.last() {
        Some(AgentEventPayload::Result { ok, cost_usd, .. }) => {
            assert!(*ok, "final Result should be ok");
            assert!(cost_usd.is_some(), "Result should carry a cost_usd");
        }
        other => panic!("last event should be a successful Result, got {other:?}"),
    }
    assert!(
        kinds.iter().any(|k| matches!(k, AgentEventPayload::ToolUse { .. })),
        "work arc should include at least one ToolUse"
    );
    assert!(
        kinds.iter().any(|k| matches!(k, AgentEventPayload::OutputChunk { .. })),
        "work arc should include at least one OutputChunk"
    );
}

#[tokio::test]
async fn resume_threads_session_id_through_to_worker_started() {
    let bin = fake_claude_bin();
    let cwd = std::env::temp_dir();

    let runner = ClaudeRunner::new("task-resume");
    let mut rx = runner
        .resume("sess-resumed-42", "continue the thing", &cwd, &bin)
        .await
        .expect("resume fake-claude")
        .events;

    let mut events = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }

    // resume passed `--resume sess-resumed-42` → fake-claude echoes that id in its
    // init line → the first WorkerStarted carries it. Proves the handle is threaded
    // end-to-end (the whole point of persisting session_id for crash recovery).
    assert!(!events.is_empty(), "expected a non-empty event stream");
    match &events[0].payload {
        AgentEventPayload::WorkerStarted { session_id, .. } => {
            assert_eq!(session_id.as_deref(), Some("sess-resumed-42"));
        }
        other => panic!("first event should be WorkerStarted, got {other:?}"),
    }
}
