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

  // --- 任务公告板 (TaskBoardScene) ---
  boardScrim: 0x1c130c, // dim wash behind the focused board
  boardWoodLight: 0x8a5a30, // top sheen of the timber frame
  boardWood: 0x6b4424, // --wood, the plank face
  boardWoodDark: 0x422914, // carved frame shadow / border
  boardCork: 0x9c6b3f, // cork backing the pinned cards sit on
  boardCorkEdge: 0x5e3a1e,
  cardParchment: 0xfbf3df, // pinned parchment card face
  cardParchmentEdge: 0xd9c39a,
  cardInk: '#3a2e25', // --text on a card
  cardInkDim: '#7a6651',
  cardPin: 0xc0392b, // the red pin head
  cardPinShine: 0xe8786a,
  cancelBtn: 0xb44637, // cancel (撤回) affordance
  cancelBtnInk: '#fbf3df',
  pinBtn: 0x4f7a3a, // "钉新委托" call-to-action green
  pinBtnInk: '#fbf3df',
  // status badge fills (queued / running / verifying / done / failed)
  badgeQueued: 0x8a7a5a,
  badgeRunning: 0x2f6fb0,
  badgeVerifying: 0xc98a2e,
  badgeDone: 0x4f7a3a,
  badgeFailed: 0xb44637,
  badgeInk: '#fbf3df',
} as const;

// CJK-safe font stack reused across every canvas Text object.
export const CJK_FONT = 'PingFang SC, Hiragino Sans GB, Microsoft YaHei, sans-serif';

// Scene keys — one registry so launch/start calls never drift on a typo.
export const SCENE = {
  boot: 'BootScene',
  preloader: 'PreloaderScene',
  hall: 'HallScene',
  ui: 'UIScene',
  board: 'TaskBoardScene',
} as const;
