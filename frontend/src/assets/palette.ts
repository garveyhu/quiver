/**
 * 深夜工作室调色板 · 唯一真源(与 pixel.css 的 --qv-* 变量一一对应)。
 * 组件里用 CSS 变量 `var(--qv-night)` 取色;需要在 TS 里引用色值(如导出 PNG)时用本表。
 */
export const PALETTE = {
  night: '#1b2440',
  night2: '#141b33',
  wall: '#2a2f4d',
  floor: '#3a3050',
  floor2: '#473b5e',
  wood: '#7a5a3c',
  wood2: '#9a7350',
  woodDk: '#5a4636',
  amber: '#ffd27a',
  amber2: '#ff9d4d',
  glow: '#ffe6ad',
  skin: '#ffc6a8',
  skin2: '#e89e82',
  pink: '#e2748a',
  blue: '#7aa7e6',
  screen: '#8fd0ff',
  plant: '#4f8a5a',
  plant2: '#5b9a66',
  cream: '#f3e6d0',
  red: '#e2604f',
  green: '#7bbf7e',
  cloth: '#3a2c4a',
  cloth2: '#2d2138',
  rug: '#5b466e',
  rug2: '#b07a8e',
  brick: '#46343f',
  brick2: '#382836',
  board: '#e7e1cf',
  neon: '#5ce0ff',
  dim: '#8b94a6',
  dim2: '#5a647a',
  ink: '#15131a',
} as const;

export type PaletteKey = keyof typeof PALETTE;

/** CSS 变量名,如 cssVar('night') → 'var(--qv-night)' */
export const cssVar = (key: PaletteKey): string =>
  `var(--qv-${key.replace(/([A-Z])/g, '-$1').toLowerCase()})`;
