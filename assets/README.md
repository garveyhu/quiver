# 🎨 Quiver 美术资产库

> 把 Quiver 当成一个**大型 cozy 像素游戏**来经营美术。这个目录就是资产总库:
> 每个子目录是一类资源,各自带 `README.md`(用途 + 风格基准 + 提示词 + 状态表 + 已生成图预览)。
>
> 物理位置:`<ComfyUI>/output/projects/quiver/`,通过仓库根的 `assets` 软链就地访问。
> 生成规则与管理规约见仓库 `docs/art-pipeline.md`。

---

## 🧭 风格基准(Style Bible — 所有提示词都隐含这一段)

> 统一风格是资产库的生命线。**每条提示词都在这段基准之上写**,只补该资源的具体内容。

- **风格**:精细的 **16-bit cozy 像素画**,crisp clean pixels,轻微 dithering。
- **色板**:暖琥珀 + 皮革棕为主,鼠尾草绿点缀;柔和发光的灯光(壁炉/挂灯/烛火)。
- **调性**:傍晚围炉、安静温暖的奇幻**弓匠公会工坊**。
- **引擎写法(Z-Image Turbo)**:自然语言描述、低 cfg、**别堆质量词**(no "masterpiece/8k/ultra")。
- **场景类**:追加 `wide interior composition, no characters, no people, no text`。
- **独立道具 / UI 类(需抠图)**:追加 `a single <对象> centered on a flat plain light-grey background, no scene, soft, no text`——平底好抠透明。

英文基准片段(可直接前置到任何提示词):
```
16-bit cozy pixel art, warm amber and brown palette, soft glowing lamplight,
crisp clean pixels, gentle dithering, calm cozy fantasy archer's-guild workshop mood
```

---

## 📂 资源分类

| 目录 | 内容 | 生成方式 |
|------|------|---------|
| [`scenes/`](scenes/README.md) | 全屏场景 / 房间背景 | ComfyUI t2i |
| [`characters/`](characters/README.md) | 角色精灵表(弓箭手) | 🖐 手绘(t2i 做不了一致多姿势) |
| [`props/`](props/README.md) | 世界里的物件(家具 `furniture/` + 装饰 `decor/`) | ComfyUI t2i + 抠图 |
| [`ui/`](ui/README.md) | 界面美术(面板 `panels/` + 按钮 `buttons/` + 图标 `icons/`) | ComfyUI t2i + 抠图 / 9-slice |
| [`fx/`](fx/README.md) | 粒子 / 特效帧 | ComfyUI t2i + 抠图 |
| [`branding/`](branding/README.md) | logo / 应用图标 / 启动图 | ComfyUI t2i + 后处理 |

---

## 🚦 状态图例(每个资源都有)

| 标记 | 含义 |
|------|------|
| ⬜ | 待生成 |
| ✅ | 已生成(下方有 markdown 预览图) |
| 🔁 | 已生成但待重做(不满意 / 风格不符) |
| 🖐 | 手绘资源(非 ComfyUI 产出) |
| 📦 | 已切片接入 `frontend/public/`(代码在用) |

> 约定:生成后把状态改成 ✅ 并补一行 `![名](文件名.png)` 预览;接入前端后在备注标「已集成」。

---

## ⚙️ 怎么生成(简版,详见 `docs/art-pipeline.md`)

```bash
cd ~/.claude/skills/comfyui
# 始终带 --project quiver:产物自动落到 output/projects/quiver/<子目录>/,即本资产库
~/.venvs/current/bin/python scripts/comfy.py t2i "<风格基准 + 该资源提示词>" \
  --project quiver --prefix scenes/workshop-hall --w 1216 --h 832 --seed 7
```

- `--prefix <子目录>/<名>` 决定落到哪个分类目录。
- 生成后:Read 出图核对 → 满意则把对应 README 的状态改 ✅ + 加预览;不满意调提示词重生成。
- 需要透明底的道具/UI:生成后用 `scripts/slice_assets.py` 同款 alpha 抠图(黑/灰底 key-out)。

---

## 📊 总进度

| 分类 | 已生成 / 计划 |
|------|--------------|
| scenes | 1 / 4 |
| characters | 0 / 4(🖐 手绘,4 张可用,见该目录) |
| props | 0 / 16 |
| ui | 0 / 14 |
| fx | 0 / 6 |
| branding | 1 / 3(app 图标已有) |

> 进度随生成更新;每类的细表在各自 README。
