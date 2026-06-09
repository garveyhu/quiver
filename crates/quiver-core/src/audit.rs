//! 独立审计 · 变异式(DESIGN §8):防"奖励作弊" —— worker 为了让验收通过,可能去**改弱
//! 测试**(删断言、加 `#[ignore]`/`.skip`、把断言改成恒真)而非真把活干对。
//!
//! 本模块是审计的**静态半**:扫 worker 产出的 unified diff,揪出测试削弱信号。**动态半**
//! (在干净克隆里重跑测试、对比通过集,防 worker 在自己 worktree 里动手脚)是后续集成刀。

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
}
