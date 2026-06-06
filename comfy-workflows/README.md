# comfy-workflows/ — Quiver 专用 ComfyUI 工作流

> 为本项目特定资产类型定制的 ComfyUI 工作流,**随仓库 git 提交、供所有开发者共用**。
> 按图片需求选不同工作流(透明道具 / 角色 / 图标…),而不是只靠 skill 的默认 builder。

## ⚠️ 两种格式:UI(画布能开) vs API(只能 `raw` 跑)

ComfyUI 有两种工作流 JSON,**别混**:

| 格式 | 长相 | 画布能打开? | 怎么跑 |
|------|------|------------|--------|
| **UI / litegraph** | `{ "nodes": [...], "links": [...] }` | ✅ 能(侧边栏点开即编辑) | `raw`(自动转 API) |
| **API / 执行格式** | `{ "1": {"class_type","inputs"}, ... }` | ❌ **不能(画布显示"画布为空")** | `raw` 直接跑 |

→ **想在画布里编辑的工作流必须存成 UI 格式**(在画布里搭好后**保存**就是 UI 格式)。
→ `*.api.json` 是给 skill/headless 跑的导出件,**画布打不开它**(这就是你点开 `keyable-prop` 空白的原因)。

## 推荐工作方式(画布编辑 + git 共享)

1. 在 ComfyUI 画布里搭/改工作流 → **保存**,存到 `projects/quiver/`(= 本目录,反向软链)。这是 **UI 格式**,画布随时能再打开。
2. 提交进 git,别人 clone 即可在自己的画布里打开同一条工作流。
3. 跑它(headless / 批量):`comfy.py raw <repo>/comfy-workflows/<wf>.json --project quiver --prefix <类>/<名>`。

## 这套怎么运作(反向软链)

- 真实目录在仓里:`quiver/comfy-workflows/`(committed)。
- ComfyUI 反向软链:`<ComfyUI>/user/default/workflows/projects/quiver` → 本目录。画布里在 `projects/quiver/` 下可见可编辑;保存即落回仓。
- `raw --project/--prefix` 会改写工作流里 SaveImage 的前缀,产物落进 `assets/<类>/`(同资产管线)。

## 工作流目录册

| 工作流 | 用途 | 格式 | 状态 |
|--------|------|------|------|
| `prop-gen.json` | 单道具出图(纯 Z-Image,纯洋红底);抠图交给 `--keyflat` | **UI**(画布可开 + `raw` 可跑) | ✅ |
| `prop-gen.api.json` | 同上 API 导出(headless/CI) | API | ✅ |

> **职责分离(实测后的正确架构)**:**工作流只管"生成"**,**抠图交给 `--keyflat` 后处理**(洪水填充 color-key,扁平美术比任何 in-graph 抠图节点都稳)。
> ```bash
> # 改 prop-gen 里 node5 的 <object>,然后:
> comfy.py raw .../comfy-workflows/prop-gen.json --project quiver --prefix props/decor/<名> --keyflat --key-tol 95
> # 或不用工作流、直接 builder:
> comfy.py t2i "…game prop… on a solid uniform magenta background, clearly coloured …, no text." \
>   --project quiver --prefix props/decor/<名> --keyflat --key-tol 95
> ```
> `--keyflat`(t2i 和 raw 都支持)生成后自动用 `scripts/keyflat.py` 抠纯色底。实测 `anvil.png`/`candle_00001_.png` 都是这么来的,干净透明。
>
> ⚠️ **已退役 BiRefNet `keyable-prop`**:BiRefNet 是给真实照片做分割的,对扁平像素/插画会失败(整张当前景、背景 alpha~254 抠不掉)。in-graph 神经抠图对本项目无用,已删。模型文件留着,仅以后做写实/3D 风时再用。
>
> `prop-gen.json` 是 **UI 格式**——画布侧边栏 `projects/quiver/` 下点开即编辑,保存仍是 UI 格式;`raw` 跑它自动转 API。两个文件由 `api2ui` 保持一致(见下)。

## 用 api2ui 把 API 工作流转成画布可开的 UI 格式

手写的/builder 拼的是 API 格式(画布打不开)。skill 新增 `scripts/api2ui.py`:用运行中 ComfyUI 的 `/object_info` 真实 schema 把 API 转成扁平 UI/litegraph(画布能开),并处理 widget 顺序 + seed 后的 `control_after_generate` 幽灵 widget。
```bash
~/.venvs/current/bin/python ~/.claude/skills/comfyui/scripts/api2ui.py <api.json> <out_ui.json>
```
本目录的 `keyable-prop.json` 就是这样从 `keyable-prop.api.json` 生成的(已用 ui2api 回转逐节点校验一致)。

## 配方:在画布搭出 keyable-prop(得到可编辑的 UI 版)

最省事:打开 ComfyUI 自带的 **`image_z_image_turbo`**(侧边栏里,UI 格式能开),在它的 `VAEDecode` 之后**接 3 个节点**,再存成 `projects/quiver/keyable-prop`:

```
VAEDecode ──IMAGE──┬──────────────► JoinImageWithAlpha.image
                   └► RemoveBackground.image
LoadBackgroundRemovalModel(BiRefNet.safetensors) ──► RemoveBackground.bg_removal_model
RemoveBackground ──MASK──► JoinImageWithAlpha.alpha
JoinImageWithAlpha ──IMAGE──► SaveImage
```

- 提示词改成**平光、纯底、无光晕**(可抠道具关键):
  `16-bit pixel art game prop, flat even neutral lighting, a single isolated <对象>, plain solid flat grey background, no glow, no halo, no shadow, nothing else, no text.`
- 保存后:`comfy.py raw <repo>/comfy-workflows/keyable-prop.json --project quiver --prefix props/decor/<名>`。

## 抠图模型

`LoadBackgroundRemovalModel` 用 **BiRefNet**(Swin-L, 1024)。已装:`<ComfyUI>/models/background_removal/BiRefNet.safetensors`(来自 hf-mirror 的 `ZhengPeng7/BiRefNet`,444MB)。换机器需重下到该目录。
