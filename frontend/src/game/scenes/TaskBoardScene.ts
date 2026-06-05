import Phaser from 'phaser';
import type ScrollablePanel from 'phaser4-rex-plugins/templates/ui/scrollablepanel/ScrollablePanel';
import type Sizer from 'phaser4-rex-plugins/templates/ui/sizer/Sizer';
import type RexUIPlugin from 'phaser4-rex-plugins/templates/ui/ui-plugin';
import { EventBus, BUS, type TaskRecord, type BoardMeta } from '@/game/EventBus';
import { requestTextInput } from '@/game/requestTextInput';
import { PALETTE, CJK_FONT, SCENE } from '@/game/palette';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import { play } from '@/utils/sound';

// A queued card dropped above `targetId` lands strictly below the card above it
// and above the target. The scheduler re-sorts by position, so any value between
// neighbours works — hand it the target's position minus a small epsilon. This
// is the SAME fractional-position contract the old React TaskBoard used.
const POSITION_EPSILON = 0.5;

const CARD = {
  width: 460,
  height: 96,
  gap: 14,
  radius: 12,
  padX: 18,
  padY: 14,
} as const;

// Per-status badge fill, keyed off the Rust task.status vocabulary.
function badgeColor(status: string): number {
  switch (status) {
    case 'queued':
      return PALETTE.badgeQueued;
    case 'running':
      return PALETTE.badgeRunning;
    case 'verifying':
      return PALETTE.badgeVerifying;
    case 'verified':
    case 'done':
      return PALETTE.badgeDone;
    default:
      return PALETTE.badgeFailed;
  }
}

function isQueued(status: string): boolean {
  return status === 'queued';
}

function costText(cost: number | null): string {
  return cost === null ? STR.taskCostUnknown : `${STR.taskCostPrefix}${cost.toFixed(2)}`;
}

interface CardNode {
  task: TaskRecord;
  container: Phaser.GameObjects.Container;
  // The queued cards' canvas-space order at drag start, for drop-target search.
  homeY: number;
}

/**
 * The in-world 任务公告板 (P2). Clicking the Hall's board hotspot sleeps the Hall
 * (the world stays warm, not stopped) and launches this scene: a dim wash, a
 * carved timber notice-board, and pinned parchment commission cards in a rexUI
 * ScrollablePanel.
 *
 * Each card is a scene object (not an HTML form card): a status badge, a prompt
 * summary, cost, and branch, with a red pin. Queued cards drag-to-reorder (the
 * unchanged fractional-position contract, routed to useTaskBoard.reorder via the
 * bus) and carry a 撤回 (cancel) affordance. The "＋ 钉新委托" button summons the
 * IME-safe React text overlay to write a prompt, then enqueues it.
 *
 * All board data + actions flow over the EventBus — the scene never touches IPC.
 * It subscribes in `create()` and tears every listener down on `shutdown`.
 */
export class TaskBoardScene extends Phaser.Scene {
  // rexUI is registered as a scene plugin (mapping 'rexUI' in PhaserGame).
  declare rexUI: RexUIPlugin;

  private scrim?: Phaser.GameObjects.Rectangle;
  private frame?: Phaser.GameObjects.Container;
  private panel?: ScrollablePanel;
  private listSizer?: Sizer;
  private emptyText?: Phaser.GameObjects.Text;
  private backBtn?: Phaser.GameObjects.Container;
  private pinBtn?: Phaser.GameObjects.Container;
  // a fixed geometry mask over the scroll viewport: rexUI lays the cards out at
  // the scene root (not inside its own masked layer), so we clip them ourselves.
  private cardMaskShape?: Phaser.GameObjects.Graphics;
  private cardMask?: Phaser.Display.Masks.GeometryMask;

  private tasks: TaskRecord[] = [];
  private meta: BoardMeta = { projectPath: null, error: null };
  private cards = new Map<string, CardNode>();

  private pinPromptOpen = false;

  constructor() {
    super(SCENE.board);
  }

  create(): void {
    // Phaser REUSES the scene instance across stop→start, so every GameObject
    // field from a prior open is now a stale handle to a destroyed object. Clear
    // them before re-layout, or `this.scrim?.setSize()` hits a destroyed object
    // (the §6 scene-recreate footgun, the input-overlay sibling of listener leaks).
    this.resetState();

    this.layoutChrome();
    this.scale.on('resize', this.layoutChrome, this);

    EventBus.on(BUS.boardTasks, this.onTasks, this);
    EventBus.on(BUS.boardMeta, this.onMeta, this);
    // Ask the bridge to flush the current board snapshot now that we're up.
    EventBus.emit(BUS.sceneReady);

    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);

    if (import.meta.env.DEV) {
      (window as unknown as { __boardScene?: unknown }).__boardScene = this;
    }
  }

  private resetState(): void {
    this.scrim = undefined;
    this.frame = undefined;
    this.panel = undefined;
    this.listSizer = undefined;
    this.emptyText = undefined;
    this.backBtn = undefined;
    this.pinBtn = undefined;
    this.cardMaskShape = undefined;
    this.cardMask = undefined;
    this.cards.clear();
    this.pinPromptOpen = false;
  }

  private teardown(): void {
    EventBus.off(BUS.boardTasks, this.onTasks, this);
    EventBus.off(BUS.boardMeta, this.onMeta, this);
    this.scale.off('resize', this.layoutChrome, this);
  }

  private close(): void {
    play('close');
    this.scene.stop(SCENE.board);
    // Wake the world back up (it was slept, not stopped, so it stays warm).
    this.scene.wake(SCENE.hall);
  }

  // --- bus handlers -------------------------------------------------------

  private onTasks(tasks: TaskRecord[]): void {
    this.tasks = tasks;
    this.renderCards();
  }

  private onMeta(meta: BoardMeta): void {
    this.meta = meta;
    this.refreshEmptyState();
  }

  // --- chrome (frame / title / buttons / scroll panel) --------------------

  private layoutChrome(): void {
    const { width, height } = this.scale;

    // dim wash over the sleeping world.
    if (!this.scrim) {
      this.scrim = this.add
        .rectangle(0, 0, width, height, PALETTE.boardScrim, 0.55)
        .setOrigin(0, 0)
        .setDepth(0)
        .setInteractive();
      // a click on the wash (outside the board) closes back to the world.
      this.scrim.on('pointerup', () => this.close());
    } else {
      this.scrim.setSize(width, height).setPosition(0, 0);
    }

    const boardW = Math.min(560, width - 80);
    const boardH = Math.min(620, height - 110);
    const cx = width / 2;
    const cy = height / 2 + 10;

    this.buildFrame(cx, cy, boardW, boardH);
    this.buildButtons(cx, cy, boardW, boardH);
    this.buildPanel(cx, cy, boardW, boardH);
  }

  private buildFrame(cx: number, cy: number, w: number, h: number): void {
    this.frame?.destroy();
    const frame = this.add.container(cx, cy).setDepth(10);

    const shadow = this.add.graphics();
    shadow.fillStyle(0x000000, 0.35);
    shadow.fillRoundedRect(-w / 2 + 6, -h / 2 + 10, w, h, 22);

    // carved timber frame: dark border, plank face, light top sheen.
    const g = this.add.graphics();
    g.fillStyle(PALETTE.boardWoodDark, 1);
    g.fillRoundedRect(-w / 2, -h / 2, w, h, 22);
    g.fillStyle(PALETTE.boardWood, 1);
    g.fillRoundedRect(-w / 2 + 10, -h / 2 + 10, w - 20, h - 20, 16);
    g.fillStyle(PALETTE.boardWoodLight, 0.5);
    g.fillRoundedRect(-w / 2 + 10, -h / 2 + 10, w - 20, 12, { tl: 16, tr: 16, bl: 0, br: 0 });
    // cork backing the cards are pinned to.
    g.fillStyle(PALETTE.boardCork, 1);
    g.fillRoundedRect(-w / 2 + 22, -h / 2 + 58, w - 44, h - 124, 10);
    g.lineStyle(2, PALETTE.boardCorkEdge, 0.8);
    g.strokeRoundedRect(-w / 2 + 22, -h / 2 + 58, w - 44, h - 124, 10);

    const title = this.add
      .text(0, -h / 2 + 30, STR.boardSceneTitle, {
        fontFamily: CJK_FONT,
        fontSize: '20px',
        color: PALETTE.hudInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);

    frame.add([shadow, g, title]);
    this.frame = frame;
  }

  private buildButtons(cx: number, cy: number, w: number, h: number): void {
    this.backBtn?.destroy();
    this.pinBtn?.destroy();

    // back button, top-left inside the frame.
    this.backBtn = this.makeButton(
      cx - w / 2 + 70,
      cy - h / 2 + 30,
      STR.boardSceneBack,
      PALETTE.boardWoodDark,
      PALETTE.hudInk,
      () => this.close(),
    ).setDepth(30);

    // "钉新委托" call-to-action, bottom-centre.
    this.pinBtn = this.makeButton(
      cx,
      cy + h / 2 - 34,
      STR.boardScenePin,
      PALETTE.pinBtn,
      PALETTE.pinBtnInk,
      () => void this.openNewCommission(),
    ).setDepth(30);
  }

  private makeButton(
    x: number,
    y: number,
    label: string,
    fill: number,
    ink: string,
    onClick: () => void,
  ): Phaser.GameObjects.Container {
    const c = this.add.container(x, y);
    const text = this.add
      .text(0, 0, label, {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color: ink,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    const padX = 16;
    const padY = 9;
    const bw = text.width + padX * 2;
    const bh = text.height + padY * 2;
    const bg = this.add.graphics();
    bg.fillStyle(fill, 1);
    bg.fillRoundedRect(-bw / 2, -bh / 2, bw, bh, 9);
    bg.lineStyle(2, 0x000000, 0.18);
    bg.strokeRoundedRect(-bw / 2, -bh / 2, bw, bh, 9);
    c.add([bg, text]);
    c.setSize(bw, bh);
    c.setInteractive({ useHandCursor: true });
    c.on('pointerup', onClick);
    c.on('pointerover', () => c.setScale(1.05));
    c.on('pointerout', () => c.setScale(1));
    return c;
  }

  private buildPanel(cx: number, cy: number, w: number, h: number): void {
    this.panel?.destroy();
    const sizer = this.rexUI.add.sizer({ orientation: 'y', space: { item: CARD.gap } });
    this.listSizer = sizer;

    const viewportW = w - 60;
    const viewportH = h - 150;
    const viewportY = cy + 6;

    // clip the (root-level) cards to the cork viewport so long lists don't spill.
    this.cardMaskShape?.destroy();
    const maskShape = this.add.graphics().setVisible(false);
    maskShape.fillStyle(0xffffff, 1);
    maskShape.fillRect(cx - viewportW / 2, viewportY - viewportH / 2, viewportW, viewportH);
    this.cardMaskShape = maskShape;
    this.cardMask = maskShape.createGeometryMask();

    this.panel = this.rexUI.add
      .scrollablePanel({
        x: cx,
        y: viewportY,
        width: viewportW,
        height: viewportH,
        scrollMode: 'y',
        panel: { child: sizer, mask: { padding: 2 } },
        space: { panel: 4 },
        // mouse-wheel scroll only; cards own their drag so reorder isn't stolen.
        mouseWheelScroller: { focus: false, speed: 0.4 },
      })
      .setDepth(20)
      .layout() as unknown as ScrollablePanel;

    this.emptyText = this.add
      .text(cx, cy + 6, '', {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color: PALETTE.hudInk,
        align: 'center',
        wordWrap: { width: viewportW - 40 },
      })
      .setOrigin(0.5, 0.5)
      .setDepth(25);

    this.renderCards();
  }

  // --- cards --------------------------------------------------------------

  private renderCards(): void {
    if (!this.listSizer || !this.panel) return;

    // Rebuild the list from scratch: simplest correct approach given live status
    // churn (a card flips queued→running→done and gains/loses drag + cancel).
    this.listSizer.clear(true);
    this.cards.clear();

    for (const task of this.tasks) {
      const card = this.buildCard(task);
      this.cards.set(task.id, { task, container: card.container, homeY: 0 });
      // A bare Phaser container has no intrinsic size rexUI can measure, so give
      // the Sizer the card's fixed footprint explicitly — else the row collapses.
      this.listSizer.add(card.sizerChild, {
        expand: false,
        align: 'center',
        minWidth: CARD.width,
        minHeight: CARD.height,
      });
    }

    this.panel.layout();
    this.refreshEmptyState();
  }

  private refreshEmptyState(): void {
    if (!this.emptyText) return;
    if (this.tasks.length > 0) {
      this.emptyText.setVisible(false);
      return;
    }
    const msg = this.meta.projectPath ? STR.boardSceneEmpty : STR.boardSceneNoProject;
    this.emptyText.setText(msg).setVisible(true);
  }

  // A card = a fixed-size rexUI child (so the Sizer lays it out) wrapping a
  // Phaser container we draw + make interactive ourselves.
  private buildCard(task: TaskRecord): {
    container: Phaser.GameObjects.Container;
    sizerChild: Phaser.GameObjects.Container;
  } {
    const queued = isQueued(task.status);
    const c = this.add.container(0, 0);
    c.setSize(CARD.width, CARD.height);
    // rexUI lays this container out in world space but leaves it at the scene
    // root (depth 0), where the frame's cork (depth 10) would cover it — promote
    // it above the cork, below the buttons (depth 30), and clip it to the cork
    // viewport so a long scrolled list doesn't spill past the frame.
    c.setDepth(22);
    if (this.cardMask) c.setMask(this.cardMask);

    const bg = this.add.graphics();
    bg.fillStyle(PALETTE.cardParchment, 1);
    bg.fillRoundedRect(-CARD.width / 2, -CARD.height / 2, CARD.width, CARD.height, CARD.radius);
    bg.lineStyle(2, PALETTE.cardParchmentEdge, 1);
    bg.strokeRoundedRect(-CARD.width / 2, -CARD.height / 2, CARD.width, CARD.height, CARD.radius);

    // red pin, top-centre.
    const pin = this.add.graphics();
    pin.fillStyle(PALETTE.cardPin, 1);
    pin.fillCircle(0, -CARD.height / 2 + 4, 7);
    pin.fillStyle(PALETTE.cardPinShine, 1);
    pin.fillCircle(-2, -CARD.height / 2 + 2, 2.5);

    const left = -CARD.width / 2 + CARD.padX;
    const top = -CARD.height / 2 + CARD.padY;

    // status badge.
    const statusLabel = TASK_STATUS_LABEL[task.status] ?? task.status;
    const badge = this.makeBadge(statusLabel, badgeColor(task.status));
    badge.setPosition(left + badge.width / 2, top + 8);

    // mode chip.
    const modeLabel = task.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate;
    const modeText = this.add
      .text(left + badge.width + 12, top + 1, modeLabel, {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.cardInkDim,
      })
      .setOrigin(0, 0);

    // prompt summary (single clamped line).
    const prompt = this.add
      .text(left, top + 26, task.prompt, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: PALETTE.cardInk,
        wordWrap: { width: CARD.width - CARD.padX * 2 - 70 },
        maxLines: 2,
      })
      .setOrigin(0, 0);

    // cost + branch foot.
    const foot = costText(task.costUsd) + (task.branch ? `  ·  ${STR.taskBranchPrefix}${task.branch}` : '');
    const footText = this.add
      .text(left, CARD.height / 2 - CARD.padY - 4, foot, {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.cardInkDim,
        wordWrap: { width: CARD.width - CARD.padX * 2 },
        maxLines: 1,
      })
      .setOrigin(0, 1);

    c.add([bg, pin, badge, modeText, prompt, footText]);

    // cancel (撤回) — only queued commissions can be pulled.
    if (queued) {
      const cancel = this.makeCancel();
      cancel.setPosition(CARD.width / 2 - 22, -CARD.height / 2 + 18);
      cancel.on('pointerup', (p: Phaser.Input.Pointer) => {
        p.event.stopPropagation();
        EventBus.emit(BUS.boardCancel, { id: task.id });
      });
      c.add(cancel);
    }

    // Drag-to-reorder for queued cards only.
    if (queued) {
      const hit = new Phaser.Geom.Rectangle(
        -CARD.width / 2,
        -CARD.height / 2,
        CARD.width,
        CARD.height,
      );
      c.setInteractive({
        hitArea: hit,
        hitAreaCallback: Phaser.Geom.Rectangle.Contains,
        draggable: true,
        useHandCursor: true,
      });
      this.wireCardDrag(c, task);
    }

    // The rexUI Sizer needs a child with a layout size; the card container IS
    // that child (fixed size), so return it directly.
    return { container: c, sizerChild: c };
  }

  private makeBadge(label: string, fill: number): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const text = this.add
      .text(0, 0, label, {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.badgeInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    const padX = 8;
    const padY = 3;
    const bw = text.width + padX * 2;
    const bh = text.height + padY * 2;
    const bg = this.add.graphics();
    bg.fillStyle(fill, 1);
    bg.fillRoundedRect(-bw / 2, -bh / 2, bw, bh, 6);
    c.add([bg, text]);
    c.setSize(bw, bh);
    return c;
  }

  private makeCancel(): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const bg = this.add.circle(0, 0, 11, PALETTE.cancelBtn, 1);
    const x = this.add
      .text(0, 0, '✕', {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.cancelBtnInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    c.add([bg, x]);
    c.setSize(22, 22);
    c.setInteractive(new Phaser.Geom.Circle(0, 0, 11), Phaser.Geom.Circle.Contains);
    c.on('pointerover', () => c.setScale(1.15));
    c.on('pointerout', () => c.setScale(1));
    return c;
  }

  // --- drag-reorder (fractional-position contract, unchanged) -------------

  private wireCardDrag(card: Phaser.GameObjects.Container, task: TaskRecord): void {
    let startX = 0;
    let startY = 0;

    card.on('dragstart', () => {
      startX = card.x;
      startY = card.y;
      card.setScale(1.04);
      card.setAlpha(0.92);
      // snapshot every queued card's current world Y for drop-target search.
      for (const node of this.cards.values()) {
        node.homeY = node.container.getWorldTransformMatrix().ty;
      }
    });

    // `dragX`/`dragY` are the pointer-tracked target position in the card's
    // parent space; nudge vertically only (the Sizer owns X), within the mask.
    card.on('drag', (_p: Phaser.Input.Pointer, _dragX: number, dragY: number) => {
      card.y = dragY;
    });

    card.on('dragend', (p: Phaser.Input.Pointer) => {
      card.setScale(1);
      card.setAlpha(1);
      card.setPosition(startX, startY);

      const targetId = this.dropTarget(task.id, p.worldY);
      if (targetId) {
        const target = this.tasks.find(t => t.id === targetId);
        if (target) {
          // Drop just ahead of the target card in board order — identical to the
          // old React board's epsilon contract; the scheduler re-sorts by position.
          EventBus.emit(BUS.boardReorder, {
            id: task.id,
            position: target.position - POSITION_EPSILON,
          });
        }
      }
    });
  }

  // Which queued card's slot did we drop onto? Nearest queued card by world Y,
  // excluding the dragged one.
  private dropTarget(draggedId: string, worldY: number): string | null {
    let best: string | null = null;
    let bestDist = Infinity;
    for (const task of this.tasks) {
      if (!isQueued(task.status) || task.id === draggedId) continue;
      const node = this.cards.get(task.id);
      if (!node) continue;
      const d = Math.abs(node.homeY - worldY);
      if (d < bestDist) {
        bestDist = d;
        best = task.id;
      }
    }
    return best;
  }

  // --- 钉新委托 (summon IME-safe overlay → enqueue) -----------------------

  private async openNewCommission(): Promise<void> {
    if (this.pinPromptOpen) return;
    if (!this.meta.projectPath) {
      this.refreshEmptyState();
      return;
    }
    this.pinPromptOpen = true;
    const { width, height } = this.scale;
    const fieldW = Math.min(440, width - 80);
    const result = await requestTextInput({
      anchor: {
        x: width / 2 - fieldW / 2,
        y: height / 2 - 70,
        width: fieldW,
        height: 140,
      },
      value: '',
      multiline: true,
      placeholder: STR.taskPlaceholder,
      submitLabel: STR.textInputSubmit,
    });
    this.pinPromptOpen = false;
    if (result && result.length > 0) {
      EventBus.emit(BUS.boardEnqueue, { prompt: result });
    }
  }
}
