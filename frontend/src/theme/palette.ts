/**
 * 设计 token —— 取自 redesign-iso-directions.html 的 `:root` 调色板。
 *
 * 语义色与表面色集中在这里，CSS 侧的 `:root` 变量与本表保持同源(见 styles/global.css)。
 * 每件家具/瓦片用到的逐像素具体色值属于"美术数据"，与各自的绘制函数同处一地，
 * 不强行抽成 token——它们是像素画的内容，不是主题。
 */

export const PALETTE = {
  bgDeep: '#0a0e1a',
  text1: '#eef2fb',
  text2: '#aab4cd',
  text3: '#76819c',
  accent: '#ffd27a',
  ok: '#7bc47e',
  warn: '#f0b24a',
  bad: '#e2604f',
  info: '#8fd0ff',
} as const;
