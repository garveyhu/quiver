//! agent 二进制解析(§12):按设置覆盖 → 按模式自动解析。Simulate 找免费的 `fake-claude`
//! 测试替身,Real 找官方 `claude`。环境预检([`crate::environment`])复用 `resolve_real_claude` /
//! `which_in_path`,保证"检查通过"等价于"启动找得到"。

use std::path::{Path, PathBuf};

use quiver_store::Settings;

use super::RunMode;

/// Resolve the agent binary honoring the saved `agent_bin_override` (Phase B)
/// before the per-mode auto-resolution.
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
