# Rust / Cargo / Tauri 速查表（按本项目实例）

> 配套 `docs/rust-tour.md`。这里按主题速查，每个点尽量配一个**本仓库里的真实例子**，
> 读代码卡住时回来对一下。不是 Rust 教程的替代，是"我在 Quiver 里看到的这个符号是啥"的字典。

---

## 1. Cargo 命令速查

| 命令 | 干什么 | 何时用 |
|------|--------|--------|
| `cargo check` | 只做类型检查，不生成可执行文件 | 最快的"语法/类型对不对"反馈循环，改 Rust 后先跑它 |
| `cargo build` | 编译（debug 版，带调试信息、不优化） | 要产物时；`fake-claude` 也会被一并编出来 |
| `cargo build --release` | 编译优化版 | 发布；注意 release 不含 `#[cfg(debug_assertions)]` 的代码 |
| `cargo test` | 跑所有测试 | 改了逻辑后 |
| `cargo test -p quiver-core` | 只测某个 crate（`-p` = package） | 想聚焦一个 crate |
| `cargo test 名字片段` | 只跑名字匹配的测试 | 调单个用例，如 `cargo test keep_branch` |
| `cargo run -p quiver-core --example run_once` | 跑某 crate 的 example | 看不开 GUI 的最小示例 |
| `cargo tauri dev` | 编译 + 启动桌面应用（先拉前端 dev server） | 跑整个 app |
| `cargo build -p quiver-app --bin Quiver` | 只编 Rust 二进制，不碰前端 | 只想验证 Rust 侧 |
| `cargo fmt` | 自动格式化 | 提交前 |
| `cargo clippy` | 进阶 lint，给改进建议 | 想写得更地道 |
| `cargo tree` | 打印依赖树 | 排查"这个库谁引进来的" |
| `cargo add 包名` | 往当前 crate 加依赖 | 比手改 Cargo.toml 省事 |

> **debug vs release 的坑**：本项目的 dev-bridge（`src-tauri/src/dev_bridge.rs`）整个被
> `#[cfg(debug_assertions)]` 包住，**只在 debug 构建里存在**。所以"调试能用、发布没了"是预期行为。

---

## 2. 项目结构概念

| 词 | 含义 | 本项目 |
|----|------|--------|
| **crate** | 一个编译单元（一个库或一个可执行程序） | `quiver-core` / `quiver-store` / `fake-claude` / `quiver-app` |
| **workspace** | 一组共享依赖锁的 crate 集合 | 根 `Cargo.toml` 的 `[workspace] members` |
| **package** | `Cargo.toml` 描述的一个包（可含 1 个 lib + 多个 bin） | 每个 crate 目录 |
| **`mod`** | crate 内部的命名空间（模块） | `quiver-core/src/lib.rs` 的 `pub mod event;` 等 |
| **`pub`** | 对外可见（不写就是私有，只本模块可见） | `pub fn run()` / `pub struct TaskSpec` |
| **`use`** | 把路径引入当前作用域，省得每次写全名 | `use quiver_core::git::GitGuard;` |
| **`lib.rs`** | 库 crate 的根模块 | `quiver-core/src/lib.rs` |
| **`main.rs`** | 二进制 crate 的入口（`fn main`） | `src-tauri/src/main.rs` |
| **`[lib] name=...`** | 给库起的导入名 | `quiver_app_lib`（注意连字符变下划线） |
| **路径依赖** | 依赖同仓库的兄弟 crate | `quiver-core = { path = "../crates/quiver-core" }` |
| **git 依赖** | 从 git 仓库拉的依赖 | `fix-path-env = { git = "..." }` |
| **`examples/`** | `cargo run --example` 能跑的示范程序 | `quiver-core/examples/run_once.rs` |
| **`tests/`** | 集成测试（独立编译，黑盒测公开 API） | `quiver-core/tests/*.rs` |

> **连字符 vs 下划线**：crate 名常写连字符 `quiver-core`，但代码里 `use` 时变下划线
> `quiver_core`——Rust 标识符不能有连字符，Cargo 自动转换。

---

## 3. 所有权 / 借用（Rust 最独特的部分）

核心三条规则：
1. 每个值有且只有一个**所有者**；所有者离开作用域，值被销毁（`drop`）。
2. 可以把值**借**给别人用：`&T`（共享/只读借用，可同时多个）或 `&mut T`（可变借用，同时只能一个）。
3. 借用不能比被借的值活得久（编译器靠"生命周期"保证）。

| 写法 | 含义 | 本项目例子 |
|------|------|------------|
| `&state` | 只读借用，看完还回去 | `current_project(&state)`（`lib.rs`） |
| `&mut command` | 可变借用，能改对方 | `apply_env_allowlist(&mut command)`（`runner/claude/mod.rs`） |
| `x.clone()` | 复制一份新的所有权 | `app.clone()` / `store.clone()`（要交给后台任务时） |
| `move`（闭包/async） | 把捕获的变量所有权搬进去 | `tokio::spawn(async move { ... })`（`scheduler.rs`） |
| `drop(x)` | 立刻销毁、释放资源 | `drop(permit)`（队列空时还令牌） |

**什么时候 clone、什么时候借？** 经验法则：
- 只是"看一眼、调个方法"→ 借 `&`。
- 对方要**活得比当前函数久**（典型：扔进 `tokio::spawn` 的后台任务）→ 借不了，得 `clone` 一份所有权进去。
- `Arc<T>` 和 `AppHandle` 的 `clone` 很便宜（只是引用计数 +1），所以为了跨任务共享而 clone 它们是地道做法，别有心理负担。

---

## 4. 错误处理：`Result` / `Option` / `?`

Rust **没有异常**。"可能失败"和"可能为空"都用类型表达，强制你处理。

| 类型 | 两种取值 | 语义 |
|------|----------|------|
| `Result<T, E>` | `Ok(T)` / `Err(E)` | 操作可能**失败** |
| `Option<T>` | `Some(T)` / `None` | 值可能**不存在**（Rust 没有 null） |

**`?` 运算符**（最常用）：跟在 `Result`/`Option` 后面——成功就解包继续，失败就把 `Err`/`None`
当作整个函数的返回值提前弹出。
```rust
let store = state.store()?;        // 拿到就继续；Err 就直接 return 这个 Err
let task = store.get_task(&id)?;   // 一连串 ? 把错误处理压扁
```

**常见配套方法**：

| 方法 | 作用 | 本项目例子 |
|------|------|------------|
| `.map_err(\|e\| ...)` | 把 `Err` 里的错误**换类型/换措辞** | `.map_err(\|e\| format!("{e:#}"))`：库错误→中文串 |
| `.ok_or_else(\|\| ...)` | 把 `Option` 的 `None` 变成 `Result` 的 `Err` | `store.get_task(&id)?.ok_or_else(\|\| "...".into())` |
| `.unwrap_or(默认值)` | 失败/空时给个默认 | `.unwrap_or(1)`（`enqueue_task_cmd` 里 max_workers 兜底） |
| `.expect("msg")` | 失败就带 msg panic（崩） | `builder.run(...).expect("error while running Quiver")`（最外层可接受） |
| `if let Some(x) = opt` | 只处理"有值"那支 | `if let Some(store) = store { ... }` |
| `let Some(x) = ... else { return }` | 没值就提前退出 | `let Some(folder) = chosen else { return Ok(None) };`（`pick_project`） |

**两种错误风格在本项目里的分工**：
- **`anyhow::Result<T>`**（core 内部）：万能错误类型，配 `?` 极顺手。见 `supervisor.rs`、`run.rs` 的内部函数。
- **`Result<T, String>`**（Tauri 命令层）：错误就是给前端看的**中文串**。见所有 `#[tauri::command]`。
- 用 `.map_err(|e| format!("{e:#}"))` 在边界上把前者转成后者。`{e:#}` 是"打印完整错误链"的格式。

---

## 5. 枚举 `enum` + 模式匹配 `match`

Rust 的枚举每个变体可以带数据，配 `match` **穷尽**处理（漏一支编译不过——这是特性不是麻烦）。

```rust
// 简单枚举（run.rs）
enum RunMode { Simulate, Real }
match mode {
    RunMode::Simulate => resolve_fake_claude(),
    RunMode::Real     => resolve_real_claude(),
}

// 带数据的枚举（event.rs）——每个变体携带不同字段
enum AgentEventPayload {
    WorkerStarted { model: Option<String>, auth_mode: AuthMode },
    ToolUse { tool: String, summary: String },
    Result { ok: bool, cost_usd: Option<f64>, num_turns: u32 },
    Error { code: String, message: String },
    // ...
}
```

| 写法 | 含义 |
|------|------|
| `match x { A => ..., B => ... }` | 穷尽分支；每支必须覆盖或用 `_ => ...` 兜底 |
| `if let Pattern = x { ... }` | 只关心一种模式时的简写 |
| `Foo::Bar { field, .. }` | 解构出 `field`，`..` 表示其余字段忽略 |
| `_` | 通配符："其它情况"/"我不关心的字段" |

本项目的枚举地图：`RunMode`（run.rs）、`FinishStatus`/`FailureReason`/`Cleanup`（supervisor.rs）、
`AgentEventPayload`/`RunnerKind`/`AuthMode`（event.rs）。读它们是理解状态机的最快路径。

---

## 6. 结构体 `struct` + `#[derive(...)]`

```rust
#[derive(Default)]                 // 自动生成"全空"的默认值
struct AppState {
    project_path: Mutex<Option<PathBuf>>,
    scheduler: Scheduler,
}
```

**`#[derive(...)]` 是"让编译器自动实现某些 trait"**。本项目高频出现的：

| derive | 给你什么能力 | 例子 |
|--------|--------------|------|
| `Default` | `T::default()` 造默认值 | `AppState::default()` |
| `Clone` | `.clone()` 复制 | 到处 |
| `Copy` | 小类型按位复制，不操心所有权 | `RunMode`（`#[derive(Clone, Copy)]`） |
| `Debug` | `{:?}` 格式化打印（调试用） | 几乎每个类型 |
| `PartialEq, Eq` | 能用 `==` 比较 | `RunMode`、`FinishStatus`（测试里 `assert_eq!`） |
| `Serialize` | 能被 serde 转成 JSON（发给前端） | `AgentEvent`、`FinishedEvent` |
| `Deserialize` | 能从 JSON 还原（接前端参数） | `RunMode` |

构造与返回：
```rust
let task = TaskSpec { id: task_id, prompt };   // 构造
// 函数最后一个表达式、不带分号、不写 return → 就是返回值
Ok(RunOutcome { task_id, status, events, cleanup, failure, branch })
```
> `prompt`（不写 `prompt: prompt`）是**字段简写**：当变量名和字段名相同可省略。

---

## 7. trait（接口 / 特质）

trait = 一组方法的契约。任何类型 `impl Trait for 类型` 实现后即可当该 trait 用。

```rust
// 定义（runner/mod.rs）
#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn spawn(&self, prompt: &str, cwd: &Path, bin: &Path) -> Result<Receiver<AgentEvent>>;
    fn kind(&self) -> RunnerKind;
}

// 实现（runner/claude/mod.rs）
#[async_trait]
impl AgentRunner for ClaudeRunner {
    async fn spawn(&self, ...) -> Result<...> { /* 启动子进程 */ }
    fn kind(&self) -> RunnerKind { RunnerKind::ClaudeCli }
}
```

| 记号 | 含义 |
|------|------|
| `trait Foo { ... }` | 定义契约 |
| `impl Foo for Bar` | 让 `Bar` 实现 `Foo` |
| `impl Bar { ... }` | 给 `Bar` 加固有方法（不关 trait），如 `impl RunMode { fn label() }` |
| `trait Foo: Send + Sync` | 约束：实现者必须也满足 Send/Sync（线程安全） |
| `#[async_trait]` | 生态宏，让 trait 里能写 `async fn`（原生暂不支持） |
| `impl FnMut(&AgentEvent) + Send` | "接受任意满足此约束的闭包"作参数（见 `run_task_streaming`） |
| `impl Into<String>` | "接受任何能转成 String 的东西"（见 `ClaudeRunner::new`） |

**本项目里 trait 的两个用法**：
1. **可替换后端**：`AgentRunner` 是"唯一 backend-specific 表面"。要加 Codex 只需再写一个 impl。
2. **扩展 trait（给别人的类型加方法）**：`lib.rs:291-300` 的 `EmitTaskUpdate` 给 `AppHandle`
   挂了个 `emit_task_update()`，纯粹为了少写一遍 channel 名——这是 Rust 常见的"extension trait"技巧。

---

## 8. 异步 `async` / `.await` + Tokio

| 概念 | 一句话 | 本项目 |
|------|--------|--------|
| `async fn` | 这个函数返回一个"待办"（Future），调用它不会立刻执行 | 几乎所有 IO 函数 |
| `.await` | "在这儿等它完成，等的时候让出线程干别的" | `runner.spawn(...).await?` |
| `tokio::spawn(async move {...})` | 起一个**独立后台任务**，立刻返回，不等它 | 调度器派工 / runner 读 stdout |
| `tokio::runtime` | 驱动这些 Future 跑的执行器 | Tauri 已内置；`Cargo.toml` 显式声明了 tokio |
| `Send` | 类型能安全地在线程间转移 | 闭包/runner 都要求 `Send` 才能 spawn |

**为什么这个项目离不开 async**：它要同时（a）跑 N 个 agent 子进程，（b）逐行读它们的输出，
（c）保持 GUI 响应。async + `tokio::spawn` 让这些"等 IO"的活互不阻塞地交织进行。

---

## 9. 并发原语（共享状态怎么不出错）

| 工具 | 作用 | 本项目 |
|------|------|--------|
| `Arc<T>` | 多所有者共享一个值（引用计数） | `Arc<Store>`、`Arc<GitGuard>`、`Arc<Semaphore>` |
| `std::sync::Mutex<T>` | 互斥锁；`.lock()` **阻塞线程**取锁 | `AppState.project_path`（短临界区，同步上下文） |
| `tokio::sync::Mutex<T>` | 异步互斥锁；`.lock().await` **不阻塞线程** | `Scheduler.projects`（要跨 `.await` 持锁） |
| `OnceLock<T>` | 只能写一次的格子 | `AppState.store`（启动装一次，之后只读） |
| `Semaphore` | 令牌池，限制"最多同时 N 个" | `ProjectQueue.permits`（= maxWorkers） |
| `mpsc::channel` | 多生产者单消费者通道，跨任务传数据 | runner → supervisor 的事件流 |

**口诀**：
- 想**共享同一个值**给多个并发任务 → `Arc<那个值>`，每个任务拿一份 clone。
- 共享的值还要**被改** → `Arc<Mutex<值>>`。
- 同步代码/极短临界区用 `std::sync::Mutex`；要在持锁期间 `.await` 用 `tokio::sync::Mutex`（用错会死锁/编译错）。
- 限并发数 → `Semaphore`。
- 跨任务**传数据流** → `mpsc` 通道；**发送端 drop = 接收端收到 EOF（`recv()` 返回 `None`）**。

---

## 10. serde（JSON ↔ Rust，前后端的桥）

前端传来的参数、推给前端的事件，都靠 serde 在 JSON 和 Rust 类型间转换。看 `event.rs` 的标注：

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]            // Rust 的 task_id → JSON 的 taskId
pub struct AgentEvent {
    pub task_id: String,
    pub runner: RunnerKind,
    #[serde(flatten)]                          // 把 payload 的字段"摊平"到外层，不嵌套
    pub payload: AgentEventPayload,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AgentEventPayload {                    // tag="kind"：JSON 里加个 "kind" 字段标明是哪一支
    WorkerStarted { model: Option<String>, auth_mode: AuthMode },
    // → {"kind":"worker_started","model":"sonnet","authMode":"subscription"}
}
```

| 属性 | 作用 |
|------|------|
| `#[derive(Serialize)]` | 能转成 JSON（发给前端） |
| `#[derive(Deserialize)]` | 能从 JSON 还原（接前端参数） |
| `rename_all = "camelCase"` | Rust 习惯 snake_case，JS 习惯 camelCase，自动互转 |
| `#[serde(flatten)]` | 把内嵌结构的字段提到外层，少一层嵌套 |
| `tag = "kind"` | 枚举序列化时加一个判别字段，前端靠它分辨事件类型 |
| `rename_all_fields = "camelCase"` | 枚举各变体内部字段也转 camelCase |

> 前端 `useSupervisor` 收到的 `agent-event`，就是这套标注生成的 JSON。想知道前端会收到什么形状，
> 直接读 `event.rs` 的标注 + `event.rs:43-59` 的那个序列化测试最准。

---

## 11. Tauri 专属速查

| 概念 | 作用 | 本项目 |
|------|------|--------|
| `#[tauri::command]` | 标记一个可被前端 `invoke` 的函数 | `lib.rs` 里 11 个命令 |
| `tauri::generate_handler![...]` | 登记命令路由表（**漏登记 = 前端调不到**） | `lib.rs:335-362` |
| `tauri::Builder` | 构建器，链式配置 app | `run()` |
| `.manage(x)` | 注册全局共享状态 | `.manage(AppState::default())` |
| `State<'_, T>` | 命令参数里注入共享状态 | `state: State<'_, AppState>` |
| `AppHandle` | 应用句柄，可 emit 事件、访问路径等 | 命令/工人都持有它 |
| `app.emit("事件名", payload)` | 后端 → 前端**推送** | `app.emit("agent-event", event)` |
| 前端 `listen("事件名", cb)` | 前端订阅推送 | `useSupervisor` 订 `agent-event` |
| 前端 `invoke("命令名", {args})` | 前端 → 后端**请求** | `invoke("enqueue_task_cmd", {...})` |
| `.setup(\|app\| {...})` | 启动时的初始化钩子 | 打开 SQLite、起 dev-bridge |
| `#[cfg(debug_assertions)]` | 只在 debug 构建编译 | dev-bridge 整块 |
| 插件 `.plugin(...)` | 挂 Tauri 插件 | `tauri_plugin_dialog`（选文件夹对话框） |

**Tauri 心智模型**：前端（WebView，JS 进程）和后端（Rust 进程）是两个世界，**只能靠
`invoke`（请求）和 `emit`/`listen`（推送）通信**，传的都是 JSON（serde 负责翻译）。
本项目额外约定：所有 IPC 调用都封在前端 hook 里，Phaser 场景通过 EventBus 间接触发，从不直接 invoke。

---

## 12. 宏速查（带 `!` 的那些）

宏在编译期展开成代码，名字后带 `!`。本项目常见：

| 宏 | 作用 |
|----|------|
| `println!` / `eprintln!` | 打印到 stdout / stderr |
| `format!("{x}")` | 拼字符串（不打印，返回 String） |
| `vec![a, b, c]` | 造 `Vec` |
| `assert_eq!(a, b)` | 测试断言：不相等就 panic |
| `anyhow::bail!("...")` | 立刻 `return Err(anyhow!(...))` |
| `anyhow::anyhow!("...")` | 造一个 anyhow 错误值 |
| `tauri::generate_handler![...]` | 生成命令路由表 |
| `tauri::generate_context!()` | 编译期把 `tauri.conf.json` 等打包进来 |

> 格式串里的 `{e:#}`、`{x}`：`{x}` 直接内插变量；`:#` 是"美化/完整"修饰符（对 anyhow 错误会打印整条错误链）。

---

## 13. 卡住时的自检清单

- **"找不到命令" / 前端 invoke 报错** → 检查该函数是否加了 `#[tauri::command]` **且**登记进了
  `generate_handler![...]`（两个 cfg 分支都要）。
- **借用/生命周期编译错（"borrowed value does not live long enough"）** → 通常是想把一个引用扔进
  `tokio::spawn`。改成 `clone()` 一份所有权 `move` 进去。
- **"future is not Send"** → spawn 的异步块里用了非线程安全的东西，或跨 `.await` 持有了
  `std::sync::Mutex` 的锁。换 `tokio::sync::Mutex`，或缩小锁的作用域。
- **死锁/卡住** → 大概率在持锁期间 `.await` 了。把锁的作用域用 `{ }` 收紧，拿到数据就放锁再 await。
- **改了 Rust 没生效** → 确认不是只改了 `#[cfg(debug_assertions)]` 之外的 release 路径；`cargo tauri dev` 是 debug。
- **想快速验证语法对不对** → `cargo check`，比 `build` 快得多。

---

需要更深入某一块（比如想专门把"异步 + 并发"或"git worktree 子系统"单独讲透），
直接说，我可以再补一份专题文档。
</content>
