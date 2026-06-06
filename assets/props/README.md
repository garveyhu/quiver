# props/ — 世界里的物件

工坊里摆放的家具与装饰。**需要透明底**(生成时用平灰底,再 alpha 抠图)。
- `furniture/` — 工人会用/交互的大件(工作台、凳子、书架、账本、门)
- `decor/` — 氛围装饰(壁炉、灯、蜡烛、地毯、箭靶、箭桶、药水架、公告板)

> 风格基准见 [`../README.md`](../README.md)。独立道具统一追加:`a single <对象> centered on a flat plain light-grey background, no scene, soft, no text`。
> 多帧动画(壁炉)做成**横向条带**,每帧等宽。
> ⚠️ **隔离经验**:Z-Image 即便写了 "a single object" 也常**补几个底部杂物**(实测 `lantern.png` 多生了小桌/木墩)。对策:① 提示词再加 `isolated single object, nothing else, completely empty background`;② 或生成后裁出主体。所以独立道具默认状态先标 🔁(待裁切/重做),裁干净接入前端后再转 ✅/📦。

## furniture/ 状态表

| 资源 | 文件 | 用途 | 状态 | 预览 |
|------|------|------|------|------|
| 工作台+凳子(等距) | `furniture/workbench.png` | 每个工人的工位 | ⬜ | — |
| 凳子 | `furniture/stool.png` | 备用座位 | ⬜ | — |
| 书架(档案柜) | `furniture/bookshelf.png` | 书架=档案入口 | ⬜ | — |
| 翻开的账本 | `furniture/ledger-book.png` | 账本=设置入口 | ⬜ | — |
| 木门 | `furniture/door.png` | 门=项目管理入口 | ⬜ | — |

## decor/ 状态表

| 资源 | 文件 | 用途 | 状态 | 预览 |
|------|------|------|------|------|
| 壁炉(4 帧火焰条带) | `decor/fireplace.png` | 预算电力槽 | ⬜ | — |
| 挂灯笼 | `decor/lantern.png` | 氛围光 | 🔁 | ![lantern](decor/lantern.png) |
| 铁砧 | `decor/anvil.png` | 工坊装饰(**keyable-prop 透明产线首个成品**:真 alpha 抠图) | 🔁 | ![anvil](decor/anvil.png) |
| 蜡烛 | `decor/candle.png` | 桌上点缀 | ⬜ | — |
| 圆地毯 | `decor/rug.png` | 地面 | ⬜ | — |
| 箭靶 | `decor/target.png` | 墙面装饰 | ⬜ | — |
| 箭桶(成捆的箭) | `decor/arrow-barrel.png` | 角落点缀 | ⬜ | — |
| 药水架 | `decor/potion-shelf.png` | 墙面装饰 | ⬜ | — |
| 公告板(软木) | `decor/notice-board.png` | 公告板=任务入口 | ⬜ | — |

## 提示词(节选;每条都前置风格基准)

**furniture/workbench**
```
A single cozy 16-bit pixel art wooden workbench with a stool and a small rug, a candle and a few archer's tools on top, isometric 3/4 view, warm wood tones, centered on a flat plain light-grey background, soft, no text.
```
**furniture/bookshelf**
```
A single cozy 16-bit pixel art tall wooden bookshelf filled with leather books, scrolls and quivers of arrows, warm wood tones, front view, centered on a flat plain light-grey background, soft, no text.
```
**furniture/ledger-book**
```
A single cozy 16-bit pixel art open leather ledger book with parchment pages and a quill, warm tones, top-down 3/4 view, centered on a flat plain light-grey background, soft, no text.
```
**decor/fireplace**(4 帧火焰)
```
A cozy 16-bit pixel art stone fireplace with a warm crackling fire, four-frame horizontal animation strip of the flames flickering, warm amber glow, each frame identical framing, centered on a flat plain dark background, no text.
```
**decor/notice-board**
```
A single cozy 16-bit pixel art wooden cork notice board with a few pinned parchment notes and red pushpins, warm tones, front view, centered on a flat plain light-grey background, soft, no text.
```

> 其余道具按上面句式套写(换对象 + 视角)。生成后补全提示词到本表下方。

## 集成

抠图后接入 `frontend/public/props/`,在 `HallScene`/`PreloaderScene` 注册。火焰条带按 `slice_assets.py` 的 `fire` 处理。
