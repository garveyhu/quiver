# ui/ — 界面美术

DOM 浮层(Logbook/档案库/项目管理/账本)和 HUD 的皮。
- `panels/` — 9-slice 可拉伸框(羊皮 / 木质 / 卷轴 / 书)
- `buttons/` — 按钮态
- `icons/` — 世界内物件图标 + 状态徽章 + 光标

> 风格基准见 [`../README.md`](../README.md)。UI 件统一追加:`centered on a flat plain light-grey background, no text`。
> **9-slice 框**:四角不拉伸、四边可拉伸——生成时画一个**边角厚重、中间留空**的框,标好可切的安全区。

## panels/ 状态表(9-slice)

| 资源 | 文件 | 用途 | 状态 | 预览 |
|------|------|------|------|------|
| 羊皮纸面板 | `panels/parchment.png` | Logbook/档案库底 | ⬜ | — |
| 木质面板 | `panels/wood-frame.png` | 通用框 | ⬜ | — |
| 卷轴框(上下木轴) | `panels/scroll.png` | Logbook 卷轴 | ⬜ | — |
| 翻开的书 | `panels/book-open.png` | 设置账本 | ⬜ | — |
| 皮制页签 | `panels/tab.png` | 账本页签 | ⬜ | — |

## buttons/ 状态表

| 资源 | 文件 | 用途 | 状态 | 预览 |
|------|------|------|------|------|
| 木按钮(常态/悬停/按下) | `buttons/wood-button.png` | 通用按钮 3 态条带 | ⬜ | — |
| 图钉按钮 | `buttons/pin.png` | 钉新委托 | ⬜ | — |

## icons/ 状态表

| 资源 | 文件 | 用途 | 状态 | 预览 |
|------|------|------|------|------|
| 公告板图标 | `icons/board.png` | 入口图标 | ⬜ | — |
| 账本图标 | `icons/ledger.png` | 入口图标 | ⬜ | — |
| 书架图标 | `icons/shelf.png` | 入口图标 | ⬜ | — |
| 门图标 | `icons/door.png` | 入口图标 | ⬜ | — |
| 状态徽章(待接/进行/完成/失败) | `icons/status-badges.png` | 任务卡 4 态 | ⬜ | — |
| 光标 | `icons/cursor.png` | 自定义指针 | ⬜ | — |

## 提示词(节选)

**panels/parchment**
```
A single cozy 16-bit pixel art rectangular parchment panel / aged paper sheet with a subtle deckled border, warm cream tones, flat front view, thick consistent border for 9-slice scaling, empty center, centered on a flat plain light-grey background, no text.
```
**panels/scroll**
```
A single cozy 16-bit pixel art open parchment scroll with wooden rollers at top and bottom, warm cream paper, flat front view, empty center, centered on a flat plain light-grey background, no text.
```
**buttons/wood-button**
```
A cozy 16-bit pixel art wooden UI button, three-frame horizontal strip showing rest / hover-highlighted / pressed states, rounded carved wood with a warm bevel, identical framing per frame, centered on a flat plain light-grey background, no text.
```
**icons/board**(其余入口图标同句式换对象)
```
A single cozy 16-bit pixel art small icon of a cork notice board with pinned notes, bold readable silhouette, warm tones, centered on a flat plain light-grey background, no text.
```

## 集成

抠图后接入 `frontend/public/ui/`,在 `styles.css`(DOM 浮层用 `background-image` / 9-slice `border-image`)或 Phaser `NineSlice` 引用。
> 注:当前 DOM 浮层用纯 CSS 皮;接入图片纹理会更精致,但要相应改 `styles.css` 的 `--scroll-*` / 面板背景。
