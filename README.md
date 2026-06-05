# 🏹 Quiver

> 一个 cozy 像素风的 macOS 桌面应用：把一队无人值守的 headless 编码 agent 当工人，
> 派给它们任务、看着像素小弓箭手替你干活、醒来收获合并好的成果。
>
> 一个任务 = 一个 git worktree = 一个 agent 进程 = 一个工位上的小弓箭手。

技术栈：Tauri v2(Rust)+ React/TypeScript + Phaser + SQLite。

---

## 开发

```bash
cargo tauri dev          # 编译并启动开发窗口（自动起前端 dev server）
cargo build -p quiver-app --bin Quiver   # 只编译 Rust 二进制
yarn --cwd frontend dev  # 只起前端 dev server (:1420)
cargo test --workspace   # 跑 Rust 测试
```

## 打包

```bash
cargo tauri build                              # 当前架构
cargo tauri build --target universal-apple-darwin   # Intel + Apple Silicon 通用包
```

产物在 `target/release/bundle/`（cargo workspace 的 target 在仓库根，不在 `src-tauri/` 下）：
- `dmg/Quiver_<版本>_<架构>.dmg`
- `macos/Quiver.app`

---

## 首次打开（重要）

当前发布包是 **ad-hoc 签名、未经 Apple 公证**。macOS 会对从网络/AirDrop 传来的
此类应用加隔离标记，**第一次打开需要手动放行一次**（之后正常双击即可）：

**方法一 · 右键打开**
1. 在 Finder 里**右键点 `Quiver.app` → 打开**；
2. 弹窗里再点一次「打开」。

**方法二 · 终端清隔离标记**（右键无效时用）
```bash
xattr -cr /Applications/Quiver.app
```

> 出现「"Quiver" 已损坏，应移到废纸篓」不是文件损坏，而是 Gatekeeper 拦未公证应用。
> 用上面任一方法即可打开。要做到双击零提示，需用 Apple Developer ID 证书签名并公证发布。

---

## License

MIT
