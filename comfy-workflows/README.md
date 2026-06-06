# comfy-workflows/ — Quiver 专用 ComfyUI 工作流

> 为本项目的**特定资产类型**定制的 ComfyUI 工作流,**随仓库 git 提交、供所有开发者共用**。
> 不再只靠 skill 的默认 builder——**按图片需求选不同工作流**(透明道具 / 角色 / 图标…)。

## 这套怎么运作

- **真实目录在仓里**:`quiver/comfy-workflows/`(committed)。
- **ComfyUI 反向软链**:`<ComfyUI>/user/default/workflows/projects/quiver` → 本目录。
  - 在 ComfyUI **网页画布**里它出现在 `projects/quiver/` 下,可直接打开编辑;
  - 在画布里改好 **保存**,文件就落回本目录(随 git 提交)。
- **跑工作流**(skill 的 `raw`,UI 格式自动转 API):
  ```bash
  cd ~/.claude/skills/comfyui
  ~/.venvs/current/bin/python scripts/comfy.py raw \
    /Users/links/Coding/Archer/quiver/comfy-workflows/keyable-prop.json \
    --project quiver --prefix props/decor/anvil
  ```
  `--project/--prefix` 会**改写工作流里 SaveImage 的前缀**,产物落进 `assets/props/decor/`(同资产管线)。

## 何时用自定义工作流 vs 默认 builder

| 需求 | 用什么 |
|------|--------|
| 普通文生图(场景/草图) | `t2i`(默认 builder,最省事) |
| **特定类型、要可复现的高质量产线** | **本目录的专用工作流 + `raw`** |
| 出图即透明(免手动抠图) | `keyable-prop.json`(t2i + 自动抠图 → RGBA) |
| 角色/三视图/特定构图… | 在画布里搭好存进来,再 `raw` |

## 工作流目录册

| 工作流 | 用途 | 格式 | 状态 |
|--------|------|------|------|
| `keyable-prop.json` | 单个道具 → **自动抠成透明 PNG**(Z-Image → RemoveBackground → JoinImageWithAlpha) | API | ⚠️ 需先装抠图模型(见下) |

> 加新工作流:在画布搭好 → 保存到 `projects/quiver/` → 回来在本表登记一行(用途/格式/状态)。

## ⚠️ keyable-prop 需要一个抠图模型

`LoadBackgroundRemovalModel` 当前**没有可选模型**(`bg_removal_name` 选项为空)。要让这条产线跑起来,先装一个本地抠图模型(无需 API key),例如 **RMBG-2.0** 或 **BiRefNet**:
- 放到 ComfyUI 对应模型目录后,`bg_removal_name` 就能选到;把 `keyable-prop.json` 第 10 节点的 `"RMBG-2.0"` 换成实际可选项即可。
- 没装时直接跑会报 `value_not_in_list` 并**列出你真实的可选项**——照着填回去。

> 想让我帮你下一个抠图模型把这条产线打通,说一声。
