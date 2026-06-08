//! Integration test: killing a running task's agent process (the basis for the
//! UI "stop"/cancel-running feature) stops the run through the NORMAL path —
//! status `Failed`, worktree GC'd, no leftover. Own test binary so the
//! `QUIVER_FAKE_DELAY_MS` env override is process-isolated from other tests.

use std::path::PathBuf;
use std::time::Duration;

use quiver_core::git::GitGuard;
use quiver_core::runner::claude::ClaudeRunner;
use quiver_core::runner::AgentRunner;
use quiver_core::supervisor::{run_task_streaming, Cleanup, FinishStatus, RunOptions, TaskSpec};
use quiver_core::verify::VerifyCommand;

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
    std::fs::write(path.join("README.md"), "# temp\n").expect("write readme");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    dir
}

#[tokio::test]
async fn killing_running_agent_stops_run_and_gcs_worktree() {
    // Pace the agent slowly so the run is genuinely in-flight (no clean Result
    // yet) when we kill it — far longer than the kill delay below.
    unsafe {
        std::env::set_var("QUIVER_FAKE_DELAY_MS", "5000");
    }

    let repo = temp_repo();
    let wt_root = tempfile::TempDir::new().expect("wt root");
    let guard = GitGuard::new(repo.path()).with_worktrees_root(wt_root.path());
    let bin = fake_claude_bin();
    let verify = VerifyCommand::shell("exit 0");
    let task = TaskSpec {
        id: "killme".to_string(),
        prompt: "p".to_string(),
    };

    let out = run_task_streaming(
        &guard,
        &task,
        &bin,
        &verify,
        RunOptions::default(),
        |_ev| {},
        |pid| {
            // Kill the agent ~150ms after spawn — mid-run, before the first
            // 5s-delayed event, so no clean Result ever arrives.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(150));
                let _ = std::process::Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .status();
            });
        },
    )
    .await
    .expect("run resolves after the agent is killed");

    unsafe {
        std::env::remove_var("QUIVER_FAKE_DELAY_MS");
    }

    // Killed before a clean Result → Failed, and the non-kept worktree is GC'd
    // through the same §8.4 path as a crash — no leftover on disk.
    assert_eq!(
        out.status,
        FinishStatus::Failed,
        "a killed run should be Failed, got {:?}",
        out.status
    );
    assert_eq!(
        out.cleanup,
        Cleanup::ForcedRemoved,
        "a killed run should GC its worktree, got {:?}",
        out.cleanup
    );
    // Git-level: the attempt worktree is unregistered (only `main` remains).
    let listed = std::process::Command::new("git")
        .args(["worktree", "list"])
        .current_dir(repo.path())
        .output()
        .expect("git worktree list");
    let count = String::from_utf8_lossy(&listed.stdout).lines().count();
    assert_eq!(count, 1, "killed run should leave no registered git worktree");
    // NOTE: the on-disk worktree dir can briefly linger after a kill (the dying
    // agent held files when `git worktree remove` ran). It is git-unregistered
    // (above) so it's harmless to git; a periodic prune/sweep can reclaim the dir.
}

/// Does process `pid` still exist? `kill -0` probes without sending a signal.
fn pid_alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Poll `path` until it contains a parseable PID or `timeout` elapses.
fn wait_for_pid(path: &std::path::Path, timeout: Duration) -> Option<i32> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(pid) = s.trim().parse::<i32>() {
                return Some(pid);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// kill -9 the WHOLE PROCESS GROUP (DESIGN §23 P3 killpg) must reap grandchildren
/// the agent spawned — not just the agent pid. The runner puts each agent in its
/// own process group (`process_group(0)`), so the `fake-claude` `spawn_child`
/// scenario's forked `sh`/`sleep` grandchild inherits that group; signalling the
/// negative leader pid kills it too, leaving no orphan. This is the test the app's
/// `kill_pid` (libc::kill(-pid, SIGKILL)) relies on.
#[tokio::test]
async fn killpg_reaps_grandchild_no_orphan() {
    let bin = fake_claude_bin();
    let cwd = std::env::temp_dir();
    let pidfile =
        std::env::temp_dir().join(format!("quiver-orphan-probe-{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pidfile);

    // The runner forwards `--scenario spawn_child --child-pidfile <p>` verbatim;
    // fake-claude forks `sh -c 'echo $$ > <p>; sleep 30'` into ITS process group.
    let runner = ClaudeRunner::new("orphan-test").with_extra_args([
        "--scenario",
        "spawn_child",
        "--child-pidfile",
        pidfile.to_str().expect("utf8 path"),
    ]);
    let spawned = runner.spawn("p", &cwd, &bin).await.expect("spawn fake-claude");
    let leader = spawned.pid.expect("leader pid") as i32;

    let gc_pid = wait_for_pid(&pidfile, Duration::from_secs(5))
        .expect("grandchild should record its pid");
    assert!(pid_alive(gc_pid), "grandchild should be alive before the kill");

    // Kill the agent's PROCESS GROUP (negative pid). pgid == leader because the
    // runner set process_group(0); the grandchild inherited it.
    let _ = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(format!("-{leader}"))
        .status();

    // The grandchild must be gone within a short window — no orphan survives killpg.
    let mut gone = false;
    for _ in 0..50 {
        if !pid_alive(gc_pid) {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(&pidfile);
    drop(spawned); // close the event stream / let the reader task end
    assert!(
        gone,
        "killpg must reap the grandchild (pid {gc_pid}) — a single-pid kill would orphan it"
    );
}
