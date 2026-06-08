# `assets/` — 深夜工作室像素美术(CSS 组件)

Quiver 的全套像素美术,**直接用 React + CSS 实现**(不出 PNG、不靠 t2i)。
像素 UI 这种东西代码做更可控:改色改形即时生效,**动画在 CSS 里免费**(绕开"出不了流畅高帧"的限制)。

主题:**深夜工作室**(cozy 现代夜间合作社)。配色见 `palette.ts` / `pixel.css` 的 `--qv-*`。

## 目录(按类别分层)

```
assets/
├── index.ts          # 总出口(import 后自动注入 pixel.css);统一 from '@/assets'
├── palette.ts        # 调色板 token(与 pixel.css 的 --qv-* 一一对应)
├── pixel.css         # ★ 唯一真源:CSS 调色板变量 + 全部 @keyframes + 部件基类
├── primitives/       # Px —— 绝对定位像素块(最小积木)
├── characters/       # Worker —— 小工 = 看板上的一个 agent(6 状态)
├── props/            # 工位/窗/暖炉/书架/绿植/落地灯/挂钟/吊灯/地毯/猫/扶手椅
├── fx/               # Glow/Star/Dust/Steam/Bubble
├── ui/               # 游戏风 UI:MenuBar/Panel/Button/气泡/HUD/Icon(见下)
├── scenes/           # StudioRoom —— 组合出完整主大厅
└── preview/          # AssetsPreview —— DEV 预览画廊(?preview=assets)
```

## UI 套件(`ui/`,同款像素夜色)

| 组件 | 用途 |
|------|------|
| `MenuBar` | 游戏风顶部菜单栏(品牌 + 菜单项 + 右侧 HUD) |
| `Panel` | 像素面板:详情/设置/对话框的底(标题栏 + 关闭) |
| `Button` | 厚按钮 primary/ghost/danger(按下回弹) |
| `SpeechBubble` | **输出气泡**:浮头顶显示智能体最新一句(OutputChunk) |
| `StatusBubble` | **状态气泡**:`await/paused/throttle/verified/failed/retry/budget` 小徽标 |
| `CountdownBubble` | 咖啡倒计时(短时限流 resetsAt) |
| `BudgetBar` | 美元预算条(绿→黄→红,预算治理可视面) |
| `XpBar` | XP 进度 + 等级牌(游戏化) |
| `StatusTag` / `CostTag` | 任务列表/详情里的小色标 / 花费 |
| `Icon` | 像素图标(gear/play/pause/check/close/retry/…) |
| `Toast` | 通知(success/error/info,右滑入) |
| `Tooltip` | 悬浮提示 |
| `Spinner` | 加载(三点跳动) |
| `Dialog` | 模态弹窗(暗背景 + 居中面板 + 操作区) |
| `TabBar` | 标签页条 |
| `Toggle`/`Checkbox`/`TextField`/`Select`/`Slider` | 设置账本表单控件 |
| `ProjectCard` | 项目卡片(最近项目/选工坊) |
| `EventRow` | 事件流条目(图标 + 文本 + 花费 + 时间) |
| `EmptyState`(在 `scenes/`) | 空状态:工坊静悄悄(小工趴桌打盹) |
| `NavRail` | 侧边图标导航条 |
| `ProgressRing` / `ProgressBar` | 环形 / 线性进度(验证关、预算) |
| `Achievement` / `XpFloat` | 成就/升级弹窗 · +XP/+$ 飘字 |
| `SegmentedControl` | 分段控件(模拟/真实) |
| `Kbd` | 快捷键芯片 · `Divider` 分隔线 |
| `Banner` | 顶部横幅(预算到顶闭店等) |
| `StatTile` | HUD 统计块(任务/花费/等级) |
| `WorkerAvatar`(在 `characters/`) | 框住的工人头像 |
| `NotificationCenter` | 通知中心(可滚列表面板) |
| `ShortcutsPanel` | 快捷键总览 · `FieldRow` 设置字段行 |
| `Minimap` | 迷你地图(小工按状态着色) |
| `Cursor` | 像素光标 · `SoundToggle` 音效开关 |
| `SettingsLedger`(在 `scenes/`) | 设置账本整页拼装(面板+标签页+字段+控件) |

```tsx
import { MenuBar, Panel, SpeechBubble, StatusBubble, BudgetBar } from '@/assets';
<StatusBubble kind="await" />          {/* 头顶 "!" 等你处理 */}
<SpeechBubble>读 README,总结项目…</SpeechBubble>
<BudgetBar spent={62} total={100} />
```

## 用法

```tsx
import { StudioRoom, Worker, Desk, cssVar } from '@/assets';

// 整间主大厅(默认演示三只小工)
<StudioRoom />

// 自定义看板:按任务数据驱动
<StudioRoom workers={tasks.map(t => ({
  left: t.x, state: t.spriteState, body: cssVar('blue'), headphone: true,
}))} />

// 单个小工(看板格子 / 详情头像)
<Worker state="celebrate" body={cssVar('pink')} hair="#5a3a4a" />
```

> 任意组件被渲染时 `pixel.css` 会自动加载(`Worker` 与 `index.ts` 都 import 了它)。
> 直接深层 import 某个道具时,确保链路上有 `import '@/assets/pixel.css'`(或从 `@/assets` 总出口引)。

## 角色 6 状态(映射 DESIGN §5.5)

| `state` | 含义 | 表现 |
|---------|------|------|
| `idle` | 待命 | 呼吸 + 眨眼 |
| `work` | 干活 | 敲键盘(手在动) |
| `coffee` | 卡住(等你/限流/预算到顶) | 端咖啡 + 冒热气 |
| `sweat` | 紧张(跑测试) | 冒汗 + 发抖 |
| `cel` | 验证通过已合并 | 蹦 + 举手 + 撒花 |
| `sick` | 失败 | 变青 + 转晕 |

换皮:`body`(身体色)/ `hair`(发色)/ `headphone`(耳机变体)。看板上 4 种工人 = 4 套配色 + 配件。

## 约定

- **颜色只在 `pixel.css` 定义一次**(`--qv-*`),组件里用 `cssVar('amber')` 或 `var(--qv-amber)`,**不写颜色字面量**。
- **复杂动画走 CSS**(`pixel.css` 的 `@keyframes` + `.qv-a-*` 工具类),符合项目"自定义动画用 CSS 兜底"的规范。
- 像素坐标(`Px` 的 left/top/w/h)是**美术数据**,内联即可,不视为业务魔法数字。
- 组件:PascalCase、`interface` props、不用 `React.FC`、跨目录用 `@/` 别名。

## 后续

- 接入:把 `StudioRoom` 的 `workers` 由真实任务流驱动(`AgentEvent` → `WorkerState`,见 `office/poseMachine`)。
- 需要 PNG 时,可对组件做一次性截图导出;但推荐直接用 DOM 渲染,动画与可维护性更好。
