I now have full grounding: the real shell layout (`scale = Math.min` contain, `DESK_X` fixed 3 desks, floating absolute panels, fixed 740×460 scene), the pose machine, the complete palette, the prop/UI inventory, and the CSS keyframe vocabulary. I have everything needed to synthesize one concrete, decision-made plan.

---

# Quiver UI/UX 统一重构方案 + 分阶段实施计划

> 本文是拍板文档。所有论断已对照真实代码（`QuiverShell.tsx`、`StudioRoom.tsx`、`poseMachine.ts`、`poseToState.ts`、`palette.ts`、`pixel.css`、`ui/index.ts`、`props/index.ts`）。每个改动都落在现有数据出口（`useSupervisor / useTaskBoard / useStats / useReplay / useWorkers`）和现有组件上，**不改后端 IPC 契约**。

---

## 1. 核心设计语言

**一句话定位：Quiver 是一台「夜班工坊驾驶舱」——一间全屏铺满、会随时间呼吸的像素夜班工作室是你的挡风玻璃，工坊外壳上焊着会发光的仪表、白板和传送轨，⌘K 是你不碰鼠标也能派活、追流、过关的引擎。**

主线 = **方向1「沉浸活世界」的世界观** × **方向2「指挥中心」的外壳结构** × **方向4「键盘效率流」的操作内核**。取舍理由：

- 用「沉浸活世界」做**心智模型**（任务=工人=空间叙事），因为它最贴产品定位、复用率最高。
- 用「指挥中心」做**布局骨架**（焊死的三舱外壳，根治"浮在画上的网页 div"），因为它最直接根治用户痛点③④。
- 用「键盘效率流」做**操作层**（⌘K 命令总线 + 焦点系统），因为重度监工整夜不该靠鼠标戳。
- **方向3「环境陪伴」按取舍降级为可选增强**：昼夜色温、安睡夜巡模式纳入后期阶段（P4/P5），不进主线骨架——它们是"锦上添花"，不是"根治简陋"的必需。

### 设计原则（5 条，按优先级）

1. **世界即界面，外壳即仪表（无浮层）**。不存在"漂浮在场景上的卡片"。所有 chrome 要么物化进世界（队列=白板便签、预算=油表/壁炉、XP=灯串），要么焊进统一外壳（顶栏 Strip / 右栏 Rail / 底坞 Dock）。物理上消灭"网站感"。
2. **两个尺度彻底解耦**。像素美术层 `cover` 整数倍缩放铺满（永不留黑边、永不糊），信息层是真·响应式 HTML（不缩放、断点重排、永远锐利）。这是根治痛点①②的唯一正解。
3. **光与姿态是实时反馈的母语**。低帧像素最怕"动起来廉价"，所以状态优先用**灯色切换 + 6 态姿态 + 气泡**表达（零帧成本、信息量大、永远高级），动效只做点睛。
4. **键盘是一等公民**。⌘K 命令面板是一切动作的单一入口；vim 式焦点环让"我现在操作哪块"永远可见。可爱的皮 + 专业的骨。
5. **颜色只从 `PALETTE`/`cssVar` 取，动效只用 `steps()` 低帧**。杜绝硬编码色值（符合 react 规范），杜绝 60fps 平滑缓动（出戏）。

---

## 2. 整体信息架构 + 自适应策略

### 2.1 三个"区域"，不是三个"页面"

保留 `view: 'board' | 'archive' | 'settings'` 状态，但重定位为同一座工坊的三个区域。切换不是换页，是镜头/外壳内容切换，外壳常驻。

| view | 空间隐喻 | 取代现状 |
|---|---|---|
| `board` | **主驾驶舱**：全屏工作室 + 三舱外壳 | 看板（场景+浮层） |
| `archive` | **档案室**：双栏 master-detail（书架/卷轴）+ 回放 | 居中单列 Panel |
| `settings` | **总控台**：配电箱/账本风格设置 | 居中单列 Panel |

### 2.2 主驾驶舱 = 三舱外壳 + 全屏氛围层

整窗是一块**焊死的工坊外壳**（统一 `qv-cockpit` 容器，舱格用现有 `inset 0 0 0 2px wall` 内描边语言连成一体），分四区：

- **顶栏 Strip**（贴顶通栏 ~36px）：brand + 项目名 · 过夜运行时钟 · **预算条 + XP 条**（连续量，主仪表）· **运行/排队/已完成计数胶囊**（离散量）· `⌘K` 提示芯片 · 声音。档案/设置/项目 从 tab 降为右侧小图标，点开右滑抽屉。
- **中舱 主观察窗**（flex 占满）：`StudioRoom` 全屏 `cover` 铺满，工位带锚底，工人在上、白板便签在后墙。
- **右栏 Rail**（可折叠 ~280px）：上半 QUEUE（委托白板/队列）、下半 FOCUS（跟随焦点工人的转写），共享边框、滚动域、`j/k` 导航。`]`/`[` 折叠/展开，折叠成 24px 书脊。
- **底坞 Dock**（~52px）：模拟/真实分段 + 输入 + 钉上去，是 ⌘K 的快捷投影；宽度随 Rail 折叠自适应。

### 2.3 自适应策略（根治痛点①②③）

**美术层（cover 整数倍缩放）**——替换现状 `scale = Math.min(...)`（contain，留黑边）：

```
verticalScale = clamp(floor(viewportH / SCENE_H 的整数倍优先), 0.8, 2.2)
水平方向:墙/地/踢脚线改 repeat-x 平铺,要多宽铺多宽 → 永远无黑边
工位数:deskCount = clamp(floor((viewportW - 边距) / DESK_PITCH), 2, 8)
工人 > 工位:多出的走咖啡角排队(idle/coffee),可点可选,不再 "+N 角标"
全场景 image-rendering: pixelated,只整数倍缩放 → 永不糊
```

**信息层（响应式重排，三档断点，用现有 `useElementSize` 驱动，非 CSS media query，保证 Tauri 拖拽实时重排）：**

| 窗口宽 | 档 | Strip | Rail | 主观察窗 |
|---|---|---|---|---|
| ≥1180 | 驾驶舱 | 全仪表 | 默认展开 | cover，工位全见 |
| 820–1180 | 紧凑 | XP 收成胶囊 | 可折叠 | cover，可裁天花 |
| <820 | 专注 | 只留预算条+计数 | 自动折成书脊 | 场景吃满，工人放大；一切走 ⌘K |

---

## 3. 看板主驾驶舱详细设计（ASCII）

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◤QUIVER·myrepo  ◷02:14  预算▓▓▓▓░░ $4.2/$30  Lv7▓▓▓▓░  ⏵3 ⏸2 ✓7   ⌘K  🔊 ▤⚙ │ ← Strip 顶栏仪表条
├────────────────────────────────────────────────────────────────┬───────────────┤
│  ╱窗·月光    ┌─委托白板(Kanban)─┐      🕐挂钟    油表[预算]  </>霓虹 │ ┌RAIL──────┐│
│ ░░砖墙(平铺)░│排队│进行│完成     │░░░░░░░░░░░░░░ XP灯串✦✦✧✧░░░░░ │ │QUEUE   5  ││
│             │▭便│▭便│▭便签    │                                │ │›01 读README││← 当前行(amber左条)
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │ │ 02 修登录bug真││
│                                                                │ │ 03 加暗色模式 ││
│  🔥壁炉   工位1      工位2      工位3      工位4     ☕咖啡角     │ ├──────────┤│
│ (预算)  ◆working   ◆tense    ◆idle    ◆arriving  ◆◆排队中   │ │FOCUS      ││
│         "改run.rs" "跑test"            头顶气泡=最新output     │ │你▸读README ││← 跟随焦点工人
│  🐈睡猫                                              🪴绿植     │ │◂工具Bash   ││  的迷你转写
│ ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔ 踢脚线 ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔ │ │◂$0.30      ││
│                                                                │ └──────────┘│
├──────────────────────────────────────────────────────────────────────────────┤
│ [模拟│真实]  › 给工人派个活…__________________________  ↵钉上去  ⌘K更多        │ ← Dock 指令坞
└──────────────────────────────────────────────────────────────────────────────┘
```

**z 轴三层语义**（对应数据三类）：
- **墙层（被动全局状态）**：委托白板（`Kanban` prop）= 队列便签、挂钟、**预算油表/壁炉**、**XP 灯串**（`FairyLights`）、霓虹 `</>`=系统活跃。
- **地板层（活动实体）**：工人=运行中任务，工位灯色=状态，头顶气泡=最新事件。
- **前景层（氛围）**：落地灯、地毯、睡猫、`Dust`、`Vignette`、`MoonBeam`、`LightPool`。

**点工人 → 镜头推近 + Rail FOCUS 聚焦该工人**（背景压暗 + 该工位台灯打亮 + Rail 转写过滤为这一个），不弹独立浮层 Panel。Esc 退出。

---

## 4. 标志性交互（8 个：动作 → 反馈 → 数据）

**① ⌘K「即说即派」+ 便签飞上白板**（招牌）。⌘K 召唤面板（`qv-pop`+translateY 3 帧 steps 降下，背景 `Vignette` 加深去饱和）→ 打字 → Enter。零状态显示"最近动作+在跑任务+高频跳转"，第一条永远是"用这段文字派活"，`⇧⏎` 切模拟/真实。回车瞬间：面板向 Dock 收拢 → **amber 便签纸从中央抛物线飞向白板"排队"栏**（`qv-rise`/`qv-fall` 位移）→ 落定 `Star` 火花 + `freshIds` 闪一下（`useTaskBoard` 已吐 `freshIds`）→ 门口一个工人 `arriving` 走进来（`worker_started` 到达时）。**数据**：`board.enqueue` / `freshIds` / `worker_started`。真实模式先弹现有确认 `Dialog`。

**② vim 式焦点环 triage**。`g`/Tab 在 zone 间循环（Rail/Dock/scene），当前 zone 一圈 2px amber 焦点环（复用 `qv-input:focus` 视觉）。进 Rail 后 `j/k` 移动行；queued 行 `J/K` 直接 reorder（行滑一格）、`dd` cancel（向右划出灰化）、Enter 展开转写；running 行 Enter = 镜头跳到对应工位（工人 `qv-hop`）。**数据**：`board.reorder` / `board.cancel`，replace 现状 `▲▼✕` 字符按钮。

**③ 工人头顶气泡 = 活的事件流**。每来 `output_chunk` 气泡打字机式 steps 追加；`tool_use` 切工具芯片 `✎ Bash`（蓝）；Bash → `tense` 姿态 + 冒汗 `qv-drop`；`result` → `等待校验 $0.30`（tense）。**余光扫一眼场景就知道全屋在干嘛**。**数据**：`WorkerView.bubble`（poseMachine 已折叠好），现状气泡是静态的，这里接活。

**④ 工位灯色状态系统**（光=母语）。每工位一个 `LightPool`，颜色直接驱动：working=暖黄、tense（跑命令）=冷蓝 `screen`、verified=绿、failed=红。全屋灯色一眼=今夜战况。**数据**：`poseToState`（已映射 6 态），扩展为"态→灯色"映射表即可。

**⑤ verify 关「升旗/红章」时刻**（取代右上角飘 Achievement）。`finished` 到达：verified → 工人 `cel` + `Star` 火花 + **工位上方升起印分支名的小旗** + **XP 灯串整串闪 + `+100 XP` 从工人头顶 `qv-rise` 升起** + 升级翻牌 `Lv7→Lv8`；Strip 的 ✓ 计数 +1。VerifyFailed → 工人 `sick` + 工位灯转红 + **该工位盖红章** + Rail 行红 `✕verify` 芯片，Enter 展开 `verifyOutput`（复用 Transcript 红框）。**数据**：`finished` / `verifyOutput` / `sparkle` / `stats.xp,level`。

**⑥ 预算闸「断电氛围」**（取代顶部红 Banner）。`budgetPaused` 翻 true：全屋灯统一转暖红 + `Vignette` 加深 + 霓虹 `</>` 闪烁断电感 + **预算油表/壁炉指针压红区 + 嗡嗡发红光**（壁炉则熄火冒青烟）+ 工人陆续切 `coffee` 停工 + 墙上飘"今夜额度用尽，等天亮"便签。Strip 预算条 `qv-glowpulse` 红脉冲。⌘K 输"预算"直接深链设置。**数据**：`budgetPaused`（算式照搬 L128）/ `barSpent,barTotal`。

**⑦ Quick-peek 悬浮转写**。Rail 太窄看不全时，任意任务行按 `space` → 中央 steps 展开完整 `Transcript`（你的委托气泡+输出+工具卡+verify 框），`j/k` 滚动，space/Esc 收起，焦点回原行。**数据**：`liveEvents.filter(taskId)`。

**⑧ 档案书架 + 擦洗回放**。`archive` 区右滑/平移到档案墙，书脊=历史任务（色=verified 绿/failed 红/verifyFailed 琥珀）。抽出 → 卷轴（Transcript）+ `r`/`▶ 在工坊回放` → `replay.start(evs)` + 回 board，那次工人重走大厅重演；回放时镜头加"REC 回放中"角标。**数据**：`useArchive` / `useReplay`（现状 `replay.start→setView('board')` 钩子保留 L249）。

---

## 5. 像素美术 + 动效规范（低帧约束下的现代高级感）

1. **层次靠光影，不靠 blur 阴影**。全屋只 2–3 个主光源 + 每工位一盏会变色的 `LightPool`，大面积暗部 = 高级。光是唯一实时反馈通道，改一个 CSS 变量即换状态色，零帧成本。
2. **留白 = 大面积安静的墙和地**。工位间留足空地（绿植、空椅、地毯），信息密度集中在"工位+白板+Strip"三处，其余呼吸区。均匀堆砌才简陋，留白制造焦点。
3. **动效分三档，严格限制**：
   - 环境微动（常驻 2–6s 慢循环）：`Dust` 浮尘、`MoonBeam` 轻摆、串灯呼吸、`Steam`——复用 `qv-floaty/qv-steam/qv-pulse`。
   - 状态过渡（事件驱动 80–300ms，全 `steps()`）：镜头平移/zoom、灯色切换、便签飞行、姿态切换、气泡打字、焦点环跳、行 reorder 滑动。复用 `qv-pop/qv-slidein/qv-rise/qv-fall/qv-bob`。
   - 庆祝点睛（一次性 <1s）：`Star` 火花爆、升旗、XP 灯串闪、翻牌。低帧逐帧但只播一次 = 郑重不廉价。
4. **像素锐利度**：全场景 `image-rendering: pixelated`，缩放只整数倍，根治现状 `Math.min` 任意小数 scale 的糊边。
5. **像素方块光标**（`qv-cursor` 已存在未用）：Dock/⌘K 输入框用 steps 闪烁方块光标，强化"终端式驾驶舱"专业感。
6. **调色板不动**：新组件全部 `cssVar`/`PALETTE` 取色，杜绝硬编码（符合 react 规范）。

---

## 6. 组件层面

### 复用（绝大多数，零美术成本）
- **场景/props**：`StudioRoom`（改造）、`Kanban`（→真白板）、`FairyLights`（→XP 灯串）、`WallClock`、`LightPool`（→工位灯）、`NeonSign`、`Vignette`/`MoonBeam`/`Dust`、`Desk`/`Worker`（6 态）、`SpeechBubble`。
- **UI**：`MenuBar`（→Strip 基础）、`BudgetBar`/`XpBar`（逻辑搬进油表/灯串/Strip）、`StatTile`（→计数胶囊）、`StatusTag`/`CostTag`、`SegmentedControl`/`TextField`/`Button`、`Transcript`、`Dialog`/`ProjectDialog`、`Minimap`（Rail 总览可选）、`Achievement`/`XpFloat`（→升旗飘字）。
- **hooks**：`useSupervisor`/`useTaskBoard`/`useStats`/`useReplay`/`useWorkers`/`useElementSize`/`useEnvironmentCheck`——**全部现成，不改**。
- **CSS keyframe**：`qv-pop/slidein/rise/fall/bob/hop/glowpulse/steam/floaty/pulse/drop/tap`——基本够拼所有动效。

### 新建（克制，列清单）
| 组件 | 职责 | 文件位置建议 |
|---|---|---|
| `Cockpit`（外壳容器） | 三舱焊接布局 + 断点重排 + 全屏 cover | `frontend/src/shell/Cockpit.tsx`（替换 QuiverShell 浮层布局） |
| `StripBar` | 顶栏仪表条（时钟/预算/XP/计数胶囊/⌘K） | `frontend/src/shell/StripBar.tsx` |
| `Rail` | 右栏 QUEUE+FOCUS，可折叠书脊 | `frontend/src/shell/Rail.tsx` |
| `Dock` | 底坞指令栏（⌘K 投影） | `frontend/src/shell/Dock.tsx` |
| `CommandPalette` | ⌘K 命令面板 + Command Bus | `frontend/src/shell/CommandPalette.tsx` |
| `useFocusZone` | 全局焦点 zone + vim 网格（Context） | `frontend/src/hooks/useFocusZone.ts` |
| `useCommandBus` | 派活/导航/reorder/cancel/replay 统一前端 | `frontend/src/hooks/useCommandBus.ts` |
| `useTimeOfDay` | 昼夜色温（P5，纯 CSS 变量） | `frontend/src/hooks/useTimeOfDay.ts` |
| `StickyNote` | 白板便签纸 sprite | `frontend/src/assets/ui/` |
| `BudgetGauge` | 墙上预算油表（复用 BudgetBar 的 pct 逻辑） | `frontend/src/assets/props/` |
| `Flag` | verify 通过升旗 sprite | `frontend/src/assets/props/` |
| `FocusRing` | amber box-shadow + 2 帧脉冲焦点环 | `frontend/src/assets/ui/` |
| `Stamp` | 红/绿 verify 印章 sprite | `frontend/src/assets/ui/` |

---

## 实施进度(自动循环中)
- ✅ **P1 step 1**:`useElementSize` 改回调 ref(修 scale=1 bug)+ 场景 cover 整数缩放铺满、底部锚定(消灭黑边)。真应用验证 covers=true。
- ✅ **P1 step 2**:看板重构为焊死驾驶舱外壳(顶 HUD 仪表条 / 中场景+右栏 Rail / 底 Dock),废弃所有 absolute 浮层。真应用验证(scale=1.17、各区齐全、tsc 过)。
- ✅ **P2 招牌交互 ⌘K 命令面板**:`shell/CommandPalette.tsx` + wire ⌘K。输入即派活(列表首项)/ 搜索跳转看板·档案·设置·项目 / 切到最近项目;↵ 执行、⇧↵ 切模拟真实、方向键选、Esc 关。真应用验证(开关/聚焦/派活项/Esc 全通)。加进快捷键面板。
- ✅ **P3 工位灯色状态**:`poseToState.STATE_LIGHT`(态→灯色)+ StudioRoom 工人脚下渲染状态色光池 + QuiverShell 接线。验证:验证通过=绿;干活暖黄/跑命令冷蓝/失败红。光=状态反馈母语。
- ✅ **P3 点工人聚焦**:StudioRoom `focusedTaskId` → 选中工人时其他压暗(opacity 0.35,steps 过渡),配合右栏转写聚焦 = 点工人即推近那个工位。
- ⏳ 余下(都较大/可选):P2 白板便签队列 + vim 焦点系统、P1 收尾(墙地 repeat-x 平铺 + 动态工位 + 极窄断点,需 StudioRoom 流式化)、P4(预算油表/升旗/红章)、P5(昼夜/书架/夜巡)。

## 7. 分阶段重构计划

### 阶段 P1：骨架与自适应布局（最快看到现代感）
- **目标**：消灭黑边 letterbox + 把三块浮层收编进焊死的三舱外壳。这是"不简陋"最直观的信号。
- **改动范围**：新建 `Cockpit.tsx`/`StripBar.tsx`/`Rail.tsx`/`Dock.tsx`；`QuiverShell.tsx` 改为装配这四件套（保留 view 状态、所有 hook 接线）；`StudioRoom.tsx` 改 `cover` 整数倍缩放 + 墙/地 `repeat-x` 平铺 + 动态 `deskCount`（替换 `DESK_X` 硬编码 L43/L140、`scale=Math.min` L160）。
- **验证**：`cargo tauri dev` 启动，用 `scripts/agent-debug.sh shot` 截图，拖拽窗口宽/窄/极窄，确认场景永远铺满无黑边、信息层流畅重排不糊、工位数随宽增减、溢出工人走咖啡角而非角标。
- **风险**：cover 裁切可能裁到工人——对策：工位带锚容器中下部，裁天花不裁人。整数倍缩放在小窗可能过小——clamp 下限 0.8 + 专注模式兜底。

### 阶段 P2：白板便签队列 + 焦点系统 + ⌘K（操作内核）
- **目标**：队列物化进白板 + 键盘全速操控 + 招牌"即说即派"。
- **改动范围**：`Kanban` 接 `board.tasks` 渲染便签（替换 Rail/现状右侧列表的 `qv-eventrow`）；新建 `useFocusZone`/`useCommandBus`/`CommandPalette`；Dock 接 Command Bus；reorder/cancel 改键盘 `J/K/dd`（替换 `▲▼✕` L331-333）。
- **验证**：agent-debug `eval`/`type` 模拟 ⌘K 派活 → 看便签飞上白板 + freshId 闪 + 工人入场；`j/k/dd` triage 队列；确认零鼠标可完成派活→排序→撤回。
- **风险**：键盘事件与输入框冲突——现状已有 `typing` 守卫（L76）可复用扩展；Command Bus 抽象过度——先只覆盖派活/跳转/任务三类。

### 阶段 P3：实时反馈三件套（光 + 姿态 + 镜头）
- **目标**：把已有事件流"演"出来。
- **改动范围**：工人头顶气泡接活流（`WorkerView.bubble` 已就绪）；工位 `LightPool` 灯色映射表（扩展 `poseToState`）；点工人镜头推近 + Rail FOCUS 聚焦（接 `onWorkerClick` L267）；工具芯片气泡。
- **验证**：派一个真实/模拟任务，agent-debug 截图确认 working/tense/cel/sick 各态下灯色、气泡、姿态正确；点工人镜头推近、转写过滤正确。
- **风险**：`output_chunk` 高频刷气泡——节流合并（poseMachine 已 slice(0,80)，再加节流）。

### 阶段 P4：预算油表/壁炉 + 升旗 XP 灯串 + verify 章/旗（庆祝与治理物化）
- **目标**：把抽象预算/XP/verify 变成可感的世界物件。
- **改动范围**：新建 `BudgetGauge`/`Flag`/`Stamp`/`StickyNote`；XP 接 `FairyLights` 灯串 + 翻牌；`budgetPaused` 触发全屋断电氛围（替换 Banner L289）；verified 升旗 + 火花（替换 Achievement L389）。
- **验证**：构造 finished(verified/verifyFailed)、budgetPaused 状态（可用 replay 或 mock 事件），截图确认升旗/红章/断电氛围正确。
- **风险**：油表/旗/章新美术——保持极小 sprite，复用现有 Px/primitives 拼。

### 阶段 P5（可选增强）：档案书架双栏 + 昼夜色温 + 安睡夜巡
- **目标**：方向3 的陪伴增强 + 档案空间化。
- **改动范围**：`ArchiveView` 改双栏 master-detail + 书架；新建 `useTimeOfDay`（注入 `:root` 色温变量，所有 `var(--qv-*)` 自动跟随）；安睡夜巡模式（失焦/就寝触发，降帧大字概览）。
- **验证**：档案 `j/k/Enter/r` 键盘化回放；改系统时钟看色温变化；失焦进夜巡。
- **风险**：昼夜色温改全局变量可能与状态色冲突——色温只动墙/地基底，状态色（灯/旗/章）保持高饱和不受色温影响。

---

## 8. 明确不做什么

1. **不抛弃像素做扁平/拟物 SaaS**。所有现代感在像素风内追求，不引入 blur 阴影、渐变、圆角拟物、60fps 平滑动画。
2. **不改后端 IPC 契约 / 数据模型**。`agentEvent.types.ts`、`useTaskBoard/useStats/useReplay` 出口不动；本重构是"重新编排已有数据 + 加表现层/操作层"，落地风险才低。
3. **不引入 Phaser 或重型动画引擎**（已弃用 Phaser）。纯 React/CSS 像素组件 + `steps()` keyframe。
4. **不硬编码颜色/字号/间距字面量**。一律 `cssVar`/`PALETTE`/固定像素 token。
5. **不做平滑缓动过渡**。所有动效 `steps(2~3)`、80–300ms——低帧是风格宣言不是妥协。
6. **不把 P5（昼夜/夜巡/书架）塞进早期阶段**。它们是增强，不是根治简陋的必需，避免范围蔓延拖慢"最快看到现代感"。
7. **不保留任何 `position:absolute` 漂浮浮层**（现状 HUD/队列/详情/指令栏/Achievement 的浮层做法全部废弃，收进外壳/世界）。
8. **真实模式确认 Dialog 不删**（危险动作仪式感保留，L421）。

---

**落地起点**：P1 第一步只做两件事就能立刻见效——`StudioRoom` 的 `Math.min`→cover 整数倍 + 墙地平铺（消灭黑边），和把三块浮层 div 收进 `Cockpit` 三舱外壳（消灭网站感）。这两步独立可验证、风险最低、视觉冲击最大。

相关文件（绝对路径，实现主战场）：
- `/Users/links/Coding/Archer/quiver/frontend/src/shell/QuiverShell.tsx` — 浮层布局全替换为三舱外壳装配
- `/Users/links/Coding/Archer/quiver/frontend/src/assets/scenes/StudioRoom.tsx` — cover 缩放 + 平铺 + 动态工位
- `/Users/links/Coding/Archer/quiver/frontend/src/office/poseToState.ts` — 扩展"态→灯色"映射
- `/Users/links/Coding/Archer/quiver/frontend/src/shell/Transcript.tsx` — 复用进 Rail FOCUS / Quick-peek / 工人屏幕
- `/Users/links/Coding/Archer/quiver/frontend/src/assets/pixel.css` — 新增少量 keyframe + 昼夜色温变量（P5）
- `/Users/links/Coding/Archer/quiver/frontend/src/assets/palette.ts` — 色值唯一真源（P5 加时段色温组）
- `/Users/links/Coding/Archer/quiver/frontend/src/hooks/useTaskBoard.ts` / `useReplay.ts` — 派活/重排/撤回/freshIds/回放出口（不改，接线）