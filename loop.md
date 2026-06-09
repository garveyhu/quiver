# Quiver 前端重写 · loop 任务

你来给 Quiver(一个"自治 AI 研发公司"macOS 桌面应用)**从零重写前端**。
旧前端是在一版"平视办公室"上逐步加功能、跟设计稿跑偏了;现在**推倒重来**,
严格照最新设计稿重写,**并允许为适配前端而修改后端**。

这是个会反复唤醒你的 /loop:每次唤醒 = 继续推进一块、验证、提交、记录、干净收尾。
无人值守,夜里跑,早上看结果。

## 0. 每轮开头必做(状态在文件里,不在记忆里)
1. 读 docs/BUILD-LOG.md(它是跨轮真相:已完成/进行中/下一步/卡住的)。
2. 读相关代码/设计稿确认现状,别凭记忆。
3. 从 BUILD-LOG 的"下一步"挑一个【最小、可验证、可提交】的切片开工。

## 1. 先读这些(按需读对应段落,别全量灌)
- docs/redesign-iso-directions.html —— ⭐**前端唯一视觉/交互真相**:可运行的
  "等距(isometric)像素办公室"原型。新前端就是把它忠实搬进 React/TS。打开它、对照它做。
  里面有:等距瓦片办公室(7 房间:休息室/工位区/质检台/领导区·经理/可扩展/运维·预算/
  发货口)、工人走位+动画、连续语义缩放(滚轮缩进→模糊→工作台详情)、整夜时间轴回放、
  信任卡、晨报/验收、命令栏(⌘K)、HUD、预算条、急停。
- docs/ui-design.md —— UI 设计说明。
- docs/BUILD-LOG.md —— 后端这几夜做了什么(P0/P1/P2 已完成)、踩过的坑。读"晨报"段。
- docs/autonomous-org.md —— 后端架构蓝图(§22 像素办公室控制台是前端北极星之一、
  §4 agent 运行层、§5 编排、§6 记忆、§20 schema、§23 分阶段)。
- CLAUDE.md、~/.claude/rules/react-codebase.md —— 前端编码规约,**严格遵守**。

## 2. 这次怎么做(核心)
- **删掉 frontend/src/ 整个目录,重写。** 保留 frontend/ 脚手架(package.json、
  vite.config、tsconfig、index.html、node_modules);只重写 src/。
- 视觉/交互**忠实移植 redesign-iso-directions.html**(等距办公室),不要再用旧平视版。
- 【所有美术用代码生成】(CSS/canvas 像素,跟原型同款)——绝不用图片资源、不用 ComfyUI、
  不新开跑偏风格的 html。
- 用户可见文案一律**中文**。
- 做成能跑通的纵向切片:先静态等距场景 → 接真实数据(IPC)→ 实时事件 → 可写交互。

## 3. 后端:现有契约 + 【允许为适配前端而改后端】
后端(Rust)P0/P1/P2 已完成且测试全绿——它是**已验证的资产**。但这次**前端主导**:
**为了适配新前端,允许修改后端**(加新 IPC 命令、调整 wire 形状/字段、补返回数据、
加事件)。护栏:
- 改后端必须保持 `cargo check --workspace` + `cargo test --workspace` **全绿**才提交。
- 别破坏 supervisor/scheduler/记忆 的核心逻辑(P0 崩溃恢复 / P1 记忆 / P2 作废·待审·
  印证);只在其上**加/调**接口面,不要推翻已验证的机制。
- 守现有分层与规约:IPC 命令写在 src-tauri/src/lib.rs、注册进 invoke_handler、返回
  `Result<T,String>`、中文错误文案;纯 supervisor 逻辑在 crates/quiver-core(不碰 tauri 类型);
  持久化在 crates/quiver-store(操作库)与 crates/quiver-memory(记忆库,§6/§20);幂等 migrate。
- 改了 IPC/事件的 wire 形状,**前后端两边一起改、保持同步**(Rust 是 camelCase 序列化)。

### 现有后端可消费的东西(以 lib.rs / 各 crate 源码为准去核对签名)
- IPC 命令(invoke_handler 已注册):pick_project · select_recent_project ·
  remove_recent_project · set_project_alias · get_initial_state · enqueue_task_cmd ·
  run_task_cmd(单发,旧) · list_tasks · reorder_task · cancel_task_cmd ·
  get_task_events · get_settings · update_settings · get_stats · suggest_verify_command ·
  check_environment · get_brief
- 前端调后端:`import { invoke } from '@tauri-apps/api/core'`(**没开 withGlobalTauri,
  window.__TAURI__ 不可用,必须用 invoke**);事件 `import { listen } from '@tauri-apps/api/event'`。
- 事件通道:`agent-event`(每个 AgentEvent 带 taskId)、`task-updated`(任务变化刷看板)。
- AgentEvent(camelCase,tag=kind):worker_started{sessionId,model,authMode} ·
  tool_use{tool,summary} · output_chunk{text} ·
  result{ok,costUsd,numTurns,tokens,durationMs} · error{code,message} ·
  finished{status,costUsd,branch,verifyOutput}
- 主要 wire 类型(Rust 侧 + 旧 frontend/src/types 可参考,重写要干净):TaskRecord、
  Settings、Stats{total,verified,failed,costUsd,xp,level,spentDay,spentMonth,verifiedDay,
  failedDay}、InitialState、StoredEvent、EnvironmentCheck、Brief{project,facts[],recentEpisodes[]}。
- simulate 模式跑的是 fake-claude 假数据(免费、确定性),开发时用它验证。
- 千问 key 等密钥:**只在运行时从 ~/.agents/resources.json 读,绝不写进仓库或提交。**

## 4. 前端规约(react-codebase.md 摘要,严格遵守)
- TS strict;props 用 interface/type 显式声明,不用 React.FC;`@/` 路径别名,不写 ../../。
- 分层:IPC 调用集中在 services 或 hooks,不散进展示组件;展示组件只接 props 出 UI;
  跨子树共享状态走 Context/Zustand;服务端缓存别手写 useState+useEffect。
- 禁止硬编码颜色/字号/间距字面量——从主题文件/设计 token 取(原型调色板作 token 源)。
- 复杂像素/动画用 CSS。文案中文。

## 5. 每步必做:验证(别只读代码就说"好了")
- 类型:`yarn --cwd frontend tsc --noEmit` 过才提交;后端动了再 `cargo check --workspace`
  (必要时 `cargo test --workspace`)。
- 真机:用 scripts/agent-debug.sh 起真窗口看一眼(见 docs/agent-debug.md):
    scripts/agent-debug.sh up        # 起 app(:1420 必须先空)
    scripts/agent-debug.sh shot      # 截图 → 对照 redesign-iso-directions.html 看贴不贴(需终端 Screen Recording 权限)
    scripts/agent-debug.sh eval '…'  # webview 跑 JS 查 DOM(走 localhost,不需权限)
  改了界面就截图对照原型,逐步逼近。

## 6. 已知的坑(省得重踩)
- **启动报错多半是 :1420 被残留 vite 占了**:`cargo tauri dev` 的 vite 子进程在 app
  退出后容易残留 listen :1420 → 再启动 "Port 1420 already in use"。先
  `lsof -nP -iTCP:1420 -sTCP:LISTEN` 看谁占着,kill 那个 node/vite。
- **收尾务必拆掉自己起的测试实例**,别留着占 :1420(否则下次启动又撞端口)。
- tauri.conf 的 frontendDist 指向 ../frontend/dist:dev 用 `cargo tauri dev`(:1420 实时源码),
  打包版(.app/release)才用 dist;改了 dev 看不到就确认跑的是 cargo tauri dev。
- bridge 程序化 .click() 触发不了 React 受控输入/onClick——验证渲染/布局可以,验证
  "点了有反应"靠真人点或集成测试。
- 集成测试改了 fake-claude 要先 `cargo build -p fake-claude` 再跑(否则跑到旧二进制)。

## 7. 每轮纪律 + 红线
- 一次只做一块,做完【必须验证】(tsc / cargo / 截图),过不了不提交、标 WIP 记进 BUILD-LOG。
- 验证过就【提交】(在分支 feat/game-first;Angular 提交规范;footer 加
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`)。粒度小、信息清楚。
- 还有余力接着下一块;上下文变大或遇卡点就干净收尾:更新 BUILD-LOG 后停。
- 每轮结尾更新 docs/BUILD-LOG.md:✅这轮做了啥(带 commit hash)、▶下一步、⚠卡住/需我拍板的。
- 红线:【绝不 push / 开 PR / 合 main】(我自己合);【绝不硬编码密钥】;不删不是你建的、
  不做 git reset --hard / force push;同一错误连续两次过不去就停手、记进 BUILD-LOG、转下一个
  独立任务,绝不空转。

## 8. 建议节奏(纵向切片,顺序来)
1. 删 frontend/src + 搭骨架:App / 主题 token(取自原型调色板)/ IPC 封装层(services 或 hooks)/
   全局状态。
2. 等距办公室**静态场景**:iso 投影 + 瓦片地板 + 7 房间 + 墙 + 家具(isoBox),对照原型截图逼近。
3. 接 get_initial_state / get_stats / list_tasks:出 HUD + 看板 + 项目。
4. 工人落瓦片 + listen('agent-event') 实时动画(simulate 跑 fake-claude 验证)。
5. 命令栏(⌘K)/ 晨报 / 连续语义缩放 / 时间轴 / 信任卡 逐个搬。
6. 可写交互接 IPC:派活(enqueue_task_cmd)、取消(cancel_task_cmd)、重排、设置(update_settings)…
   缺什么后端数据/命令就**按需加后端**(见 §3)。

现在开始:读 redesign-iso-directions.html 和 lib.rs 摸清"要长成啥样"+"能调/该加什么",
然后删 frontend/src、从骨架重写,小步验证小步提交。
```
