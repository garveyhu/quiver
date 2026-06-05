//! Workshop pre-flight self-diagnosis (DESIGN §9 health check).
//!
//! `check_environment` runs a handful of CHEAP, LOCAL probes that answer "will a
//! real run even get off the ground?" BEFORE the user spends a cent. Every probe
//! is filesystem/env-only or a single bounded subprocess — **zero Anthropic API
//! calls, zero side effects that outlive the call, total wall-clock budget well
//! under 10s** (each probe carries its own short timeout).
//!
//! The `claude` binary + subscription-env probes reuse the EXACT resolution the
//! real launch path uses ([`crate::run::resolve_real_claude`] /
//! [`crate::run::assert_subscription_env`]) so a green check can never disagree
//! with what a real spawn would find.
//!
//! All user-facing strings are Chinese to match the project convention.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;

use crate::run::{resolve_real_claude, which_in_path, SUBSCRIPTION_ENV_KEYS};
use crate::AppState;

/// Per-check verdict. Serialized lowercase so the UI matches on `ok|warn|fail`.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

/// One row in the workshop health panel. `camelCase` on the wire to match the
/// TypeScript `EnvironmentCheck` type.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCheck {
    /// Stable probe id (`claude_found`, `git_found`, …) — the UI keys on this.
    pub id: String,
    /// Chinese label shown next to the status glyph.
    pub label: String,
    /// `ok | warn | fail`.
    pub status: CheckStatus,
    /// Chinese human-readable result (includes the discovered path/version).
    pub message: String,
    /// Chinese fix steps — only set for `warn`/`fail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Raw detail (resolved path, version string, …) for the expandable row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl EnvironmentCheck {
    fn ok(id: &str, label: &str, message: String, detail: Option<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Ok,
            message,
            remediation: None,
            detail,
        }
    }

    fn warn(id: &str, label: &str, message: String, remediation: String, detail: Option<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Warn,
            message,
            remediation: Some(remediation),
            detail,
        }
    }

    fn fail(id: &str, label: &str, message: String, remediation: String, detail: Option<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Fail,
            message,
            remediation: Some(remediation),
            detail,
        }
    }
}

/// Run the local workshop pre-flight checks and return one verdict per probe.
///
/// Always `Ok(..)` for a reachable environment — individual problems are encoded
/// as `warn`/`fail` rows, NOT as a command-level error, so the UI can render the
/// full panel even when several things are wrong.
#[tauri::command]
pub async fn check_environment(state: State<'_, AppState>) -> Result<Vec<EnvironmentCheck>, String> {
    let project = {
        let guard = state.project_path.lock().expect("project_path lock");
        guard.clone()
    };

    // All probes are blocking (filesystem/env/one bounded subprocess); run the
    // whole batch off the async runtime's worker threads so we never stall the
    // Tauri command executor.
    tauri::async_runtime::spawn_blocking(move || run_checks(project))
        .await
        .map_err(|e| format!("体检任务执行失败：{e}"))
}

/// The synchronous probe battery. Ordered the way the panel reads top-to-bottom.
fn run_checks(project: Option<PathBuf>) -> Vec<EnvironmentCheck> {
    let claude_path = resolve_real_claude().ok();
    vec![
        check_claude_found(claude_path.as_ref()),
        check_claude_version(claude_path.as_ref()),
        check_claude_auth(),
        check_subscription_env(),
        check_git_found(),
        check_path_fixed(),
        check_worktree_temp(project.as_ref()),
    ]
}

/// `claude` binary resolvable via the same path the real launch uses.
fn check_claude_found(claude_path: Option<&PathBuf>) -> EnvironmentCheck {
    const ID: &str = "claude_found";
    const LABEL: &str = "Claude 命令";
    match claude_path {
        Some(p) => EnvironmentCheck::ok(
            ID,
            LABEL,
            format!("已找到 claude：{}", p.display()),
            Some(p.display().to_string()),
        ),
        None => EnvironmentCheck::fail(
            ID,
            LABEL,
            "找不到 claude 命令——真实模式无法启动。".to_string(),
            "请安装官方 Claude CLI，或在「设置」里把 agent 二进制路径指向 \
             claude 的绝对路径（也可设置环境变量 QUIVER_CLAUDE_BIN）。"
                .to_string(),
            None,
        ),
    }
}

/// `claude --version` reports a version. Only meaningful when the binary was
/// found; one bounded subprocess, no network, killed if it overruns ~5s.
fn check_claude_version(claude_path: Option<&PathBuf>) -> EnvironmentCheck {
    const ID: &str = "claude_version";
    const LABEL: &str = "Claude 版本";
    let Some(path) = claude_path else {
        return EnvironmentCheck::warn(
            ID,
            LABEL,
            "未检测版本——claude 命令尚未找到。".to_string(),
            "先解决上一项「Claude 命令」，版本检测会自动恢复。".to_string(),
            None,
        );
    };

    match run_bounded(
        Command::new(path).arg("--version"),
        Duration::from_secs(5),
    ) {
        Ok(out) if out.status_ok => {
            let version = out.stdout.trim().to_string();
            let shown = if version.is_empty() {
                "（命令成功但未输出版本号）".to_string()
            } else {
                version.clone()
            };
            EnvironmentCheck::ok(
                ID,
                LABEL,
                format!("claude 版本：{shown}"),
                if version.is_empty() { None } else { Some(version) },
            )
        }
        Ok(out) => EnvironmentCheck::warn(
            ID,
            LABEL,
            "claude --version 返回了非零状态。".to_string(),
            "请确认 claude 命令可正常执行（在终端手动跑一次 `claude --version`）。"
                .to_string(),
            Some(out.stderr.trim().to_string()).filter(|s| !s.is_empty()),
        ),
        Err(e) => EnvironmentCheck::warn(
            ID,
            LABEL,
            format!("执行 claude --version 失败：{e}"),
            "请确认 claude 命令可正常执行（在终端手动跑一次 `claude --version`）。"
                .to_string(),
            None,
        ),
    }
}

/// Login state — checks for the existence of a credentials file OR, on macOS,
/// the login-Keychain item (never reads either's contents, so this costs nothing
/// and leaks nothing). If an API key env var is present we surface a warn
/// pointing at the API-key route.
fn check_claude_auth() -> EnvironmentCheck {
    const ID: &str = "claude_auth";
    const LABEL: &str = "Claude 登录态";

    if std::env::var_os("ANTHROPIC_API_KEY").is_some() {
        return EnvironmentCheck::warn(
            ID,
            LABEL,
            "检测到 ANTHROPIC_API_KEY——将走 API key 计费而非订阅。".to_string(),
            "若要用订阅额度，请取消设置 ANTHROPIC_API_KEY 后重新检查；\
             若确实要用 API key 计费请知悉这会消耗按量费用。"
                .to_string(),
            None,
        );
    }

    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return EnvironmentCheck::warn(
            ID,
            LABEL,
            "无法定位 HOME 目录，跳过登录态检测。".to_string(),
            "请确认运行环境设置了 HOME 环境变量。".to_string(),
            None,
        );
    };

    // Candidate credential files claude may write, across versions/locations.
    let candidates = [
        home.join(".claude").join(".credentials.json"),
        home.join(".claude").join("credentials.json"),
        home.join(".config").join("claude").join(".credentials.json"),
        home.join(".config").join("claude").join("credentials.json"),
    ];
    for path in &candidates {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.is_file() && meta.len() > 0 {
                return EnvironmentCheck::ok(
                    ID,
                    LABEL,
                    "已检测到 Claude 登录凭证。".to_string(),
                    Some(path.display().to_string()),
                );
            }
        }
    }

    // On macOS the official CLI stores its OAuth credentials in the login
    // Keychain (service "Claude Code-credentials"), not a plaintext file — so the
    // file probe above misses a logged-in user. Query the item's existence with
    // `security` (no `-w`, so it never prompts and never reads the secret).
    #[cfg(target_os = "macos")]
    {
        let found = std::process::Command::new("security")
            .args(["find-generic-password", "-s", "Claude Code-credentials"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if found {
            return EnvironmentCheck::ok(
                ID,
                LABEL,
                "已检测到 Claude 登录凭证（钥匙串）。".to_string(),
                Some("macOS 钥匙串：Claude Code-credentials".to_string()),
            );
        }
    }

    EnvironmentCheck::warn(
        ID,
        LABEL,
        "未检测到 Claude 登录凭证——真实模式可能因未登录而失败。".to_string(),
        "请在终端运行 `claude` 并完成登录（订阅 / OAuth），登录后重新检查。"
            .to_string(),
        None,
    )
}

/// Subscription routing guard — mirrors [`crate::run::assert_subscription_env`].
/// Fails (not just warns) when a forbidden var is set, because a real run would
/// refuse to launch. The plain API-key case is already surfaced by `claude_auth`
/// as a warn; here we cover the broader Bedrock/Vertex/Foundry/auth-token set.
fn check_subscription_env() -> EnvironmentCheck {
    const ID: &str = "subscription_env";
    const LABEL: &str = "订阅路由";

    let offenders: Vec<&str> = SUBSCRIPTION_ENV_KEYS
        .iter()
        .copied()
        .filter(|key| std::env::var_os(key).is_some())
        .collect();

    if offenders.is_empty() {
        return EnvironmentCheck::ok(
            ID,
            LABEL,
            "未检测到会绕过订阅的环境变量。".to_string(),
            None,
        );
    }

    let joined = offenders.join("、");
    EnvironmentCheck::fail(
        ID,
        LABEL,
        format!("检测到会绕过订阅/OAuth 的环境变量：{joined}。真实模式将拒绝启动。"),
        format!("请取消设置这些环境变量（{joined}）后重新检查，以走订阅额度路径。"),
        Some(joined),
    )
}

/// `git` binary resolvable on PATH.
fn check_git_found() -> EnvironmentCheck {
    const ID: &str = "git_found";
    const LABEL: &str = "Git 命令";
    match which_in_path("git") {
        Some(p) => EnvironmentCheck::ok(
            ID,
            LABEL,
            format!("已找到 git：{}", p.display()),
            Some(p.display().to_string()),
        ),
        None => EnvironmentCheck::fail(
            ID,
            LABEL,
            "在 PATH 中找不到 git 命令。".to_string(),
            "请安装 git（macOS 可运行 `xcode-select --install` 或用 Homebrew \
             安装），并确认它在 PATH 中。"
                .to_string(),
            None,
        ),
    }
}

/// Whether the sparse launchd PATH looks repaired (`fix_path_env::fix()` ran).
/// Heuristic: a healthy shell PATH includes `/opt/homebrew` or a `.local` bin.
fn check_path_fixed() -> EnvironmentCheck {
    const ID: &str = "path_fixed";
    const LABEL: &str = "PATH 修复";
    let path = std::env::var("PATH").unwrap_or_default();
    let healthy = path.contains("/opt/homebrew")
        || path.contains("/usr/local/bin")
        || path.contains(".local/bin");
    if healthy {
        EnvironmentCheck::ok(
            ID,
            LABEL,
            "PATH 看起来已修复（包含常见的命令目录）。".to_string(),
            None,
        )
    } else {
        EnvironmentCheck::warn(
            ID,
            LABEL,
            "PATH 看起来很稀疏——从 Finder 启动的 .app 可能找不到 git / claude。"
                .to_string(),
            "若命令解析正常可忽略；否则请从终端启动，或确认 PATH 包含 \
             /opt/homebrew/bin 等命令目录。"
                .to_string(),
            None,
        )
    }
}

/// The per-project worktree staging dir is creatable + writable. Creates a
/// throwaway probe dir and removes it immediately (no residue).
fn check_worktree_temp(project: Option<&PathBuf>) -> EnvironmentCheck {
    const ID: &str = "worktree_temp";
    const LABEL: &str = "工作树暂存";

    let Some(project) = project else {
        return EnvironmentCheck::warn(
            ID,
            LABEL,
            "尚未选择项目——无法检测工作树暂存目录。".to_string(),
            "请先在门口选择一个 git 项目，然后重新检查。".to_string(),
            None,
        );
    };

    let base = project.join(".quiver").join("worktrees");
    let probe = base.join(".quiver-health-probe");
    match std::fs::create_dir_all(&probe) {
        Ok(()) => {
            // Remove just our probe dir; leave any real worktree staging intact.
            let _ = std::fs::remove_dir(&probe);
            EnvironmentCheck::ok(
                ID,
                LABEL,
                "工作树暂存目录可写。".to_string(),
                Some(base.display().to_string()),
            )
        }
        Err(e) => EnvironmentCheck::fail(
            ID,
            LABEL,
            format!("无法创建工作树暂存目录：{e}"),
            format!(
                "请确认对项目目录有写权限，可手动创建 {} 试试。",
                base.display()
            ),
            Some(base.display().to_string()),
        ),
    }
}

/// Outcome of a bounded subprocess run.
#[derive(Debug)]
struct BoundedOutput {
    status_ok: bool,
    stdout: String,
    stderr: String,
}

/// Run `cmd` with captured stdout/stderr, killing it if it exceeds `timeout`.
///
/// We poll `try_wait` on a short interval rather than pulling in tokio's `time`
/// feature, keeping this probe dependency-free. The only command we ever run
/// here is `claude --version`, which is fast; the timeout is a safety net for a
/// hung binary.
fn run_bounded(cmd: &mut Command, timeout: Duration) -> std::io::Result<BoundedOutput> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()?;

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let output = child.wait_with_output()?;
            return Ok(BoundedOutput {
                status_ok: status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "命令执行超时",
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_probe_finds_git() {
        // git is present in CI and dev; this asserts the reused PATH scan works.
        let check = check_git_found();
        assert_eq!(check.id, "git_found");
        if which_in_path("git").is_some() {
            assert_eq!(check.status, CheckStatus::Ok);
        }
    }

    #[test]
    fn worktree_probe_leaves_no_residue() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().to_path_buf();
        let check = check_worktree_temp(Some(&project));
        assert_eq!(check.status, CheckStatus::Ok);
        // The probe sub-dir must be gone afterwards.
        let probe = project
            .join(".quiver")
            .join("worktrees")
            .join(".quiver-health-probe");
        assert!(!probe.exists(), "health probe dir must be removed");
    }

    #[test]
    fn worktree_probe_warns_without_project() {
        let check = check_worktree_temp(None);
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.remediation.is_some());
    }

    #[test]
    fn subscription_env_ok_when_clean() {
        // Snapshot + clear the forbidden vars for a deterministic clean check.
        let saved: Vec<(&str, Option<std::ffi::OsString>)> = SUBSCRIPTION_ENV_KEYS
            .iter()
            .map(|k| (*k, std::env::var_os(k)))
            .collect();
        for (k, _) in &saved {
            std::env::remove_var(k);
        }
        let check = check_subscription_env();
        for (k, v) in saved {
            if let Some(v) = v {
                std::env::set_var(k, v);
            }
        }
        assert_eq!(check.status, CheckStatus::Ok);
    }

    #[test]
    fn run_bounded_captures_stdout() {
        let out = run_bounded(
            Command::new("echo").arg("hello"),
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(out.status_ok);
        assert_eq!(out.stdout.trim(), "hello");
    }

    #[test]
    fn run_bounded_times_out() {
        let err = run_bounded(
            Command::new("sleep").arg("5"),
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }
}
