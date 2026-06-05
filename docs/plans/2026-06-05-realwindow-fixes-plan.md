足够了。我已经掌握全部根因证据。关键修正:帽子裁切**不是** cell 高度不匹配(234×256 完全一致),而是 `spawnNode:694` 用 `c{i}-idle`(idle 帧无顶部留白,内容贴帧顶 row 0),叠加 `fitSprite` 缩放 + origin `(0.5,1)` 底对齐后,帽子像素落在帧最顶端,容易被上方 bubble、相邻 sprite 的 depth 排序或 sprite 自身缩放精度吞掉——而 walk/bow 帧有手臂抬高的留白所以不裁。下面直接产出计划。

# Quiver 真窗口缺陷修复 + 重设计执行计划

> 北极星：`docs/plans/2026-06-05-quiver-game-first-redesign.md`。所有"真窗口验证"指用 `scripts/agent-debug.sh` 对**实际 Tauri 窗口**做 eval/读 DOM/读几何/截图，不用 headless（headless 用 2560×1600 掩盖了所有超框 bug）。

---

## 裁决（逐问题定性 + 难度 + 优先级）

| # | 问题 | 定性 | 难度 | 优先级 |
|---|------|------|------|--------|
| ① | 无输出却"完成" | **UX 不清晰**（非 bug：`finished` 事件正常驱动终态；`output_chunk` 内容只进 `detail`，气泡显示固定文案，用户看不到 agent 在做什么） | 低（1 行核心改 + 一处 HUD） | **P0 正确性/可观测** |
| ③ | 书架/卷轴超框 | **真 bug**（硬编码 margin 无 clamp，窄窗内容溢出 mask/视口） | 中（4 处几何 clamp） | **P1 超框** |
| ④ | 精灵帽子裁切 | **真 bug，但根因被误判**（帧 234×256 与 `CHAR_CELL` 一致，调查的"改 cell 高度"方案错误；真因是 idle 帧内容贴帧顶 row 0，无安全留白） | 低-中（切片脚本加顶部 padding 重切 + loader 对齐） | **P2 帽子** |
| ② | claude 健康检查 | **新功能**（缺 preflight 自诊，首次真实模式失败无引导） | 中-高（新 Rust 模块 + 命令 + 1 处 UI 落点） | **P3 健康检查** |
| ⑤ | 首页交互引导 | **纯美化 + 部分新功能**（入口偏角、无图标、glint 弱、无首次引导） | 中（HallScene 局部重写） | **P4 首页重设计** |

排序原则：**正确性/可观测（①）> 超框（③）> 帽子（④）> 健康检查功能（②）> 首页重设计（⑤）**。

---

## ① 无输出却"完成"——让工作可见

**根因**（已核验）：
- `frontend/src/office/poseMachine.ts:62-71` — `output_chunk` 分支把气泡写死成 `bubble: '思考 / 输出中…'`，实际输出文本 `text` 只塞进 `detail`（HallScene 气泡不渲染 `detail`，只渲染 `bubble`）。
- `frontend/src/office/poseMachine.ts:111-139` — `finished` 事件是唯一终态来源，**无论中途有没有 output/tool_use 都会翻 celebrate/sick**。所以"无输出却完成"是 `finished` 正常到达、而中途气泡从没展示过真实内容造成的错觉，链路并未断。
- `frontend/src/devMock.ts:39` `defaultMode: 'simulate'` — 用户默认落 simulate，devMock 的确发了 `output_chunk`，但气泡吞了它。

**修法**：

1. **核心改（poseMachine）**：让气泡显示实际输出片段。
   `poseMachine.ts:62-71` output_chunk 分支：
   ```ts
   case 'output_chunk': {
     const text = event.text.trim().slice(0, 80) || '（输出）';
     return {
       ...prev,
       pose: 'working',
       bubble: text,        // ← 从固定文案改为真实输出片段（保留 80 字截断）
       detail: text,
       log: pushLog(prev, event, text),
     };
   }
   ```
   注意：`tool_use`(73-85) 已经显示真实工具名，是好范式；只有 output_chunk 是哑的。

2. **HUD 提示一行**（消除"这是不是卡住了"的疑虑）：在 UIScene 的 HUD 加常驻提示「点弓箭手看完整卷轴」，文案进 `strings.ts`（如 `STR.hintOpenLogbook`）。落点 `frontend/src/game/scenes/UIScene.ts`。

3. **保留终态语义**：不动 `finished` 分支——它本来就该是权威完成信号。

**验收（真窗口）**：
- `scripts/agent-debug.sh up`，simulate 跑一单。
- `scripts/agent-debug.sh eval "window.__quiverWorkerBubble?.()"`（若无 hook 则）`scripts/agent-debug.sh shot ".board"` 连拍 3 张，间隔 1s：气泡文本应**随 output_chunk 变化**（不再恒为「思考/输出中…」）。
- 终态截图气泡为「完成 ✓ $x.xx」。

---

## ③ 书架/卷轴超框——几何 clamp

**根因**（已核验）：四处硬编码 margin、无 `Math.max`/clamp，窄窗下视口算成负或小于内容宽，mask 裁不住。

| 文件:行 | 现状 | 修法 |
|---------|------|------|
| `ArchiveScene.ts:314` | `viewportW = w - 60` | `const viewportW = Math.max(BOOK.width + 40, w - 60)` 并据此让 `BOOK.width` 自适应（窄窗时 `Math.min(BOOK.width, viewportW - 40)`）|
| `ArchiveScene.ts:315` | `viewportH = h - 196` | `const viewportH = Math.max(180, h - 196)`；同时把 chrome 带高（标题+控件 196）改为按 `frameRect.h` 比例算，不写死 |
| `LogbookScene.ts:338-340` | `viewportW = w - 56`；`viewportH = h - rollerH*2 - headerH - 28`；`viewportY` 复杂叠加 | `viewportH = Math.max(160, h - rollerH*2 - headerH - 28)`；并 `Phaser.Math.Clamp` viewportY 到「羊皮纸顶（`cy - h/2 + rollerH + headerH`）↔ 底（`cy + h/2 - rollerH`）」之间，保证 well 不出框 |
| `LogbookScene.ts:341-343 / 351` | mask/well 用未 clamp 的 viewX/viewTop | well 与 geometry mask 的 rect 用 clamp 后的同一组 `{viewX, viewTop, viewportW, viewportH}`，行文本 wordWrap 宽度扣掉 `padX*2 + stamp(70) + 14` 再喂 mask |
| `HallScene.ts:497-498` | `floorY = height-70`；`y = floorY - 58 - row*height*0.14`（archer 随高缩放、无 clamp，可能浮到可见地板上方/底部出屏）| `const y = Phaser.Math.Clamp(floorY - 58 - row*height*0.14, ARCHER_H*0.5, height - 90)` |
| `HallScene.ts:522-541` | hotspot label x 比例定位、label 宽可变，窄窗右溢 | label x 用 `Phaser.Math.Clamp(x, labelHalfW, width - labelHalfW)`（与 ⑤ 重排合并做）|

**验收（真窗口）**：
- `scripts/agent-debug.sh up`，用 eval 把窗口缩到极端：`scripts/agent-debug.sh eval "window.resizeTo(640,480)"`。
- 打开书架：`agent-debug.sh eval` 读书卡几何 `el.getBoundingClientRect()`，断言每张卡 `rect.right <= innerWidth && rect.bottom <= innerHeight`。
- 打开卷轴：同法断言 well/转录行不超过羊皮纸 frameRect。
- `shot` 截图肉眼复核 640×480 / 960×720 两档都不溢出。

---

## ④ 精灵帽子裁切——切片留白（根因已修正）

**根因（修正调查的误判）**：实测 `char{0,3}-{walk,idle,bow}.png` 帧均为 **234×256**，与 `PreloaderScene.ts:6` `CHAR_CELL={w:234,h:256}` **完全一致**——所以「改 CHAR_CELL.h 到真实 ch」的 Option A 是错的，不存在不匹配。`slice_assets.py:177-201` 用**跨所有帧的统一 bbox + pad=6**，所有姿势同高同位。真因：**idle/celebrate/sick 等单帧里帽子像素紧贴帧顶 row 0**（pad=6 不足以兜住帽尖，或统一 bbox 的 top 由"内容最靠上的那帧"决定后，帽子恰好压在切割边），渲染时 origin `(0.5,1)` 底对齐 + `fitSprite` 缩放，最顶一两行像素被采样/缩放精度吞掉，表现为"帽子被削平"。walk/bow 因手臂抬高自带顶部留白，所以不裁。

**修法（择一，推荐 A）**：

- **A（首选，源头修）**：`scripts/slice_assets.py:197` 把统一 bbox 的 `pad=6` 顶部单独加大——改成顶部 padding ≥ 12（`bbox[1] = max(0, bbox[1] - 12)`），重跑切片生成新 `char*-*.png`。这样帽子上方恒有 ≥12px 透明安全带，缩放再也吃不到帽尖。重切后帧高会变（如 256→268），**必须同步**更新 `PreloaderScene.ts:6` 和 `HallScene.ts:25` 的 `CHAR_CELL.h` 为脚本打印出的真实 ch（保持单一真源）。
- **B（不改素材，渲染兜）**：`HallScene.ts` `fitSprite`/`spawnNode:694` 给 sprite 容器留顶部余量——把缩放基准从 `ARCHER_H/CHAR_CELL.h` 改为对"内容高"而非"帧高"，或 origin y 从 1 微调到 0.98 给帽子让 2% 空间。（治标，不推荐，会引入姿势间跳动）

**验收（真窗口）**：
- `agent-debug.sh up`，simulate 跑一单让弓箭手切到 idle/celebrate。
- `agent-debug.sh shot ".board"` 或对单个 archer 元素截图，肉眼看帽尖完整（与 walk 帧对比无削平）。
- 切片后 `git diff --stat frontend/public/sprites/` 应显示帧文件更新；`grep CHAR_CELL` 两处 h 值一致且等于脚本输出。

---

## ② claude 健康检查（check_environment）——新功能

### 检查项清单（6 项，全本地探针、零 API 调用、零副作用、总 < 10s）

| id | 检查内容 | 实现 | 超时 |
|----|---------|------|------|
| `claude_found` | claude 二进制可解析 | 复用 `run.rs::resolve_real_claude()` | <100ms |
| `claude_version` | `claude --version` ≥ 最低版本 | spawn 一次解析 stdout（仅当 found） | <5s |
| `claude_auth` | 登录态 | 检查 `~/.claude/credentials*` / `~/.config/claude/credentials*` 存在且非空（**只查存在性不读内容**，不烧预算）；若检测到 `ANTHROPIC_API_KEY` 则 warn 走 API-key 路由 | <500ms |
| `git_found` | git 二进制可解析 | which_in_path("git") | <100ms |
| `path_fixed` | launchd 稀疏 PATH 是否被 `fix_path_env::fix()` 修复 | 当前 `PATH` 含 `/opt/homebrew` 或 `.local` 即 ok，否则 warn | <10ms |
| `worktree_temp` | `<project>/.quiver/worktrees` 可写 | create_dir_all 探针（秒级删） | <1s |

补充 `subscription_env`（可选）：`run.rs::assert_subscription_env()` 逻辑——检测到 `ANTHROPIC_API_KEY/AUTH_TOKEN/*_BEDROCK/*_VERTEX/*_FOUNDRY` 时 fail 并给中文修复说明（现状报错是英文且只在启动时炸）。

### 返回结构

新建 `src-tauri/src/environment.rs`，命令注册进 `lib.rs` 的 `invoke_handler`：

```rust
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct EnvironmentCheck {
  pub id: String,            // 上表 id
  pub label: String,         // 中文标签
  pub status: CheckStatus,   // ok | warn | fail
  pub message: String,       // 中文人话（含发现的路径/版本）
  pub remediation: Option<String>, // 仅 warn/fail：中文修复步骤
  pub detail: Option<String>,      // 路径/版本号等
}
#[tauri::command]
async fn check_environment(state: State<'_, AppState>) -> Result<Vec<EnvironmentCheck>, String>
```
所有 message/remediation **中文**（匹配项目约定）。`check_claude_found` 复用 `resolve_real_claude`，与启动路径同源，避免"体检过了启动还失败"。

### UI 落点（**一个明确方案**：账本第六页签「工坊体检」）

不用门口热点（侵入工坊原稿）、不用首启模态（打断入场）。**就放 SettingsScene 第 6 个分组**：
- `frontend/src/game/scenes/SettingsScene.ts` 的 `GROUPS` 加 `{ key: 'health', label: STR.settingsSectionHealth }`。
- 新 hook `frontend/src/hooks/useEnvironmentCheck.ts` → `invoke('check_environment')`，经 EventBus 喂给场景（遵循 React=IPC 数据层、Phaser=渲染的架构）。
- 渲染：每项 `✓/⚠/✗` + 标签 + message，warn/fail 展开 remediation；底部「重新检查」按钮；全 ✓ 显示「工坊准备就绪」。
- 新类型 `frontend/src/types/environment.types.ts`，文案补进 `strings.ts`。

**验收（真窗口）**：
- 故意制造失败：`QUIVER_CLAUDE_BIN=/nonexistent agent-debug.sh up`（或临时改 PATH），开账本→体检页签，`agent-debug.sh dom` 应见 `claude_found` 行为 ✗ 且有中文修复步骤。
- 正常环境全 ✓，「重新检查」可重跑。
- `agent-debug.sh ipc` 抓到 `check_environment` invoke 往返，无 Anthropic API 网络调用。

---

## ⑤ 首页重设计（四块落地清单）

现状弱点（已核验 `HallScene.ts:513-589`）：入口全在四角（`0.84/0.26`、`0.9/0.56`、`0.9/0.8`、`0.1/0.86`），纯中文无图标，glint 仅 7px/1100ms（M5 后偏弱），无首次引导。

### A. 入口美化
- **加图标**：`makeHotspotLabel` 加 `icon?: string` 参数，公告板📌 / 账本📖 / 书架📚 / 门🚪（emoji，零素材成本）。`HallScene.ts:628-648`。
- **放大 label**：字号 15→17px，内边距 12→16，圆角 9→12，加 0.25 alpha drop-shadow，确保复杂背景上可读。
- **门单独偏心**：门是"进入前关键决策"，与右栈视觉隔离（左下，强调）。

### B. 引导
- **首次引导序列**：`localStorage('quiver-seen-onboarding')` 门控，首次进入且有项目时，右栈三入口逐个 halo 闪 6 次 + 浮气泡（「把委托钉这儿，弓箭手接单开工」/「这儿改设置和预算」/「翻历史委托和卷轴」），5s 内播完。新方法 `showOnboardingSequence()`。
- **空状态箭头**：`showSleeper()`(`HallScene.ts:410-441`) 沉睡弓箭手下方加呼吸箭头「↓ 点亮工坊」指向门。
- **可观测衔接**：与 ① 的「点弓箭手看卷轴」HUD 提示协同。

### C. 布局
- **右侧竖栈对齐**：三入口统一 `x≈0.82`，y 取 `0.28 / 0.50 / 0.72` 均匀间隔（替代现 0.26/0.56/0.8 散布），与中心工位形成对话。
- **视觉层级**：spec 加 `priority: 'primary'|'secondary'`，公告板=primary（label 18px、halo 56px），其余 secondary（halo 48px）。
- **窄窗 clamp**：label x `Phaser.Math.Clamp(x, halfW, width-halfW)`（与 ③ 合并）。

### D. Juice
- **glint 增强**：7→10px，周期 1100→800ms，scale 1→1.8，alpha 峰 1.0。
- **hover 层次**：label 放大 1.08→1.15 且上浮 6px；halo 常亮 0.08 底色、hover 脉冲到 0.24；glint hover 加速到 400ms。
- **梯次呼吸**：四入口 glint tween 各错开 `i*200ms`，形成节奏而非齐闪。
- **声音反馈**（可选，需素材）：`utils/sound.ts` 加 `hover`（轻木拨）/`click`（钉书针啪），Kenney CC0。

**验收（真窗口）**：
- `agent-debug.sh shot`：右栈三入口竖直对齐、均匀间隔，各带图标，label 明显增大。
- `agent-debug.sh eval` 模拟 hover（dispatch pointerover），读 label scale/y 变化与 halo alpha 上升。
- 清 localStorage 后首启：连拍截图见三气泡 5s 内逐个出现。
- 无项目时截图：沉睡弓箭手下方箭头呼吸指向门。
- 640×480 下 label 不右溢（读 getBoundingClientRect）。

---

## 实现里程碑（拆分 + 可并行归并）

| 里程碑 | 内容 | 可并行 | 一句话验收 |
|--------|------|--------|-----------|
| **M1 可观测正确性**（①） | poseMachine output_chunk 气泡显真实输出 + HUD「看卷轴」提示 | 独立 | 真窗口 simulate 跑一单，气泡随输出变化、终态显「完成 ✓ $x」 |
| **M2 几何 clamp**（③ + ⑤-C 窄窗 clamp） | ArchiveScene/LogbookScene/HallScene 四处 `Math.max`+`Clamp` | 与 M1 并行 | 640×480 与 960×720 两档下书卡/卷轴行/archer/label 均不溢框（getBoundingClientRect 断言通过）|
| **M3 帽子留白**（④） | slice_assets.py 顶部 padding≥12 重切 + 两处 CHAR_CELL.h 同步真实 ch | 与 M1/M2 并行（仅碰素材+loader 常量）| idle/celebrate 帧帽尖完整无削平；两处 CHAR_CELL.h 一致 |
| **M4 环境体检**（②） | environment.rs + check_environment 命令 + 账本第6页签 + hook/类型/文案 | Rust 侧独立；UI 侧依赖 ⑤ 的 SettingsScene 不冲突 | 缺 claude 时体检页 `claude_found` 标 ✗ 带中文修复；正常全 ✓ 可重检 |
| **M5 首页重设计**（⑤-A/B/D + C 布局） | HallScene 图标/布局/glint/引导序列/空状态箭头 | 依赖 M2 的 label clamp 先落（避免重复改同段） | 真窗口右栈对齐带图标、glint 明显呼吸、首启三气泡引导、空态箭头指门 |

**归并建议**：M1/M2/M3 三者改动文件几乎不重叠（poseMachine vs 三场景几何 vs 切片脚本+loader），可三路并行后顺序合并；M4 Rust 侧与全部前端并行，UI 部分在 M2 之后落 SettingsScene；M5 必须在 M2 的 HallScene label clamp 落地后再开，避免同段二次冲突。