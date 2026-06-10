//! 独立审计 · 变异式(DESIGN §8):防"奖励作弊" —— worker 为了让验收通过,可能去**改弱
//! 测试**(删断言、加 `#[ignore]`/`.skip`、把断言改成恒真)而非真把活干对。
//!
//! 本模块是审计的**静态半**:扫 worker 产出的 unified diff,揪出测试削弱信号。**动态半**
//! (在干净克隆里重跑测试、对比通过集,防 worker 在自己 worktree 里动手脚)是后续集成刀。

use std::path::Path;
use std::process::Command;

use anyhow::Context;

use crate::verify::VerifyCommand;

/// §8 动态审计:把 `source_repo` 在 `commit` 处**完整克隆**到干净目录 `dest`(worker 碰不到的
/// 独立副本),在那里跑 `verify`,返回是否通过。在独立副本里重跑测试,挫败 worker 在自己
/// worktree 里对测试动的手脚(源里未提交的改动 / 工作树篡改都不会被 clone 带过来)。
/// `dest` 须不存在(由 `git clone` 创建)。与 [`scan_test_tampering`](静态半)互补。
pub fn clean_clone_verify(
    source_repo: &Path,
    commit: &str,
    verify: &VerifyCommand,
    dest: &Path,
) -> anyhow::Result<bool> {
    let src = source_repo
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("源仓库路径非 UTF-8"))?;
    let dst = dest.to_str().ok_or_else(|| anyhow::anyhow!("目标路径非 UTF-8"))?;
    // 1. 本地完整克隆(独立于源工作树)。§8.2:clone/checkout 都带防御 flags —— 恶意仓库
    //    能在检出瞬间靠 .gitattributes 过滤器/钩子/子模块执行代码,审计副本绝不能中招。
    //    `--no-recurse-submodules` 单独加(子模块是检出代码执行的经典口子)。
    run_git_safe(&["clone", "--quiet", "--no-recurse-submodules", src, dst])?;
    // 2. 检出目标 commit(detached HEAD)。
    run_git_in_safe(dest, &["checkout", "--quiet", commit])?;
    // 3. 在干净副本里跑 verify。
    let (program, args) = verify
        .argv()
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("verify argv 为空"))?;
    let status = Command::new(program)
        .args(args)
        .current_dir(dest)
        .status()
        .with_context(|| format!("跑 verify 失败:{program}"))?;
    Ok(status.success())
}

/// 跑 git,前置 §8.2 防御 flags(见 [`SAFE_GIT_FLAGS`](crate::git::SAFE_GIT_FLAGS))。
fn run_git_safe(args: &[&str]) -> anyhow::Result<()> {
    let full: Vec<&str> = crate::git::SAFE_GIT_FLAGS.iter().copied().chain(args.iter().copied()).collect();
    let out = Command::new("git")
        .args(&full)
        .output()
        .with_context(|| format!("git {args:?} 启动失败"))?;
    if !out.status.success() {
        anyhow::bail!("git {:?} 退出非 0:{}", args, String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

/// 同 [`run_git_safe`] 但带 `current_dir`。
fn run_git_in_safe(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let full: Vec<&str> = crate::git::SAFE_GIT_FLAGS.iter().copied().chain(args.iter().copied()).collect();
    let out = Command::new("git")
        .current_dir(dir)
        .args(&full)
        .output()
        .with_context(|| format!("git -C {} {args:?} 启动失败", dir.display()))?;
    if !out.status.success() {
        anyhow::bail!("git {:?} 退出非 0:{}", args, String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

/// 一次篡改扫描结果。`signals` 非空 = 测试疑似被改弱,审计应拦下 / 降信任 / 转人审。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TamperScan {
    pub signals: Vec<String>,
}

impl TamperScan {
    pub fn is_clean(&self) -> bool {
        self.signals.is_empty()
    }
}

/// 路径像测试文件吗(启发式:含 test/spec,或在 tests/ 目录)。
fn is_test_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("test") || p.contains("spec") || p.contains("/tests/")
}

/// 新增行里的测试跳过标记(多语言)。
const SKIP_MARKERS: &[&str] = &[
    "#[ignore]",
    ".skip(",
    "skip(",
    "xfail",
    "@pytest.mark.skip",
    "@unittest.skip",
    "test.skip",
    "it.skip",
    "describe.skip",
    "t.skip(",
];

/// 扫一段 unified diff,标出"把测试改弱"的信号(§8)。只看测试文件里的增删行:
/// 新增跳过标记 / 新增恒真断言 / 删除断言行。
pub fn scan_test_tampering(diff: &str) -> TamperScan {
    let mut signals = Vec::new();
    let mut file = String::from("?");
    let mut in_test = false;
    let mut removed_asserts = 0u32;

    for line in diff.lines() {
        // 文件头(+++ b/path):切换当前文件 + 是否测试文件。先于单字符 +/- 判断。
        if let Some(rest) = line.strip_prefix("+++ ") {
            let path = rest.trim().trim_start_matches("b/");
            file = path.to_string();
            in_test = is_test_path(path);
            continue;
        }
        if line.starts_with("--- ") || line.starts_with("@@") || line.starts_with("diff ") {
            continue;
        }
        if !in_test {
            continue;
        }
        if let Some(added) = line.strip_prefix('+') {
            let a = added.to_ascii_lowercase();
            if SKIP_MARKERS.iter().any(|m| a.contains(m)) {
                signals.push(format!("{file}: 新增跳过标记 → {}", added.trim()));
            }
            if a.replace(' ', "").contains("assert!(true)") || a.contains("asserttrue(true)") {
                signals.push(format!("{file}: 新增恒真断言 → {}", added.trim()));
            }
        } else if let Some(removed) = line.strip_prefix('-') {
            let r = removed.to_ascii_lowercase();
            if r.contains("assert") || r.contains("expect(") {
                removed_asserts += 1;
            }
        }
    }
    if removed_asserts > 0 {
        signals.push(format!("测试文件删除了 {removed_asserts} 行断言"));
    }
    TamperScan { signals }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_added_skip_marker_in_test_file() {
        let diff = "+++ b/crates/foo/tests/bar_test.rs\n+    #[ignore]\n+    fn t() {}\n";
        let scan = scan_test_tampering(diff);
        assert!(!scan.is_clean());
        assert!(scan.signals[0].contains("跳过标记"));
    }

    #[test]
    fn flags_removed_assertions_and_trivial_assert() {
        let diff = "+++ b/src/foo_test.rs\n-    assert_eq!(x, 42);\n-    assert!(ok);\n+    assert!(true);\n";
        let scan = scan_test_tampering(diff);
        assert!(scan.signals.iter().any(|s| s.contains("删除了 2 行断言")));
        assert!(scan.signals.iter().any(|s| s.contains("恒真断言")));
    }

    #[test]
    fn ignores_non_test_files() {
        // 同样的"删断言"出现在非测试文件 → 不报(那是正常重构)。
        let diff = "+++ b/src/lib.rs\n-    assert_eq!(x, 42);\n+    let y = 1;\n";
        assert!(scan_test_tampering(diff).is_clean());
    }

    #[test]
    fn clean_test_edit_has_no_signals() {
        let diff = "+++ b/tests/api_test.rs\n+    assert_eq!(resp.status, 200);\n";
        assert!(scan_test_tampering(diff).is_clean());
    }

    #[test]
    fn clean_clone_verify_runs_in_isolated_copy() {
        use crate::verify::VerifyCommand;
        use tempfile::TempDir;

        // 造一个有提交的源仓库(committed marker.txt)。
        let src = TempDir::new().unwrap();
        let p = src.path();
        let git = |args: &[&str]| {
            let o = Command::new("git").current_dir(p).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["init", "--quiet"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(p.join("marker.txt"), "ok").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "init"]);

        // 克隆到干净副本跑 verify:marker 在 → 通过。
        let d1 = TempDir::new().unwrap();
        let pass = clean_clone_verify(
            p,
            "HEAD",
            &VerifyCommand::shell("test -f marker.txt"),
            &d1.path().join("clone"),
        )
        .unwrap();
        assert!(pass, "干净副本里 marker 在,verify 通过");

        // verify 失败 → false。
        let d2 = TempDir::new().unwrap();
        let fail = clean_clone_verify(
            p,
            "HEAD",
            &VerifyCommand::shell("test -f nope.txt"),
            &d2.path().join("clone"),
        )
        .unwrap();
        assert!(!fail, "verify 失败返回 false");
    }

    #[test]
    fn malicious_gitattributes_filter_does_not_execute_on_clone() {
        // §8.2:恶意仓库用 .gitattributes 声明一个 smudge 过滤器,过滤器命令写一个"中招"
        // 文件。审计 clean_clone(带 SAFE_GIT_FLAGS:core.attributesFile=/dev/null 等)检出时
        // **绝不能**执行它 —— pwned 文件不该出现。
        use crate::verify::VerifyCommand;
        use tempfile::TempDir;

        let src = TempDir::new().unwrap();
        let p = src.path();
        let pwned = src.path().join("PWNED");
        let git = |args: &[&str]| {
            let o = Command::new("git").current_dir(p).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["init", "--quiet"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        // 仓库内声明: *.txt 走 evil 过滤器;过滤器在 config 里写一个外部命令(touch PWNED)。
        std::fs::write(p.join(".gitattributes"), "*.txt filter=evil\n").unwrap();
        git(&["config", "filter.evil.smudge", &format!("sh -c 'touch {}'; cat", pwned.display())]);
        std::fs::write(p.join("marker.txt"), "ok").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "init"]);

        let dst = TempDir::new().unwrap();
        let _ = clean_clone_verify(p, "HEAD", &VerifyCommand::shell("true"), &dst.path().join("clone"));
        assert!(!pwned.exists(), "smudge 过滤器在审计克隆时被执行了(§8.2 防御失效!)");
    }
}
