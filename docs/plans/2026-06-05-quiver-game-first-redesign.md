# Quiver Game-First 重设计北极星文档

> 把 Quiver 从「带像素皮肤的网站」翻转成「本身就是一个游戏场景的 app」。
> 一句话裁决：**引擎不是问题，chrome（标签栏 + 表单卡片 + 模态框）才是问题。** 我们不换引擎、不重写 Rust、不动 IPC hooks，只做一次结构翻转 + 皮肤化 + juice。

---

## 0. 结论先行（裁决，不和稀泥）

**(a) 选 HYBRID game-first，不选 FULL-PHASER。** 决定性理由是中文 IME：第一版给中国人用，任务 prompt / 设置 / 预算全要打中文字。经过 5 路调研交叉验证，**没有任何 canvas 引擎能让你"在画布里打字"**——Phaser/rexUI 的 `TextEdit`、PixiJS 的 `DOMContainer`、Excalibur（还有 #1032 focus bug）统统是在 canvas 上盖一个真实的 HTML `<input>`/`<textarea>`。既然文本输入注定要穿透 canvas 回到 DOM，那 FULL-PHASER 只是给你一个**更差版本的 DOM**，还白扔约 1100 行已经分层干净的 React。所以 FULL-PHASER 连它唯一的卖点（"一块画布统一一切"）都兑现不了。

**裁决冲突点**：调研里"框架选型"明确投 HYBRID，"模板素材"和"场景架构"则花了大量篇幅推 `phaser4-rex-plugins` 把 settings/board 做成纯 canvas 面板——这两者在「文本输入这一个面是不是要回 DOM」上其实**不冲突**：rexUI 的输入控件底层也是 DOM overlay。我的裁决是：**rexUI 只用于"世界内、锚定在 sprite/场景坐标上、不需要键盘输入"的 UI（气泡、9-slice 木框、滚动列表展示层、toast）；所有需要键盘输入的字段（prompt、预算数字、模型名、搜索框）走一个按需召唤的 React `<textarea>` overlay（option-b）。** 不要为了"纯粹"把带 IME 的输入塞进 rexUI——那是自找 CJK 候选词回车提交的经典 bug。

**(b) 主框架 + 要装的库（确认过 `package.json`：当前 `phaser ^4.1.0` + `react ^18.3.1`，不升级 React）：**

```bash
yarn --cwd frontend add motion phaser4-rex-plugins
```

- **`phaser` ^4.1.0** — 保留，升格为全屏常驻世界（已装）。
- **`react` ^18.3.1** — 保留，缩成「IPC 数据层 + 按需文本输入 overlay」（已装）。**不升 React 19**（这正是 `@pixi/react v8` 出局的硬成本之一）。
- **`motion`**（即 Framer Motion 的新包名）— React overlay 的开合 juice（账本翻页、卷轴展开的入场/退场）。
- **`phaser4-rex-plugins`** ⚠️ **必须是 `phaser4-` 前缀，不是 `phaser3-`**。Rex Rainbow 已为 Phaser 4 重写整套插件（v4.0.7, MIT）；老的 `phaser3-rex-plugins` 跑不动 Phaser 4 的新 WebGL 渲染器。**这是最容易静默浪费几小时的坑。** 用于世界内 9-slice 木框 / ScrollablePanel / Dialog / Toast / 气泡——但**不**用它的 InputText 做主输入。
- `EventBus` 不是 npm 包，是从 `phaserjs/template-react-ts` 抄一个 `Phaser.Events.EventEmitter` 单例文件（MIT，明确允许直接 lift）。

---

## 1. 游戏世界地图：进入 app 看到什么

进入 app = **你站在工坊里**。没有标签页、没有顶栏、没有右上角齿轮。导航 = 看向墙上/桌上/书架上的物件并点击它，镜头推过去，物件打开。

```
╔══════════════════════════════════════════════════════════════════════════╗
║                         QUIVER · 弓匠工坊（全屏常驻世界 HallScene）           ║
║                                                                            ║
║   ┌── 壁炉 / 电力槽 ──┐                          ┌── 墙上公告板 ──┐         ║
║   │  🔥🔥🔥          │   [横梁挂着的灯笼💡]      │ ▤ 委托一  📌  │         ║
║   │  火越旺=预算越足   │                          │ ▤ 委托二  📌  │         ║
║   │  烧成余烬=预算暂停  │     ☕ rate-limit         │ ▤ 委托三  📌  │         ║
║   └─────┬───────────┘     咖啡杯飘在弓箭手头顶    │  ＋ 钉新委托  │         ║
║         │ = 预算 HUD       └──────────────────────┴──────┬───────┘         ║
║   ══════╪══════════════════════════════════════════════╪══════════        ║
║         │                  工 坊 地 板                    │                  ║
║    🏹 工位1            🏹 工位2            🏹 工位3      │   📖 桌上账本     ║
║    小弓箭手A          小弓箭手B          (空工位)      │   = 设置           ║
║    在 typing         在 sweating                       │                    ║
║    (跑着的agent)     (跑 Bash 测试)                     │   📚 书架/档案柜    ║
║                                                        │   = 运行档案+Logbook║
║    每个工位上的小弓箭手 = 一个正在跑的 claude 进程        │   一本书=一次历史run ║
║                                                        │                    ║
║   [门🚪 = 选项目/点亮工坊]                               │                    ║
╚══════════════════════════════════════════════════════════════════════════╝
                                  ▲
                  React 仅在你打字时，在对应物件上盖一层透明 <textarea>
```

### 世界内对象 → 功能映射表（4 个现有界面全部安置进世界，零标签页）

| 现有 React 界面 | 世界内对象 | 交互 | 打开后（内容层）| 现有代码归属 |
|---|---|---|---|---|
| **工坊**（`OfficeScene` 直播） | **房间本身** | 默认态，你就站在里面 | 弓箭手们在干活 = 实时 dashboard | `OfficeScene.ts` → `HallScene` |
| **公告板/任务队列**（enqueue/reorder/cancel） | **墙上公告板**（已有 notice-board 素材） | 点击 → 镜头推近墙面 | 任务卡 = 钉着的羊皮纸，拖动重排=重钉 | `TaskBoard.tsx` → `TaskBoardScene` |
| **设置** | **桌上账本**（ledger book） | 点击 → 书在前景**翻开**，页签=设置分组 | 书页内是可读字段行（数字/文本字段召唤 overlay） | `SettingsPanel.tsx` → `SettingsScene` |
| **运行档案** | **书架/档案柜**（已有 bookshelf 素材） | 点击 → 镜头 dolly 到书架 | 每次 run = 一本书脊/文件夹 | `ArchiveView.tsx` → `ArchiveScene` |
| **I/O Logbook**（单 run 全文 transcript） | **抽一本书 → 展开成卷轴**，或**直接点工位上的小人** | 点 run-book / 点跑着的弓箭手 | 卷轴当**框**，里面是干净等宽 transcript（RimWorld 规则） | `LogbookViewer.tsx` → `LogbookScene` |
| **预算/电力**（§10 美元上限） | **壁炉**（火焰高度 = 余额）| 环境态，永远可见，不是一个屏 | 满额=旺火暖房；花钱=火渐弱；暂停=火banked成余烬、灯暗、弓箭手放下弓坐下 | 新增；绑定 `useSettings` 预算字段 |
| **rate-limit 倒计时**（§5.5） | **弓箭手头顶咖啡杯** | sprite 上的环境气泡 | 唯一出现倒计时的地方，保持 diegetic | `poseMachine.ts` 已有 pose 表 |
| **跨 tab "有更新"提示**（`useTabAlerts`） | **公告板/账本上的微光/glint** | 不再是脉冲红点 | 物件上的呼吸式微光 | `useTabAlerts.ts` → 逻辑搬进 `UIScene` |

> **核心心智模型（来自 Spiritfarer/Strange Horticulture/Stardew）**：这是一个 **hub-world + 世界内 portal**，不是"主菜单→子屏幕"树。管理 = 在房间里走到物件前触摸它。用户永远看不到 tab-bar，只看到一个有东西可摸的房间。

---

## 2. ASCII Mockups（让 owner 一眼看懂「这是游戏不是网站」）

### Mockup A — 目标主场景（进入即此画面）

```
┌────────────────────────────────────────────────────────────────────────────┐
│  🔥 余额 $6.20/$10   ☾ 03:14    工坊里有 2 位弓匠在忙        [UIScene HUD]   │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│    ╱▔▔▔▔▔▔▔╲                                      ╔════ 公告板 ════╗        │
│   ╱  🔥🔥🔥  ╲    💡        💡        💡          ║ 📌 修复登录超时  ║        │
│  │  壁炉(预算) │                                   ║ 📌 重构 recall   ║        │
│   ╲▁▁▁▁▁▁▁╱                                      ║ 📌 加单测        ║        │
│                                                   ║   ＋ 钉新委托    ║        │
│   ═══════════════════════════════════════════    ╚═════════════════╝        │
│                                                                              │
│        🏹                  🏹                ┌─────────┐                     │
│       ╱│╲  💬"在跑测试…"    ╱│╲ ☕"限流·2:30" │ 📖 账本  │  ← 设置             │
│       ╱ ╲  (sweating)      ╱ ╲  (idle)       │ (设置)   │                     │
│      工位1                 工位2              └─────────┘                     │
│      委托:#a3f               委托:#b7c          ┌──┬──┬──┐                    │
│                                                │📚│📚│📚│  ← 书架(档案)        │
│   🚪门(选项目)         [空工位·等委托]          └──┴──┴──┘                    │
└────────────────────────────────────────────────────────────────────────────┘
   全屏 Phaser canvas · 无标签栏 · 无表单卡片 · 导航=点世界里的物件
```

### Mockup B — 翻开账本 = 设置（`SettingsScene`，HallScene 进入 sleep，背景虚化）

```
┌────────────────────────────────────────────────────────────────────────────┐
│  🔥 余额 $6.20/$10   ☾ 03:15                                  [HUD 仍在顶]   │
│  ░░░░░░░░░ (工坊背景虚化/调暗，火光仍在透出，弓箭手仍在动) ░░░░░░░░░░░░░░░░░░░│
│                                                                              │
│            ╔══════════════════ 一本翻开的账本 ══════════════════╗            │
│            ║  ┃运行┃ 预算 │ Agent │ 外观                        ║            │
│            ║  ┃━━━━┃                          ～ 羊皮纸质感 ～    ║            │
│            ║                                                     ║            │
│            ║   默认模式      ◀ 真实运行 ▶     (木刻拨杆)         ║            │
│            ║                                                     ║            │
│            ║   并发弓匠数     ◀   3   ▶      (旋钮)             ║            │
│            ║                                                     ║            │
│            ║   每夜预算上限   $ [ 10.00 ]  ← 点这里：React       ║            │
│            ║                   ▲ 透明<textarea>盖在此格,支持IME   ║            │
│            ║                                                     ║            │
│            ║   模型           [ claude-opus-4 ] ← 同上,可打中文   ║            │
│            ║                                          ✎ 自动保存 ║            │
│            ╚═════════════════════════════════════════════════════╝          │
│                       (点书外任意处 / 合上 → 书合拢动画 + 翻页音, 回工坊)      │
└────────────────────────────────────────────────────────────────────────────┘
  书=diegetic 框(rexUI 9-slice 木框) · 字段行可读 · 只有数字/文本格才召唤 React 输入
```

### Mockup C — 点小人 → 卷轴展开看完整 I/O（`LogbookScene`）

```
┌────────────────────────────────────────────────────────────────────────────┐
│  点击工位1的小弓箭手 → 镜头缓推向他 → 一卷羊皮卷轴从上「唰」地展开            │
│                                                                              │
│         ╭───────────────────── 卷 轴（diegetic 框）─────────────────────╮   │
│         │  ◜ 委托 #a3f · 修复登录超时 · claude-opus-4 · $0.043 ◝         │   │
│         │ ┌────────────────────────────────────────────────────────────┐ │   │
│         │ │ 09:14:02  ▶ worker_started   model=opus-4      [等宽·可读] │ │   │
│         │ │ 09:14:05  🔧 tool_use Bash   "cargo test"                  │ │   │
│         │ │ 09:14:31  📄 output_chunk    "running 42 tests..."         │ │   │
│         │ │ 09:15:10  🔧 tool_use Edit   src/auth/token.rs             │ │   │
│         │ │ 09:15:44  ✓ result ok=true   cost=$0.043  turns=7          │ │   │
│         │ │ 09:15:45  🎉 finished verified  branch=fix/login-timeout   │ │   │
│         │ │ ▒▒▒▒▒▒▒▒▒▒▒▒ (可滚动·rexUI ScrollablePanel) ▒▒▒▒▒▒▒▒▒▒▒▒  │ │   │
│         │ └────────────────────────────────────────────────────────────┘ │   │
│         │         [ ▶ 回放 ]  ← 让这个小人在工坊里重演整段 run            │   │
│         ╰────────────────────────────────────────────────────────────────╯   │
│                          (卷轴卷起音 + 收起 → 回工坊)                         │
└────────────────────────────────────────────────────────────────────────────┘
  两层规则：卷轴=低密度 diegetic 框；里面 transcript=高密度干净等宽文本，绝不做成"手写字"
  「回放」复用现成 useReplay → 小人按同样 pose 重新走一遍（log 是一场演出，不是文本 dump）
```

---

## 3. 模板 / 素材采购清单（5-8 个最高杠杆，license 已核）

> **OSS 仓库铁律**：仅把 **Kenney CC0** 系列 vendor 进 `frontend/public/assets/`。所有 ⚠️ 标记的（itch.io cozy 包、LimeZu）**commercial-OK 但禁止再分发**——只能 link/credit 或 build 时按文档下载，**绝不提交进本仓库**。

| # | 资源 | 用途 | URL | License | 怎么接入 |
|---|---|---|---|---|---|
| 1 | **`phaser4-rex-plugins`** v4.0.7 | 世界内 UI 工作马：9-slice 木框 / ScrollablePanel（档案&Logbook 列表）/ Dialog（确认合并）/ Toast / Sizer 布局 / 气泡 | [npm](https://www.npmjs.com/package/phaser4-rex-plugins) · [rexUI docs](https://rexrainbow.github.io/phaser3-rex-notes/docs/site/ui-overview/) | ✅ MIT | `yarn add`；game config 注册 `UIPlugin` 为 scene plugin `mapping:'rexUI'`；开 `dom.createContainer:true`。**坑：必须 `phaser4-`，不是 `phaser3-`** |
| 2 | **Kenney — Fantasy UI Borders**（140 borders） | 可缩放木质/羊皮 9-slice 框，直接喂给 rexUI `NinePatch`/Dialog/ScrollablePanel 的 `background` | [itch](https://kenney-assets.itch.io/fantasy-ui-borders) | ✅ **CC0** | vendor 进 `public/assets/ui/`；用内置 `this.add.nineslice(...)` 或 rex NinePatch。**单个最高杠杆的木框素材** |
| 3 | **Kenney — UI Pack + RPG Expansion** | 按钮/滑块/光标/丝带/木石面板 — 面板词汇表基线 | [UI Pack](https://kenney.nl/assets/ui-pack) · [RPG Exp](https://kenney.nl/assets/ui-pack-rpg-expansion) | ✅ **CC0** | vendor 进仓库，无需署名，可商用 |
| 4 | **Kenney — Interface Sounds + UI Audio** | cozy SFX：翻页、钉纸、坐下 thunk、校验 chime、火 banked 的 whoomph | [Interface Sounds](https://kenney.nl/assets/interface-sounds) · [UI Audio](https://kenney.nl/assets/ui-audio) | ✅ **CC0** | vendor；通过现有 `utils/sound.ts` + Phaser `this.sound` 播放 |
| 5 | **`phaserjs/template-react-ts` 的 `EventBus.ts`** | React↔Phaser 事件桥（mitt 风格单例），替掉 `App.tsx` 的 tab 切换 | [repo](https://github.com/phaserjs/template-react-ts) | ✅ MIT | 只 lift `EventBus.ts` 这一个文件 + `PhaserGame.tsx` ref 桥的写法 |
| 6 | **内置 NineSlice + `scene.transition()`** | 可缩放面板 + "开门/翻书"转场，零安装 | [Nine Slice](https://docs.phaser.io/phaser/concepts/gameobjects/nine-slice) · [ScenePlugin](https://docs.phaser.io/api-documentation/class/scenes-sceneplugin) | ✅ Phaser core (MIT) | 直接用；想要花哨入场再考虑 #7 |
| 7 | **`JHAvrick/phaser3-transitions`**（可选） | Fade/Slide/Grow/Explode UI 入场退场 | [repo](https://github.com/JHAvrick/phaser3-transitions) | ✅ MIT | tween-based，多数能移植到 v4，**需在 v4 上测试** |
| — | ⚠️ **LimeZu Modern Interiors** | cozy 室内 tileset（如要扩房间） | [itch](https://limezu.itch.io/moderninteriors) | ⚠️ name-your-price, 可商用, **需署名, 禁再分发** | 只 link+credit，**别 commit**；用免费 v2.2 原型 |
| — | ⚠️ **Pixel Parchment UI Kit / Cozy UI** | 羊皮卷轴/farming 图标 | [Parchment](https://the-bees-knees-games.itch.io/pixel-parchment-ui-kit) · [Cozy UI](https://pixel-banner.itch.io/cozy-ui-user-interface) | ⚠️ 可商用, **禁再分发** | link/下载，别 vendor |
| — | ❌ **Veyroa "Free CC0" RPG UI Pack** | — | [itch](https://veyroa.itch.io/fantasy-minimal-pixel-art-gui) | ❌ **标错的 CC0** | **不要用**：页面写 CC0 但实际打包了多作者、license 冲突/禁再分发的素材（其评论区已确认） |

**license 雷区小结**：① `phaser4-` ≠ `phaser3-`（功能坑非 license 坑，但同样致命）；② Veyroa 假 CC0——OSS 仓库禁用；③ LimeZu + 所有 itch.io cozy 包可商用但禁再分发，只能 link/credit。**保持仓库内素材集 Kenney-CC0-only，法律上就干净。**

---

## 4. 复用 vs 重建表（逐区域，基于现状审计）

| 现有文件/层 | 裁决 | 说明 |
|---|---|---|
| **`src-tauri/**`（Rust 核心 + 所有命令）** | **🟢 原样保留** | 硬要求。EventBus 永不触达 Rust；hooks 是唯一 IPC 调用方。命令名/channel（`agent-event`/`task-updated`/`enqueue_task_cmd`…）不动一个字 |
| **`hooks/useSupervisor.ts`** | **🟢 原样保留** | **IPC 契约必须原样保留。** agent-event 流 + 项目选择器，被 `GameBridge` 包一层 |
| **`hooks/useTaskBoard.ts` / `useSettings.ts` / `useArchive.ts` / `useReplay.ts`** | **🟢 原样保留** | 纯数据层，与展示无耦合。`useReplay` 已在 bus 上游切换事件源，Logbook 重演"直接就能用" |
| **`hooks/useTabAlerts.ts`** | **🟡 逻辑搬进 `UIScene`** | "有更新"badge → 公告板/账本上的 diegetic 微光。跨 tab 闪烁的想法以"世界内 juice"形式存活 |
| **`office/poseMachine.ts`** | **🟢 逐行保留（复用皇冠）** | 纯状态机 `reduceWorker`/`createWorker`，零 DOM/Phaser 依赖，replay 也复用。搬进 `HallScene` 跑 |
| **`office/types.ts`**（`WorkerView`/`Pose`） | **🟢 保留** | 场景的领域模型 |
| **`office/OfficeScene.ts`**（511 行） | **🟠 升格为 `HallScene`** | ~90% 存活：房间布局/火光/灯光/弓箭手 spawn/pose tween/气泡/celebrate juice 全保留。**新增**：board/book/door 热点、scene-launch 接线、`EventBus` 订阅、从 `PixelOffice` 搬入的增量事件循环。**移除** `onHover/onSelect/onReady` 这些 React 回调间接层，改在 canvas 内或 UIScene 做 |
| **`components/PixelOffice.tsx`** | **🔴 删除（逻辑吸收）** | 增量事件循环 + slot 记账搬进 `HallScene`；tooltip/detail DOM 搬进 canvas/UIScene。**注意：它现在每次 mount 创建/销毁一个 Phaser.Game——这个反模式必须消失，全局只能有一个 `Phaser.Game`** |
| **`components/SceneTabs.tsx`** | **🔴 删除** | 网页标签 → diegetic 导航（点公告板/账本/门）。**「这是个网站」的头号元凶** |
| **`components/settings/SettingsButton.tsx`** | **🔴 删除** | 右上角浮动齿轮 = 从世界外打扰玩家。改走账本 |
| **`components/board/TaskBoard.tsx` + `TaskCard.tsx`** | **🟠 重建为 `TaskBoardScene`** | 卡片=sprite，拖拽重排走 Phaser input + 同样的 fractional-position `onReorder` 契约（epsilon 逻辑直接移植） |
| **`components/settings/*`**（Panel/Field/Stepper/Button） | **🟠 重建为 `SettingsScene`** | 拨杆/旋钮调 `patch()`（契约不变）；数字/文本格召唤 React overlay |
| **`components/archive/*`**（ArchiveView/Entry/Logbook*） | **🟠 重建为 `ArchiveScene`/`LogbookScene`** | 书架→开一本书=Logbook；"回放"→`replay.start` 在 HallScene 重演 |
| **`components/{EventLogPanel,EventList,EventRow}.tsx`** | **🟠 重建为世界内 ticker** | 实时 log → UIScene 里的"工坊日志"滚动面板，或 per-archer 角色卡 |
| **`components/WorkerDetail.tsx`** | **🟠 canvas 内重建** | 点弓箭手 → 世界内"角色卡"面板（rexUI container） |
| **`components/{ProjectPicker,RecentProjects,ModeToggle}.tsx`** | **🟡 吸收进 Hall/UIScene** | 项目选择=门/招牌热点调 `pickProject`；模式切换=世界内拨杆 |
| **`components/TaskInput.tsx`** | **🟡 复用为 React `<textarea>` overlay** | 它已是受控 `<textarea>` + IME 处理，是 option-b 的现成基底 |
| **`components/CozyEmpty.tsx`** | **🟠 canvas 内重建** | 空状态 → 世界内小品（暗冷工坊/沉睡弓箭手） |
| **`App.tsx`** | **🔴 掏空成 ~30 行 shell** | 只 mount `<PhaserGame/>` + `<GameBridge/>`（headless，re-emit hooks 到 bus）+ `<TextInputOverlay/>`。删掉所有 panel-switching state |
| **`utils/sound.ts` / `utils/eventFormat.ts`** | **🟢 保留** | sound cue 在 Phase 5 接真音频；formatter 给 canvas 文本复用 |
| **`strings.ts`** | **🟢 保留并扩展** | 全部中文串被场景复用 |
| **`devMock.ts`** | **🟢 保留并扩展** | `scripts/agent-debug.sh` 验证关键；加 bus 级 mock 让场景能 headless 驱动 |
| **`styles.css`** | **🟡 大幅瘦身** | 只留全屏 canvas host + 文本 overlay 层；palette token 迁成 Phaser color 常量（已半镜像在 `OfficeScene`/`poseMachine`） |

**净结果**：Rust = 0 改动。hooks + poseMachine + types = 0 改动。`OfficeScene` = 升格。`components/` 里除文本输入机制外 = 重建为场景。`App.tsx` = 掏空成 shell。

---

## 5. 分阶段迁移计划（每阶段结束都能 `cargo tauri dev` 跑 + 截图验证）

> 用 `scripts/agent-debug.sh up/shot/eval` 在**真实窗口**逐阶段验证（见 `CLAUDE.md`）。原则：**替换件验证通过前不删旧件**，所以每个 checkpoint 都是绿的。

| Phase | 做什么 | 一句话验收标准 |
|---|---|---|
| **P0 — 搭游戏 shell（零行为变化）** | 加 `game/EventBus.ts`、`game/PhaserGame.tsx`（forwardRef host）、`game/scenes/{Boot,Preloader}.ts`；`OfficeScene` 的 load 移进 `PreloaderScene`；整个 Phaser game 配 `Scale.RESIZE` 全窗口 + `dom.createContainer:true`。旧 `App.tsx` 面板**暂留** | `agent-debug.sh shot`：房间照常渲染、弓箭手照常对事件反应，无回归 |
| **P1 — 全屏常驻世界 + HUD** | `OfficeScene`→`HallScene`；`launch('UIScene')`（预算/时钟/toast）；引入 `GameBridge` 让 **bus** 驱动弓箭手（删 `PixelOffice`）；删 React header/tabs，换成 HallScene chrome。board/settings/archive 此阶段**仍以旧 React 面板 overlay 形式打开**（临时桥） | 截图：全屏游戏 + 顶部 HUD；实时 agent 事件经 bus 驱动弓箭手；完成时 HUD 弹 toast。**壁炉=预算槽已绑定** |
| **P2 — 世界内任务板（`TaskBoardScene`）** | 公告板热点 → `launch('TaskBoardScene')`（Hall→sleep）；卡片重建为 sprite；拖拽重排+取消走不变的 hook 契约；"钉新委托"召唤 **文本 overlay（option-b 首次真用）**；删 React `TaskBoard` | 截图：点公告板镜头推近、卡片可拖排序、**用中文 prompt 端到端验证 IME**（候选词回车不误提交） |
| **P3 — diegetic 设置（`SettingsScene`）** | 门/账本热点 → `SettingsScene`；旋钮/拨杆调 `patch()`；数字/文本格复用 overlay；删 React `settings/*` | 截图：账本翻开、改并发数即时生效、预算数字字段中文输入法可用 |
| **P4 — diegetic 档案 + Logbook（`ArchiveScene`/`LogbookScene`）** | 书架热点；开书→Logbook；"回放"→`replay.start` 在 HallScene 重演（已经过 bus 接好）；删 React `archive/*` + `EventLog*` | 截图：书架→开一本书看到等宽 transcript；点「回放」小人在工坊重演 |
| **P5 — 转场 / juice / 音效 / 素材打磨** | scene 转场（camera fade/`scene.transition`）、`sound.ts` 接真音频、热点 hover 微光、火光随预算闪烁、粒子暖意、celebrate +XP/−$cost 飘字；`App.tsx` 掏成 3-元素 shell；删 `SceneTabs`、死组件、大半 `styles.css` | 截图+录屏：开书有翻页音+动画、完成有 confetti+飘 −$0.043、整体读作「游戏」 |

> 排序理由：世界+HUD 先（最大"像游戏"收益、最低风险）→ 需要文本的 board 第二（早早 de-risk IME）→ 简单面板 → 最后打磨。

---

## 6. 诚实风险清单

| 风险 | 真相 / 严重度 | 缓解 |
|---|---|---|
| **中文 IME（第一优先级）** | 只有真实 DOM `<input>`/`<textarea>` 才免费拿到 WKWebView 的 IME + 拼音候选条。**经典 CJK bug**：回车选候选词被误当成提交 | option-b 文本 overlay；提交 gate 在 `e.nativeEvent.isComposing`/`compositionend`；overlay `<textarea>` 配 CJK 字体栈（`PingFang SC, Hiragino Sans GB, Microsoft YaHei`，现成）；`scale.on('resize')` 时重定位 overlay。**永不**走 canvas-native 键盘捕获 |
| **WKWebView / WebGL 性能** | WKWebView WebGL 稳；真风险是动画开销不是渲染器。macOS 13–15 把 rAF 锁 60fps（macOS 26 Tahoe 解锁；社区 `tauri-plugin-macos-fps` 可破）——对 cozy idle-office 非问题 | 房间 overlay 用 `sleep()` 不 `stop()`（hub 冻结而非后台重渲染）；弓箭手数 cap（已有 `slot % 4`）；纹理打 atlas；气泡重绘已 gate 在文本变化；**全局只一个 `Phaser.Game`**（删 `PixelOffice` 的 per-mount game） |
| **Phaser 包体积** | 没传说中可怕。Phaser 4.1 的 `@phaserjs` scoped 包**正确 tree-shake**（不像 v3 的 980KB 单体）。Tauri 用系统 WebView，无 Chromium 重量 | 只 import 场景用到的；别整包拉 rexplugins（option-b 让输入完全不碰 rexUI）；`vite build --report` 盯着；scene 美术经 Preloader 懒加载 |
| **可访问性** | canvas 对屏幕阅读器/键盘不透明 | 保留一层 invisible-but-focusable DOM mirror 兜底关键动作（enqueue/cancel/settings）+ UIScene 全局快捷键。文本 overlay 本身已是完全可访问的真 `<textarea>`。**别发纯 canvas 无兜底版** |
| **素材管线：黑底元素集做 alpha 抠图** | 现有装饰集（campfire/bookshelves/notice board/lanterns/rug）是黑底，用前必须抠透明；当前 `preload` 逐个加载 30+ 单 PNG（4 角色还行，装饰集会爆 draw-call） | 给 `scripts/slice_assets.py` 加一步 black→alpha color-key（或预处理成带 alpha PNG）；打**texture atlas** 而非散 PNG。新 6×3 pose grid 按现有 `char{n}-*` 同法切（`CHAR_CELL 234×256`），扩 `makeAnims` |
| **场景 re-create 陈旧/监听泄漏** | overlay 关闭时 `stop()`，下次 `create()` 全新——利于正确性，但每次 `create()` 重订阅 bus，忘了在 `SHUTDOWN` 退订就泄漏 listener | 在 `create()` 订阅、`this.events.once(SHUTDOWN, …)` 退订（§3 写法）。**唯一好踩的 footgun** |
| **`phaser4-` vs `phaser3-` 插件包** | 非 license 但同样致命：装错包静默跑不动 v4 渲染器，浪费几小时 | 锁死 `phaser4-rex-plugins`；CI/install 文档显式标注 |
| **「过度 diegesis」反伤可用性** | 纯 diegesis 有真实可用性代价（RimWorld vs Dwarf Fortress）。2000 行 transcript 做成"手写卷轴"=不可读 | **两层规则**：容器用世界隐喻（书/板/卷轴），**内容保持可扫读**（等宽 log、清晰行）。框是木的，身是干净列表 |
| **无 tutorial 的 diegetic 导航** | Stardew/Strange Horticulture 的记录在案缺陷：没引导用户找不到可交互物件 | 可交互热点加缓慢呼吸式 glint/wiggle（affordance 微光）；空状态尤其要 glint 指向唯一可操作物件（暗冷工坊 → "点亮工坊·选择项目"发光在门/壁炉上） |

---

*北极星一句话*：canvas 已经是游戏，chrome 才是网站——掏掉 `App.tsx` 的标签栏/卡片/模态框，把 4 个界面绑到 4 个世界内物件（板/书/架+卷轴/壁炉），用「点物件+镜头移动」导航，把现有 React 面板重做成「打开物件后的内容」（数据密集处保持干净），按 P0→P5 把 juice 花出去，Rust 与 hooks 一行不动。
