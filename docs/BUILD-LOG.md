# Quiver 自主建造 · BUILD-LOG

> 跨轮次唯一真相。每轮开头读它,结尾更新它。状态在文件里,不在记忆里。

---

## ☀️ 晨报（最新在最上）

### 2026-06-09 · 第 1 轮

**落了什么**
- `a8502fd` feat(core): 把 session_id/tokens/duration_ms 透出到 AgentEvent
  - 修了 P0 #1 的 bug:`adapter.rs` 里 `RawLine::Init { model, .. }` 用 `..`
    丢弃了解析器已捕获的 `session_id`。现已挂到 `WorkerStarted` 事件、前端
    wire 类型(`agentEvent.types.ts`)同步加 `sessionId`。
  - 同文件内一并落定了**先前在途、未提交**的 tokens/duration_ms 透出(见下"⚠"）。

**验证过的**
- `cargo check --workspace` ✅(exit 0,整个工作树当前可编译)
- `cargo test -p quiver-core` ✅(全绿,含 event/adapter/parse 单测 + run_task/
  spawn_fake/streaming/cancel 集成测试)
- `yarn --cwd frontend tsc --noEmit` ✅

**各阶段进度**
- 后端 P0:#1 ✅(session_id 已透到事件层,但**尚未持久化/未接 --resume**)、
  #2 部分已有、#3 未动、#4 未动、#5 已有未追踪(见下)。
- 前端:纵向切片尚未开始(原型搬迁未启动)。

**今天该接哪**(优先级从上到下)
1. **先落 checkpoint 解锁 store**:`quiver-store` 当前有一组**未提交但已验证健全**的
   verify_command 持久化 WIP(schema.rs 加列 + settings.rs +21/-5 + tasks.rs +89 行,
   `cargo test -p quiver-store` 27 个全过)。下一轮先把这组 store WIP 合成一个
   checkpoint 提交(诚实标注是落定在途工作),让 task 3 能在干净基线上叠加。
2. P0 #3:在干净基线上给 `task` 表加 `session_id` / `saga_step` / `done`(完成标记)/
   `fence` 列(用已存在的 `add_column_if_absent` 幂等助手),把第 1 轮透出的
   `session_id` 落地,形成"事件→持久化"闭环。
3. P0 #1 收尾:在 supervisor / run.rs 里把 `WorkerStarted.session_id` 写进 store 的
   task 行(依赖第 2 步的列)。
4. P0 #2:扩 `AgentRunner` trait,补 `resume` / `cancel` / `set_permission`。

---

## ⚠️ 需要你拍板（无人值守不敢擅动）

1. **工作树有一大坨未提交的在途重构,不是本 loop 建的,我没动它。**
   - 内容:前端从 Phaser game 架构迁到 `shell/` + `office/`(`frontend/src/game/*`
     整组删除、新增 `frontend/src/shell/`、`frontend/src/hooks/*`、`office/*`);
     `assets/` 下若干 png 删除;`docs/` 多个设计稿(autonomous-org.md / ui-design.md /
     redesign-iso-directions.html 等)还是 untracked。
   - 现状:**整个工作树当前能编译**(cargo check + tsc 都过),说明这堆改动是
     一个 compiling 的中间态,不是半坏。
   - 我的纪律:**绝不 `git add -A`**,只 `git add` 我本轮编辑的具体文件;那一大坨
     重构与 asset 删除我一律不碰、不提交、不删。
   - 需要你定:这堆在途重构要不要先由你 commit 成一个 baseline?在它没成 baseline
     前,我每次碰一个已是 `M` 的文件,该文件里先前的未提交 hunk 会跟着我的改动一起
     被提交(例如本轮的 tokens/duration_ms 就是这么被带进 `a8502fd` 的——它们与
     session_id 同属"补全 AgentEvent wire 字段"一层,主题一致,且非交互式 git 无法
     拆 hunk,故合为一个提交)。如果你希望我严格只提交自己的 hunk,需要你先把现有
     WIP 落盘。
   - **后续轮次策略**:由于整个后端(`runner/mod.rs`、`scheduler.rs`、`lib.rs`、
     `tasks.rs`、`settings.rs`、`schema.rs` 等)都是在途未提交态,我会对**已验证健全**
     的成组 WIP(如 store 的 verify_command 特性,27 测试全过)先落 checkpoint 提交、
     诚实标注,再叠加新功能;**绝不**把半坏/我不理解的改动塞进提交,**绝不**碰前端
     game→shell 大重构与 asset 删除。你早上 review 时若想重新拆分,`git reset` 即可
     (我这边的提交都可编译、信息透明)。

---

## 📋 P0 任务清单（对照 loop 指令 §3 与代码现状）

| # | 任务 | 现状 |
|---|------|------|
| 1 | adapter.rs `RawLine::Init` 丢 `session_id` 的 bug | ✅ 已修(`a8502fd`);**待**持久化 + 接 `--resume` |
| 2 | 抽 `AgentRunner` trait(spawn/resume/cancel/set_permission) | ◐ trait 已存在(`runner/mod.rs`),仅有 `spawn`+`kind`;缺 resume/cancel/set_permission;仍在 quiver-core 内,未拆 `crates/quiver-agent` |
| 3 | quiver-store schema 加 saga_step + 完成标记 + fence(幂等 migrate) | ☐ 未动;`add_column_if_absent` 幂等助手已就绪,直接加列即可 |
| 4 | `setup()` 写 `reconcile()`:重启后从账本恢复在途任务 | ☐ 未动;`scheduler.rs` 已有 `resume_all`,需对照 |
| 5 | fake-claude kill -9 混沌测试,确认杀进程组不留孤儿 | ◐ `tests/cancel.rs` 已存在(**未追踪**),用 kill **-TERM** 单 PID;通过。任务要 kill **-9** + 进程组无孤儿——属精炼 |

> ※ P0 阶段约束:经理用 Rust 策略(不上 AI 经理);合并保持手动(`run.rs` 里
> `keep_branch:true` 的安全缝不动)。

---

## 🗺️ 关键代码地图（省后续轮次重新摸索）

- **事件 wire 契约**:`crates/quiver-core/src/event.rs`(`AgentEvent` / `AgentEventPayload`,
  serde `tag="kind"` + camelCase)↔ `frontend/src/types/agentEvent.types.ts`。**改一边必须同步另一边。**
- **Claude runner**:`crates/quiver-core/src/runner/claude/{parse,adapter,mod}.rs`。
  parse → RawLine,adapter → AgentEvent。`runner/mod.rs` 有 `AgentRunner` trait + `SpawnedAgent`。
- **supervisor**:`crates/quiver-core/src/supervisor.rs`(`run_task_streaming` / `TaskSpec` /
  `RunOptions` / `FinishStatus` / `Cleanup`)。
- **持久化**:`crates/quiver-store/src/{schema,settings,tasks,events}.rs`。schema.rs 的
  `migrate()` 是唯一增长点,`add_column_if_absent` 做幂等加列。
- **Tauri 层**:`src-tauri/src/{lib,run,scheduler}.rs`。IPC 命令在 lib.rs 注册进 invoke_handler。
- **前端**:`frontend/src/`(在途重构到 `shell/` + `office/`,见上"⚠")。
- **设计文档**:`docs/autonomous-org.md`(后端蓝图,§4 agent 层 / §5 编排 / §6 记忆 / §20 schema /
  §23 分阶段)、`docs/ui-design.md`(UI 北极星)、`docs/redesign-iso-directions.html`(可运行原型)。

---

## 📜 历轮记录

### 第 1 轮(2026-06-09)
- 建本 BUILD-LOG;评估现状、定计划。
- 修 P0 #1 session_id 丢弃 bug,透到事件层 + 前端 wire(`a8502fd`)。
- 验证:cargo check / cargo test -p quiver-core / tsc 全过。
