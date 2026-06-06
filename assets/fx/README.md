# fx/ — 粒子 / 特效帧

完成撒花、glint 微光、火星 ember 等。透明底,多帧做横向条带。

> 风格基准见 [`../README.md`](../README.md)。追加:`centered on a flat plain dark background, no text`。

## 状态表

| 资源 | 文件 | 用途 | 状态 | 预览 |
|------|------|------|------|------|
| 撒花 confetti | `confetti.png` | 任务完成 juice | ⬜ | — |
| 星光 glint | `glint.png` | 热点 affordance | ⬜ | — |
| 火星 ember | `ember.png` | 壁炉 | ⬜ | — |
| 闪光 sparkle | `sparkle.png` | 通用强调 | ⬜ | — |
| 烟雾 puff | `smoke-puff.png` | 出现/消失 | ⬜ | — |
| 升级爆发 | `levelup-burst.png` | XP/升级 | ⬜ | — |

## 提示词(节选)

**confetti**
```
A cozy 16-bit pixel art celebration confetti burst, warm amber and cream paper bits, four-frame horizontal animation strip expanding outward, centered on a flat plain dark background, no text.
```
**glint**
```
A cozy 16-bit pixel art soft four-pointed sparkle / glint, warm golden, three-frame twinkle strip, centered on a flat plain dark background, no text.
```

## 集成

抠图后接入 `frontend/public/fx/`,Phaser 粒子/动画引用。当前 juice 多为程序绘制,接图后更精致。
