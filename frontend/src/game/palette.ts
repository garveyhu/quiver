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

  // --- 设置账本 (SettingsScene, P3) — an open ledger book ---
  ledgerScrim: 0x1c130c, // dim wash over the slept world behind the book
  ledgerCover: 0x4a2d16, // the book's leather cover / outer edge
  ledgerCoverEdge: 0x2c1a0c, // cover shadow line
  ledgerSpine: 0x35200f, // centre spine the two pages meet at
  ledgerPage: 0xf3e6c8, // warm parchment page face
  ledgerPageEdge: 0xd8c39a, // page edge / rule lines
  ledgerPageShade: 0xe6d4ac, // inner-margin shade near the spine
  ledgerInk: '#3a2e25', // primary written ink
  ledgerInkDim: '#7a6651', // secondary / hint ink
  ledgerRule: 0xcdb88c, // faint horizontal rule under a field row
  // tabs (设置分组) — leather page-markers down the side
  ledgerTab: 0x6b4424, // an inactive tab
  ledgerTabActive: 0x9c6b3f, // the open tab (lighter, "lifted")
  ledgerTabInk: '#f6ead0',
  ledgerTabInkActive: '#3a2e25',
  // carved wooden lever (boolean / enum toggle)
  leverTrack: 0xcdb88c, // the slot the lever rides in
  leverTrackEdge: 0xa98a5c,
  leverKnob: 0x6b4424, // the carved handle
  leverKnobEdge: 0x422914,
  leverInk: '#3a2e25',
  leverInkDim: '#9a8466',
  // brass knob stepper (◀ n ▶)
  knobArrow: 0x6b4424,
  knobArrowDisabled: 0xc3b291,
  knobInk: '#3a2e25',
  knobWell: 0xe6d4ac, // the recessed value well
  knobWellEdge: 0xcdb88c,
  // a click-to-edit text/number cell (summons the IME overlay)
  ledgerCell: 0xfaf2dd,
  ledgerCellEdge: 0xcdb88c,
  ledgerCellInk: '#3a2e25',
  ledgerCellPlaceholder: '#a8997d',
  // save chip (记录中… / 已记录)
  ledgerSaveSaving: 0xc98a2e,
  ledgerSaveSaved: 0x4f7a3a,
  ledgerSaveError: 0xb44637,
  ledgerSaveInk: '#fbf3df',

  // --- 委托档案库 (ArchiveScene, P4) — a wall of bookshelves ---
  archiveScrim: 0x1c130c, // dim wash over the slept world behind the shelf
  shelfWoodDark: 0x3a2412, // carved cabinet frame / shadow
  shelfWood: 0x5e3a1e, // shelf plank face
  shelfWoodLight: 0x7a4f2e, // shelf top sheen
  shelfBack: 0x2c1a0c, // recessed shelf interior the books stand against
  // a run-book spine. The cloth band colour = run status; the body is leather.
  bookBody: 0x8a5a30,
  bookBodyEdge: 0x3a2412,
  bookPaper: 0xf3e6c8, // the page block peeking out the top of the spine
  bookBandInk: '#fbf3df', // title ink on the cloth band
  bookInk: '#f6ead0', // title ink on the leather spine
  bookInkDim: '#e6d4ac',
  // search field + filter dials chrome.
  archiveField: 0xf3e6c8, // the parchment search cell
  archiveFieldEdge: 0xcdb88c,
  archiveFieldInk: '#3a2e25',
  archiveFieldPlaceholder: '#a8997d',
  archiveDial: 0x6b4424, // a filter dial (status / project) face
  archiveDialEdge: 0x422914,
  archiveDialActive: 0x9c6b3f, // the dial when a non-default value is selected
  archiveDialInk: '#f6ead0',
  archiveCountInk: '#e6d4ac',

  // --- 委托卷轴 (LogbookScene, P4) — an unrolled parchment scroll ---
  scrollScrim: 0x140d07, // deeper wash — the scroll is a focused reading mode
  scrollRoller: 0x4a2d16, // the wooden roller bars top + bottom
  scrollRollerCap: 0x6b4424, // the roller end caps
  scrollParchment: 0xf3e6c8, // the unrolled parchment field
  scrollParchmentEdge: 0xd8c39a,
  scrollParchmentShade: 0xe6d4ac, // soft inner shadow at the rolled edges
  scrollInk: '#3a2e25', // header ink on the parchment
  scrollInkDim: '#7a6651',
  // the transcript reading well (a clean inset surface; the two-layer rule —
  // diegetic scroll frame, high-density monospace content inside).
  transcriptWell: 0xfaf6ea,
  transcriptWellEdge: 0xcdb88c,
  transcriptInk: '#2c241d', // monospace body ink
  transcriptInkDim: '#8a7a64', // timestamps / secondary
  transcriptOutput: '#4a5a3a', // output_chunk verbatim text
  transcriptError: '#b44637', // error lines
  // per-kind row accent stripe down the transcript gutter.
  kindWorker: 0x4f7a3a,
  kindTool: 0x2f6fb0,
  kindOutput: 0x8a7a5a,
  kindResult: 0xc98a2e,
  kindError: 0xb44637,
  kindFinished: 0x6b4f9a,
  // 回放 call-to-action.
  replayBtn: 0x4f7a3a,
  replayBtnInk: '#fbf3df',
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
  settings: 'SettingsScene',
  archive: 'ArchiveScene',
  logbook: 'LogbookScene',
} as const;
