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
→ `*.api.json` 是给 skill/headless 跑的导出件,**画布打不开它**(在画布里加载会显示"画布为空")。

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
| `char-edit.json` | **角色编辑**:给一张参考图,锁住同一角色换姿势/换装/视角(Qwen-Image-Edit-2511 + Lightning4步) | **UI**(画布可开 + `raw` 可跑) | ✅ |
| `char-edit.api.json` | 同上 API 导出 | API | ✅ |

> **职责分离(实测后的正确架构)**:**工作流只管"生成"**,**抠图交给 `--keyflat` 后处理**(洪水填充 color-key,扁平美术比任何 in-graph 抠图节点都稳)。
> ```bash
> # 改 prop-gen 里 node5 的 <object>,然后:
> comfy.py raw .../comfy-workflows/prop-gen.json --project quiver --prefix props/decor/<名> --keyflat --key-tol 95
> # 或不用工作流、直接 builder:
> comfy.py t2i "…game prop… on a solid uniform magenta background, clearly coloured …, no text." \
>   --project quiver --prefix props/decor/<名> --keyflat --key-tol 95
> ```
> `--keyflat`(t2i 和 raw 都支持)生成后自动用 `scripts/post/keyflat.py` 抠纯色底。实测 `anvil.png`/`candle_00001_.png` 都是这么来的,干净透明。
>
> ⚠️ **已退役 BiRefNet `keyable-prop`**:BiRefNet 是给真实照片做分割的,对扁平像素/插画会失败(整张当前景、背景 alpha~254 抠不掉)。in-graph 神经抠图对本项目无用,已删。模型文件留着,仅以后做写实/3D 风时再用。
>
> `prop-gen.json` 是 **UI 格式**——画布侧边栏 `projects/quiver/` 下点开即编辑,保存仍是 UI 格式;`raw` 跑它自动转 API。两个文件由 `api2ui` 保持一致(见下)。

## 用 api2ui 把 API 工作流转成画布可开的 UI 格式

手写的/builder 拼的是 API 格式(画布打不开)。skill 新增 `scripts/build/api2ui.py`:用运行中 ComfyUI 的 `/object_info` 真实 schema 把 API 转成扁平 UI/litegraph(画布能开),并处理 widget 顺序 + seed 后的 `control_after_generate` 幽灵 widget。
```bash
~/.venvs/current/bin/python ~/.claude/skills/comfyui/scripts/build/api2ui.py <api.json> <out_ui.json>
```
本目录的 `prop-gen.json` 就是这样从 `prop-gen.api.json` 生成的(已用 ui2api 回转逐节点校验一致、拓扑分层布局)。

## 模板占位:`--var` 从外面填提示词

`prop-gen` 的 prompt 里写了 `<object>` 占位。`raw --var K=V` 会把工作流里所有字符串中的 `<K>` 替换成 `V`,所以一条命令出不同道具:
```bash
comfy.py raw .../comfy-workflows/prop-gen.json --var object="a wooden barrel" \
  --project quiver --prefix props/decor/barrel --keyflat --pixelize --px 96 --colors 32
```
画布里直接改那段文字也等价。

## 角色编辑工作流 `char-edit`(锁身份换姿势/换装,Qwen-Image-Edit-2511)

**这是角色精灵表/换装/出新角色的正路**——给一张参考图,模型保住"同一个角色"只改你要的部分。实测:蓝帽弓箭手参考图 → 一条指令变成站立拉弓,身份(帽/发/配色/脸)完整保住。**不用 ControlNet、不用训 LoRA。**

权威接线(挖自 ComfyUI 官方 2511 模板):
```
UnetLoaderGGUF(Q4_K_M) → ModelSamplingAuraFlow(3.1) → LoraLoaderModelOnly(Lightning 4步) → CFGNorm
CLIPLoader(qwen_image) ┐                         LoadImage(<ref>) → FluxKontextImageScale ┐
VAELoader ─────────────┼→ TextEncodeQwenImageEditPlus(正:image1+提示)(负:空) ───────────┤
                       └→ VAEEncode(参考图)→ latent ───────────────────────────────────────┘
→ KSampler(4步/cfg1/euler/simple/denoise1) → VAEDecode → SaveImage
```

用法(`<ref>` 填上传的参考图名,`<change>` 填要改什么):
```bash
# 1) 上传参考图(单个干净的角色帧)
comfy.py upload /path/char.png            # 返回服务端文件名,如 char.png
# 2) 跑(锁身份 + 换姿势/换装/视角),再 keyflat+pixelize 回游戏风
comfy.py raw .../comfy-workflows/char-edit.json \
  --var ref=char.png \
  --var change="standing and drawing a bow, side profile view" \
  --project quiver --prefix characters/poses/bow --keyflat --pixelize --px 120 --colors 32
```
- `<change>` 例:`walking, side view` / `wearing heavy iron armor` / `front view, T-pose` / `celebrating with arms raised`。
- 提示词脚手架已固定"保持同一角色"的话术,你只填变化点。

### 需要的模型(全部魔搭 ModelScope 国内可下,放对目录即可)
| 文件 | 放到 | 魔搭来源 |
|------|------|----------|
| `qwen-image-edit-2511-Q4_K_M.gguf` (12GB) | `models/unet/` | `unsloth/Qwen-Image-Edit-2511-GGUF` |
| `qwen_2.5_vl_7b.safetensors` (15GB, fp16) | `models/text_encoders/` | `Comfy-Org/Qwen-Image_ComfyUI`(`split_files/text_encoders/`) |
| `qwen_image_vae.safetensors` (242MB) | `models/vae/` | `Comfy-Org/Qwen-Image_ComfyUI`(`split_files/vae/`) |
| `Qwen-Image-Edit-2511-Lightning-4steps-V1.0-bf16.safetensors` (810MB) | `models/loras/` | `lightx2v/Qwen-Image-Edit-2511-Lightning` |
| custom node `ComfyUI-GGUF` | `custom_nodes/` | github `city96/ComfyUI-GGUF`(走 ghproxy 镜像) |

下载命令(纯国内带宽):`HF_ENDPOINT` 不用;用魔搭 CLI——
```bash
~/.venvs/current/bin/modelscope download --model <魔搭ID> <文件> --local_dir /tmp/dl
```
> ⚠️ **别用 hf-mirror 下 Qwen**:这些是 Xet 存储仓,hf-mirror 会重定向回国际 `xethub.hf.co` CDN(走国际带宽、易断)。魔搭是纯国内 CDN。

### 实测 caveat(Mac/MPS,36GB)
- **156s/张**(Lightning 4 步);一套 18 帧约 45 分钟。要快就租云 GPU。
- 画风比扁平精灵略"精细",过 `--pixelize` 拉回;**逐帧动画级完全一致**可能仍需训角色 LoRA,但**出姿势/立绘/换装/新角色**已够用。
- fp16 文本编码器(15GB)选它是因为 MPS 不支持 fp8(会翻倍内存);nvfp4 是 N 卡专用。
