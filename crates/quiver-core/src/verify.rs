//! The verify-gate: run a configurable build/test command in a worktree
//! (DESIGN §7).
//!
//! A run is only allowed to merge if the agent produced `Result{ok:true}` AND
//! this command exits 0 against the worktree's contents. The same command is
//! re-run against the merged `main` (DESIGN §7 step 4) to catch the
//! "A and B each green alone, red together" case — so this module is the single
//! definition of "what green means," used both pre-merge and post-merge.
//!
//! The command is an explicit argv (`Vec<String>`); a shell string is expressed
//! as `["sh", "-c", "<script>"]` by the caller. Exit 0 → [`VerifyResult::Passed`];
//! any non-zero exit (or a signal) → [`VerifyResult::Failed`]. A command that
//! cannot even be spawned is a hard error (surfaced to the caller), not a silent
//! "failed", because an un-spawnable verify command is a misconfiguration, not a
//! red test.

use std::path::Path;

use anyhow::{Context, Result};

/// A configurable verify command (DESIGN §7): the argv executed in the task's
/// worktree to decide whether the work is green.
///
/// Built from an explicit argv. For a shell one-liner, construct it from
/// `["sh", "-c", script]` — the gate never implicitly wraps anything in a shell,
/// so what runs is exactly what the caller specified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyCommand {
    argv: Vec<String>,
}

impl VerifyCommand {
    /// Build a verify command from an explicit argv. The first element is the
    /// program; the rest are its arguments.
    pub fn new(argv: impl IntoIterator<Item = impl Into<String>>) -> Result<Self> {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.is_empty() {
            anyhow::bail!("verify command argv must not be empty");
        }
        Ok(Self { argv })
    }

    /// Convenience for the common shell-string form: runs `sh -c <script>`.
    pub fn shell(script: impl Into<String>) -> Self {
        Self {
            argv: vec!["sh".to_string(), "-c".to_string(), script.into()],
        }
    }

    /// The argv that will be executed (for logging / display).
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// Run the command with `cwd = dir` and classify the exit (DESIGN §7).
    ///
    /// Exit 0 → [`VerifyResult::Passed`]; any non-zero exit or termination by
    /// signal → [`VerifyResult::Failed`]. A spawn failure (program not found,
    /// not executable) is returned as an `Err` — that is a misconfigured gate,
    /// distinct from a red test.
    pub async fn run(&self, dir: &Path) -> Result<VerifyResult> {
        let (program, args) = self
            .argv
            .split_first()
            .expect("argv is non-empty by construction");

        let status = tokio::process::Command::new(program)
            .args(args)
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .status()
            .await
            .with_context(|| format!("failed to spawn verify command {program:?}"))?;

        Ok(if status.success() {
            VerifyResult::Passed
        } else {
            VerifyResult::Failed
        })
    }
}

/// Outcome of running the verify-gate (DESIGN §7): green (exit 0) or red.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyResult {
    Passed,
    Failed,
}

impl VerifyResult {
    /// Did the gate pass?
    pub fn passed(self) -> bool {
        matches!(self, VerifyResult::Passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn passing_command_yields_passed() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::shell("exit 0");
        let res = cmd.run(dir.path()).await.expect("run");
        assert_eq!(res, VerifyResult::Passed);
        assert!(res.passed());
    }

    #[tokio::test]
    async fn failing_command_yields_failed() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::shell("exit 1");
        let res = cmd.run(dir.path()).await.expect("run");
        assert_eq!(res, VerifyResult::Failed);
        assert!(!res.passed());
    }

    #[tokio::test]
    async fn command_runs_in_the_given_cwd() {
        // The command observes the worktree it is pointed at: a file present in
        // `dir` makes the gate pass, its absence makes it fail.
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("marker"), "x").expect("write marker");

        let present = VerifyCommand::shell("test -f marker");
        assert_eq!(present.run(dir.path()).await.expect("run"), VerifyResult::Passed);

        let absent = VerifyCommand::shell("test -f does-not-exist");
        assert_eq!(absent.run(dir.path()).await.expect("run"), VerifyResult::Failed);
    }

    #[tokio::test]
    async fn explicit_argv_is_run_verbatim() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::new(["true"]).expect("argv");
        assert_eq!(cmd.run(dir.path()).await.expect("run"), VerifyResult::Passed);

        let cmd = VerifyCommand::new(["false"]).expect("argv");
        assert_eq!(cmd.run(dir.path()).await.expect("run"), VerifyResult::Failed);
    }

    #[tokio::test]
    async fn unspawnable_command_is_an_error_not_a_red_test() {
        let dir = TempDir::new().expect("tempdir");
        let cmd = VerifyCommand::new(["/nonexistent/quiver/definitely-not-a-program"])
            .expect("argv");
        assert!(
            cmd.run(dir.path()).await.is_err(),
            "an un-spawnable verify command must surface as an error, not Failed"
        );
    }

    #[test]
    fn empty_argv_is_rejected() {
        let empty: Vec<String> = Vec::new();
        assert!(VerifyCommand::new(empty).is_err());
    }
}
