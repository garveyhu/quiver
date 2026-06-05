/**
 * Canvas colour + sizing constants for the Phaser world.
 *
 * Phaser needs numeric (0xRRGGBB) colours, so the cozy palette is mirrored here
 * from styles.css (the CSS custom properties remain the source of truth for the
 * DOM). Keeping them in one module means no scattered hex literals across scenes.
 */
export const PALETTE = {
  // room / canvas
  canvasBg: '#fbf6ec', // --bg (cream parchment)
  canvasBgNight: '#251d18',

  // archer bubble + juice (mirrored from OfficeScene's former local COLOR)
  bubbleFill: 0xfffdf8,
  bubbleStroke: 0xe0cda8,
  bubbleText: '#3a2e25',
  sparkle: 0xffd27a,
  glow: 0xffb347,
  popup: '#ffe9b8',
  popupStroke: '#7a4f2e',

  // HUD chrome (UIScene)
  hudPanel: 0x4f3018, // --wood-dark
  hudPanelEdge: 0x6b4424, // --wood
  hudInk: '#f6ead0', // --parchment
  hudInkDim: 'rgba(246, 234, 208, 0.72)',
  hudAccent: '#e0992e', // --accent
  emberFull: 0xe0992e, // hot fire = full budget
  emberLow: 0x7a4f2e, // banked embers = depleted

  // hotspot affordance glint
  glint: 0xffd27a,
} as const;

// CJK-safe font stack reused across every canvas Text object.
export const CJK_FONT = 'PingFang SC, Hiragino Sans GB, Microsoft YaHei, sans-serif';

// Scene keys — one registry so launch/start calls never drift on a typo.
export const SCENE = {
  boot: 'BootScene',
  preloader: 'PreloaderScene',
  hall: 'HallScene',
  ui: 'UIScene',
} as const;
