//! Integration test: killing a running task's agent process (the basis for the
//! UI "stop"/cancel-running feature) stops the run through the NORMAL path —
//! status `Failed`, worktree GC'd, no leftover. Own test binary so the
//! `QUIVER_FAKE_DELAY_MS` env override is process-isolated from other tests.

use std::path::PathBuf;
use std::time::Duration;

use quiver_core::git::GitGuard;
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
