# characters/ — 角色精灵表

> 🖐 **手绘资源,非 ComfyUI 产出。** text-to-image **做不了一致的多姿势精灵表**(每次生成的人物都不一样),所以角色这块用手绘 / Gemini 生成的整页精灵表,ComfyUI 不参与。

## 规格(切片脚本 `scripts/slice_assets.py` 依赖)

- 每张源图:**6 列 × 3 行 = 18 姿势**的网格,单张 2816×1536(每格 469×512)。
- 行 0:走路 6 帧;行 1:拉弓 6 帧;行 2:portrait / sick / celebrate / idle / reading 等单帧。
- 切片输出统一 **234×268**/帧(顶部留 12px 安全带,见 `docs/...`/切片脚本)。

## 状态表

| 资源 | 文件 | 角色 | 状态 |
|------|------|------|------|
| 弓箭手 1(蓝袍) | `archer-1.png` | 蓝袍金发 | 🖐 可用(源:`~/Documents/image/resources/quiver/Quiver-人物贴图1.png`) |
| 弓箭手 2 | `archer-2.png` | 绿袍精灵 | 🖐 可用 |
| 弓箭手 3 | `archer-3.png` | 白发法师 | 🖐 可用 |
| 弓箭手 4 | `archer-4.png` | 红发战士 | 🖐 可用 |
| 贴图 5-7 | — | 现代白领风,排版不同 | 🔁 风格不符,暂不用 |

> 当前 `frontend/public/sprites/` 用的是 1-4 切片后的帧。要更多角色 → 用 Gemini 出**同款 6×3 弓箭手**整页表(见 `~/Downloads/Quiver-Stardew-Asset-Prompts.md` 风格),放进 `~/Documents/image/resources/quiver/` 再跑切片脚本。

## 「换人物」待定

A 选角面板 / B 自动多样 / C 每项目固定角色 —— 玩法待用户拍板后实现。
