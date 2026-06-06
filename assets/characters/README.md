# characters/ — 角色精灵表

> 现有 archer-1~4 是 🖐 **手绘整页精灵表**(继续用)。
>
> **2026 更新:ComfyUI 现在也能做角色了**——用图像编辑模型 **Qwen-Image-Edit-2511**(工作流 `comfy-workflows/char-edit`):给一张参考帧,锁住同一角色**换姿势/换装/视角/出新角色**,不用 ControlNet、不用训 LoRA。实测身份保持很好。
> - **适合**:补姿势帧、出立绘、换装变体、出新角色概念图。
> - **边界**:纯文生图(无参考)仍做不了一致角色;**逐帧动画级完全一致**的整页 18 帧可能仍需训角色 LoRA。Mac 上 156s/张偏慢。
> - 用法见 `comfy-workflows/README.md` 的「角色编辑工作流 char-edit」(含所需模型的魔搭下载)。

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

> 有了 `char-edit` 管线,"出更多角色/变体"不再是瓶颈(给参考图即可批量产)——选哪种玩法主要看产品决策,不再受美术产能限制。
