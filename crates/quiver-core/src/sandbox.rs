//! 沙箱策略(DESIGN §7–8):worker 进程能碰什么 —— **默认拒绝,显式放行**(最小权能)。
//!
//! 本模块是**策略 + 渲染**,纯逻辑可测:[`SandboxPolicy`] 描述放行面,[`to_seatbelt`]
//! 把它渲染成 macOS `sandbox-exec`(seatbelt)profile 字符串。真正用 profile 把 worker 进程
//! 包起来(在克隆/spawn 前就位,§23 P3 "沙箱在多员工放开前必须到位")是后续集成刀。
//! 具体 profile 细则属 §24 待拍板 —— 这里先给可配脚手架 + 收紧默认。
//!
//! [`to_seatbelt`]: SandboxPolicy::to_seatbelt

use std::path::PathBuf;

/// worker 进程的沙箱策略。默认拒绝一切,只放行这里列出的(§7 最小权能 / Rule of Two)。
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// 是否允许出网。默认 `false` —— 大多数 worker 不该联网(防数据外带/投毒回连)。
    pub allow_network: bool,
    /// 可写子树(通常只有该任务自己的 worktree —— 写不到仓库别处、写不到 home)。
    pub writable_paths: Vec<PathBuf>,
    /// 可读子树(仓库、只读工具链等)。
    pub readable_paths: Vec<PathBuf>,
}

impl SandboxPolicy {
    /// 收紧默认(§7):只能写自己的 `worktree`、能读它、不出网。读面可再 [`allow_read`] 加。
    ///
    /// [`allow_read`]: SandboxPolicy::allow_read
    pub fn for_worktree(worktree: impl Into<PathBuf>) -> Self {
        let wt = worktree.into();
        Self {
            allow_network: false,
            readable_paths: vec![wt.clone()],
            writable_paths: vec![wt],
        }
    }

    /// 追加一个可读子树(如只读的工具链 / 缓存目录)。
    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.readable_paths.push(path.into());
        self
    }

    /// 放开出网(谨慎:仅在任务确实需要时,如装依赖)。
    pub fn with_network(mut self) -> Self {
        self.allow_network = true;
        self
    }

    /// 渲染成 macOS seatbelt(`sandbox-exec -p <profile>`)profile 字符串。
    /// `(deny default)` 打底,逐项放行;路径用 `subpath` 限定到子树。
    pub fn to_seatbelt(&self) -> String {
        let mut s = String::from("(version 1)\n(deny default)\n");
        // 进程基本运转所需(执行自身、读动态库、用临时区)——收紧但不至于跑不起来。
        s.push_str("(allow process-exec)\n(allow process-fork)\n(allow sysctl-read)\n");
        s.push_str("(allow file-read-metadata)\n");
        for p in &self.readable_paths {
            s.push_str(&format!("(allow file-read* (subpath {}))\n", quote(p)));
        }
        for p in &self.writable_paths {
            s.push_str(&format!("(allow file-write* (subpath {}))\n", quote(p)));
        }
        if self.allow_network {
            s.push_str("(allow network*)\n");
        } else {
            s.push_str("(deny network*)\n");
        }
        s
    }
}

/// seatbelt 字面量路径:双引号包裹,转义内部反斜杠和引号。
fn quote(p: &PathBuf) -> String {
    let raw = p.to_string_lossy();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_worktree_is_locked_down_by_default() {
        let p = SandboxPolicy::for_worktree("/tmp/wt");
        assert!(!p.allow_network, "默认不出网");
        assert_eq!(p.writable_paths, vec![PathBuf::from("/tmp/wt")]);
        assert_eq!(p.readable_paths, vec![PathBuf::from("/tmp/wt")]);
    }

    #[test]
    fn seatbelt_denies_by_default_and_scopes_paths() {
        let sb = SandboxPolicy::for_worktree("/tmp/wt").to_seatbelt();
        assert!(sb.contains("(deny default)"), "默认拒绝打底");
        assert!(sb.contains("(deny network*)"), "默认禁网");
        assert!(
            sb.contains("(allow file-write* (subpath \"/tmp/wt\"))"),
            "只放行写 worktree 子树"
        );
        assert!(sb.contains("(allow file-read* (subpath \"/tmp/wt\"))"));
    }

    #[test]
    fn network_and_extra_read_are_opt_in() {
        let sb = SandboxPolicy::for_worktree("/tmp/wt")
            .allow_read("/opt/toolchain")
            .with_network()
            .to_seatbelt();
        assert!(sb.contains("(allow network*)"), "显式放开出网");
        assert!(!sb.contains("(deny network*)"));
        assert!(sb.contains("(subpath \"/opt/toolchain\")"), "追加只读子树");
    }

    #[test]
    fn quote_escapes_quotes_and_backslashes() {
        let q = quote(&PathBuf::from(r#"/a/b"c\d"#));
        assert_eq!(q, r#""/a/b\"c\\d""#);
    }
}
