//! 沙箱策略(DESIGN §7–8):worker 进程能碰什么 —— **默认拒绝,显式放行**(最小权能)。
//!
//! 本模块是**策略 + 渲染**,纯逻辑可测:[`SandboxPolicy`] 描述放行面,[`to_seatbelt`]
//! 把它渲染成 macOS `sandbox-exec`(seatbelt)profile 字符串。真正用 profile 把 worker 进程
//! 包起来(在克隆/spawn 前就位,§23 P3 "沙箱在多员工放开前必须到位")是后续集成刀。
//! 具体 profile 细则属 §24 待拍板 —— 这里先给可配脚手架 + 收紧默认。
//!
//! [`to_seatbelt`]: SandboxPolicy::to_seatbelt

use std::path::PathBuf;

/// worker 进程的沙箱策略(§7–8)。`allow default` 打底(进程跑得起来、能读系统),再黑名单
/// 收死真正危险的:写(限到可写子树)、网(默认断)、读密钥目录(防外带)。
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// 是否允许出网。默认 `false` —— 大多数 worker 不该联网(防数据外带/投毒回连)。
    pub allow_network: bool,
    /// 是否收紧写:`true` → 禁写全盘、只放行 `writable_paths`;`false` → 不收写(进程能写它
    /// 运行时需要的地方,如 claude 的 ~/.claude;写隔离待实测各进程行为)。
    pub restrict_writes: bool,
    /// 收紧写时可写的子树(通常只有该任务自己的 worktree)。
    pub writable_paths: Vec<PathBuf>,
    /// 可读子树(仓库、只读工具链等)——保留字段,当前 `allow default` 下读默认放行。
    pub readable_paths: Vec<PathBuf>,
    /// **禁读**的子树(§8.3 密钥/凭据目录:~/.ssh、~/.aws、~/.config/gh、~/.netrc、
    /// ~/.agents 等)——allow default 下读默认放行,这些显式 deny 回去,防 worker 外带凭据。
    pub deny_read_paths: Vec<PathBuf>,
}

/// §8.3 必须对 worker 禁读的凭据/密钥目录(绝对路径,基于 `$HOME`)。`~/.agents/resources.json`
/// 是用户的千问 key 所在 —— 整个 `~/.agents` 禁读。
fn secret_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    [".ssh", ".aws", ".config/gh", ".netrc", ".agents", ".config/gcloud", ".kube"]
        .iter()
        .map(|s| home.join(s))
        .collect()
}

impl SandboxPolicy {
    /// 收紧默认(§7):只能写自己的 `worktree`、不出网。读面可再 [`allow_read`] 加。
    /// 用于 **verify 命令**(worker 改过的测试脚本)——它只需写 worktree 内的编译产物。
    ///
    /// [`allow_read`]: SandboxPolicy::allow_read
    pub fn for_worktree(worktree: impl Into<PathBuf>) -> Self {
        let wt = worktree.into();
        // canonicalize:seatbelt 按内核真实路径匹配,/tmp→/private/tmp 等符号链接不归一会让
        // 可写子树对不上、写被误拒(2026-06 实测)。canonicalize 失败(路径还不存在)就用原路径。
        let wt = std::fs::canonicalize(&wt).unwrap_or(wt);
        Self {
            allow_network: false,
            restrict_writes: true,
            readable_paths: vec![wt.clone()],
            writable_paths: vec![wt],
            deny_read_paths: Vec::new(),
        }
    }

    /// **worker(claude)进程**的策略(§8.3):**留网**(claude 要连 Anthropic)、**禁读密钥
    /// 目录**(防外带凭据)。**暂不收紧写** —— claude 要写自己的运行时(~/.claude 等),贸然
    /// 禁写全盘会让它崩;写隔离(allow worktree + allow claude 运行时)待实测各路径后单独收
    /// (沙箱片2b)。所以这一步先拿稳的安全收益:堵住读密钥 + 验证包裹机制不破坏 worker。
    pub fn for_worker(worktree: impl Into<PathBuf>) -> Self {
        let wt = worktree.into();
        let wt = std::fs::canonicalize(&wt).unwrap_or(wt);
        Self {
            allow_network: true,
            restrict_writes: false,
            readable_paths: vec![wt.clone()],
            writable_paths: vec![wt],
            deny_read_paths: secret_dirs(),
        }
    }

    /// 追加一个可读子树(如只读的工具链 / 缓存目录)。
    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.readable_paths.push(path.into());
        self
    }

    /// 追加一个禁读子树(§8.3 凭据黑名单的自定义补充)。
    pub fn deny_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.deny_read_paths.push(path.into());
        self
    }

    /// 放开出网(谨慎:仅在任务确实需要时,如装依赖)。
    pub fn with_network(mut self) -> Self {
        self.allow_network = true;
        self
    }

    /// 当前平台是否支持 sandbox-exec(macOS seatbelt)。其它平台调用方应跳过包裹、原样跑
    /// (或用各自的沙箱方案)。
    pub fn is_supported() -> bool {
        cfg!(target_os = "macos")
    }

    /// 把一条命令包进 sandbox-exec:返回 `(program, args)` =
    /// `sandbox-exec -p <profile> <program> <原 args…>`。调用方拿去 spawn 即在沙箱里跑。
    /// 仅 macOS 有意义(先 [`is_supported`](Self::is_supported))。
    pub fn wrap(&self, program: &str, args: &[String]) -> (String, Vec<String>) {
        let mut wrapped = Vec::with_capacity(args.len() + 3);
        wrapped.push("-p".to_string());
        wrapped.push(self.to_seatbelt());
        wrapped.push(program.to_string());
        wrapped.extend_from_slice(args);
        ("sandbox-exec".to_string(), wrapped)
    }

    /// 渲染成 macOS seatbelt profile 字符串。
    ///
    /// **策略:默认放行,收紧"写"和"网"**(2026-06 实测,§24 定调):纯 `(deny default)`
    /// 白名单在新版 macOS/SIP 下连 dyld/系统库都难放全(连 `/usr/bin/true` 都跑不起来)——
    /// 把每个工具链的依赖列全既不现实又脆。所以反过来:`(allow default)` 让进程能正常执行 +
    /// 读系统,**再黑名单收死真正危险的两件事**:① 禁写全盘、只放行回 worktree 子树(防改
    /// 仓库别处 / home / 密钥);② 默认断网(防数据外带 / 投毒回连,§8.3)。这守住了 §7
    /// 「worker 碰不到自己工作区之外、不出网」的安全本体,又不至于跑不起来。
    ///
    /// 路径须为**真实路径**([`for_worktree`](Self::for_worktree) 已 canonicalize):seatbelt
    /// 按内核真实路径匹配,符号链接不归一会让 allow 子树对不上、写被误拒。
    pub fn to_seatbelt(&self) -> String {
        let mut s = String::from("(version 1)\n(allow default)\n");
        // 写:收紧时先禁全盘、再放行可写子树(seatbelt 末规则优先 → 后 allow 覆盖前 deny)。
        if self.restrict_writes {
            s.push_str("(deny file-write* (subpath \"/\"))\n");
            for p in &self.writable_paths {
                s.push_str(&format!("(allow file-write* (subpath {}))\n", quote(p)));
            }
        }
        // 读:allow default 下默认可读,把密钥/凭据目录显式 deny 回去(§8.3 防外带)。
        for p in &self.deny_read_paths {
            s.push_str(&format!("(deny file-read* (subpath {}))\n", quote(p)));
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
        let dir = std::env::temp_dir();
        let real = std::fs::canonicalize(&dir).unwrap();
        let p = SandboxPolicy::for_worktree(&dir);
        assert!(!p.allow_network, "默认不出网");
        assert_eq!(p.writable_paths, vec![real.clone()], "可写子树 = canonicalize 后的真实路径");
        assert_eq!(p.readable_paths, vec![real]);
    }

    #[test]
    fn seatbelt_allows_default_but_locks_write_and_network() {
        // 用一个一定存在且无符号链接歧义的真实目录(canonicalize 不会改它)。
        let dir = std::env::temp_dir();
        let real = std::fs::canonicalize(&dir).unwrap();
        let sb = SandboxPolicy::for_worktree(&dir).to_seatbelt();
        assert!(sb.contains("(allow default)"), "默认放行(系统库/dyld 能读,进程跑得起来)");
        assert!(sb.contains("(deny file-write* (subpath \"/\"))"), "禁写全盘打底");
        assert!(sb.contains("(deny network*)"), "默认禁网");
        let allow_wt = format!("(allow file-write* (subpath \"{}\"))", real.display());
        assert!(sb.contains(&allow_wt), "只放行写 worktree 子树(真实路径)");
    }

    #[test]
    fn for_worker_keeps_network_but_denies_secret_reads_and_writes() {
        // worker(claude)策略:留网(连 Anthropic)、不收紧写(进程能写运行时)、禁读密钥。
        let p = SandboxPolicy::for_worker(std::env::temp_dir());
        assert!(p.allow_network, "worker 留网");
        assert!(!p.restrict_writes, "worker 暂不收紧写(避免破坏 claude 运行时)");
        let sb = p.to_seatbelt();
        assert!(sb.contains("(allow default)"));
        assert!(sb.contains("(allow network*)"), "网放开");
        assert!(!sb.contains("(deny file-write* (subpath \"/\"))"), "不收紧写");
        // §8.3:HOME 下的密钥目录被显式禁读(有 HOME 时)。
        if let Some(home) = std::env::var_os("HOME") {
            let ssh = format!("(deny file-read* (subpath \"{}/.ssh\"))", PathBuf::from(&home).display());
            assert!(sb.contains(&ssh), "禁读 ~/.ssh");
            assert!(sb.contains(".agents"), "禁读 ~/.agents(千问 key)");
        }
    }

    #[test]
    fn every_secret_dir_is_denied_read_in_worker_profile() {
        // §8.3 安全回归网:secret_dirs 里**每一个**密钥目录都必须进禁读 profile —— 防有人
        // 误删/漏掉某条(如 .aws / .kube / .config/gcloud),让 real worker 读到用户凭据。
        if std::env::var_os("HOME").is_none() {
            return; // 无 HOME(CI 沙箱)→ for_worker 不加密钥禁读,跳过
        }
        let sb = SandboxPolicy::for_worker(std::env::temp_dir()).to_seatbelt();
        for dir in secret_dirs() {
            let entry = format!("(deny file-read* (subpath \"{}\"))", dir.display());
            assert!(sb.contains(&entry), "密钥目录 {dir:?} 必须禁读,但不在 seatbelt profile 里");
        }
    }

    #[test]
    fn network_is_opt_in() {
        let sb = SandboxPolicy::for_worktree(std::env::temp_dir())
            .with_network()
            .to_seatbelt();
        assert!(sb.contains("(allow network*)"), "显式放开出网");
        assert!(!sb.contains("(deny network*)"));
    }

    #[test]
    fn wrap_builds_sandbox_exec_invocation() {
        let (prog, args) = SandboxPolicy::for_worktree(std::env::temp_dir())
            .wrap("claude", &["--foo".to_string(), "bar".to_string()]);
        assert_eq!(prog, "sandbox-exec");
        assert_eq!(args[0], "-p");
        assert!(args[1].contains("(allow default)"), "第二个参数是 profile");
        assert_eq!(&args[2..], &["claude", "--foo", "bar"], "profile 后接原命令");
    }

    #[test]
    fn supported_on_macos() {
        assert_eq!(SandboxPolicy::is_supported(), cfg!(target_os = "macos"));
    }

    #[test]
    fn quote_escapes_quotes_and_backslashes() {
        let q = quote(&PathBuf::from(r#"/a/b"c\d"#));
        assert_eq!(q, r#""/a/b\"c\\d""#);
    }
}
