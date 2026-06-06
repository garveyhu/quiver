# Quiver 美术资产管线与规约

> 把 Quiver 当**大型 cozy 像素游戏**经营美术的规章:资源怎么规划、怎么生成、怎么管理、怎么接入代码。
> 本文进 git(committed),是方法论的权威来源;**具体每类资源的提示词与状态**在资产库各目录的 `README.md` 里(那是活的目录册)。

---

## 1. 资产库在哪

- **规范存储**:`<ComfyUI>/output/projects/quiver/`(默认 `/Users/links/Coding/Hub/ComfyUI/output/projects/quiver/`)。
- **仓内访问**:仓库根 `assets` 软链 → 上面那个目录。代码里就地取用,但 `assets` 已 **gitignore**(指向仓外、是生成的二进制媒体,不进版本库)。
- **跨项目汇总**:所有项目都落在 `<ComfyUI>/output/projects/<项目>/`,在 ComfyUI 输出目录一眼看全。
- **真正进 git 的**:① 本规约 `docs/art-pipeline.md`;② 切片接入后的成品在 `frontend/public/`(代码实际加载的那份)。
  - ⚠️ 取舍:各目录 `README.md`(含提示词)随 `assets` 软链住在仓外、不进 git。提示词的"权威方法论"在本文;若要把目录册也纳入版本控制,可定期把 `assets/**/README.md` 快照进仓(见 §7)。

---

## 2. 目录约定(每个文件夹 = 一类资源)

```
assets/
├── README.md          # 总纲:风格基准 + 分类索引 + 状态图例 + 总进度
├── scenes/            # 全屏场景/房间背景(t2i)
├── characters/        # 角色精灵表(🖐 手绘,t2i 做不了)
├── props/             # 世界物件:furniture/(家具) + decor/(装饰)(t2i+抠图)
├── ui/                # 界面:panels/(9-slice) + buttons/ + icons/(t2i+抠图)
├── fx/                # 粒子/特效帧(t2i+抠图)
└── branding/          # logo/应用图标/启动图
```

- **可多级**:量大的类再分子目录(如 `props/furniture`、`ui/panels`)。
- **每个目录必有 `README.md`**:用途 + 风格基准引用 + 状态表 + 提示词 + 已生成图预览 + 集成说明。
- **加新类**:建目录 + 写 `README.md`(照现有格式)+ 在 `assets/README.md` 索引表加一行。

---

## 3. README 格式约定(每类目录)

1. **状态表**:`资源 | 文件 | 用途/规格 | 状态 | 预览`。
2. **提示词区**:每个资源一段 fenced code block(可直接复制去生成)。
3. **集成说明**:抠图/切片后接到 `frontend/public/` 哪里、哪个 scene 注册。

**状态图例(全库统一)**:

| 标记 | 含义 |
|------|------|
| ⬜ | 待生成 |
| ✅ | 已生成(同行 `预览` 列有 `![名](文件名.png)`) |
| 🔁 | 已生成但待重做(不满意/风格不符) |
| 🖐 | 手绘(非 ComfyUI) |
| 📦 | 已切片接入 `frontend/public/`(代码在用) |

> **铁律**:生成后立刻把状态改 ✅ 并补预览图链接——这样目录册永远反映真实库存。

---

## 4. 风格基准(Style Bible)——统一是生命线

所有提示词都在这段之上写,只补该资源的具体内容:

```
16-bit cozy pixel art, warm amber and brown palette, soft glowing lamplight,
crisp clean pixels, gentle dithering, calm cozy fantasy archer's-guild workshop mood
```

- **场景类**追加:`wide interior composition, no characters, no people, no text`。
- **独立道具/UI/fx(需抠图)**追加:`a single <对象> centered on a flat plain light-grey (或 dark) background, no scene, soft, no text`——平底好 key-out 透明。
- **Z-Image Turbo 写法**:自然语言、低 cfg、**别堆质量词**(no "masterpiece/8k/ultra/best quality")。详见 skill 的 `reference/prompt-engineering.md`。

---

## 5. 生成流程

```bash
cd ~/.claude/skills/comfyui   # 或 ~/.agents/extra-skills/comfyui
~/.venvs/current/bin/python scripts/comfy.py t2i \
  "<风格基准> + <该资源提示词>" \
  --project quiver \                 # ← 必带:落到 output/projects/quiver/(本资产库)
  --prefix <子目录>/<名> \            # ← 决定落到哪个分类,如 scenes/workshop-hall
  --w 1216 --h 832 --seed 7
```

- **`--project quiver` 必带**(或 `export COMFY_PROJECT=quiver`):产物自动归到资产库,本地下载也归到同处。
- **`--prefix <子目录>/<名>`** 决定分类目录与文件名。
- 生成后:**Read 出图核对** → 满意:改对应 README 状态 ✅ + 加预览;不满意:调提示词重生成(状态留 🔁)。
- Mac/MPS:Z-Image 1024² 约 70–280s(模型驻留后快);视频(Wan)很慢,慎用。
- 大批量:逐张提交;OOM 时 `scripts/comfy.py free`。

---

## 6. 抠图与接入代码

- **透明底**:Z-Image 出的是带底图。需要独立道具/UI → 用 `scripts/slice_assets.py` 同款 **alpha key-out**(边缘连通的灰/黑底 flood-fill 透明,见该脚本)。
- **9-slice**:UI 面板生成"边角厚、中间空"的框,切四角四边给 Phaser `NineSlice` 或 CSS `border-image`。
- **接入**:成品切片后放 `frontend/public/{bg,props,sprites,ui,fx}/`,在 `PreloaderScene` 加载、对应 scene 引用;`frontend/public/` 这份**进 git**(代码实际加载的)。接入后 README 状态标 📦。
- **换主场景的代价**:背景换了要重调 `HallScene` 的地板线 + 热点坐标(壁炉/公告板/书架/门),否则热点指空、小人浮空——属于真集成工作,别只换图。

---

## 7. 管理规约(governance)

- **t2i 能做什么**:场景、独立道具、UI 纹理、fx、品牌图 → 走 ComfyUI。
- **t2i 不能做**:**角色多姿势精灵表**(一致性做不到)→ 手绘/Gemini,标 🖐。
- **单一真源**:每类资源以 `assets/<类>/` 为准;`frontend/public/` 是"接入后的成品快照"。
- **不提交软链**:`/assets` 已 gitignore;成品在 `frontend/public/` 才进 git。
- **目录册快照(可选)**:想把提示词纳入 git,周期性 `cp assets/**/README.md` 到仓内 `docs/art/` 或直接在仓内维护一份镜像。
- **命名**:kebab-case,描述性;变体加 `-v2`/`-empty`/`-night`;动画用横向条带,每帧等宽。
- **风格一致 > 单图惊艳**:宁可统一也别每张换风格;新资源先对齐 Style Bible。

---

## 8. 当前状态(摘要,详见各目录 README)

| 分类 | 进度 | 备注 |
|------|------|------|
| scenes | 1/4 | 主场景 `workshop-hall.png` ✅ |
| characters | 4 可用 | 🖐 手绘;「换人物」玩法待定 |
| props | 0/16 | 待生成 |
| ui | 0/14 | 待生成;当前 DOM 浮层用 CSS 皮 |
| fx | 0/6 | 待生成;当前 juice 程序绘制 |
| branding | 1/3 | app 图标已有 |
