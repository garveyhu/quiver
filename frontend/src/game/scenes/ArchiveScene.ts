import Phaser from 'phaser';
import type ScrollablePanel from 'phaser4-rex-plugins/templates/ui/scrollablepanel/ScrollablePanel';
import type Sizer from 'phaser4-rex-plugins/templates/ui/sizer/Sizer';
import type RexUIPlugin from 'phaser4-rex-plugins/templates/ui/ui-plugin';
import { EventBus, BUS, type TaskRecord, type ArchiveMeta } from '@/game/EventBus';
import { requestTextInput } from '@/game/requestTextInput';
import { PALETTE, CJK_FONT, SCENE } from '@/game/palette';
import { fadeIn, fadeOutThen } from '@/game/transition';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import { play } from '@/utils/sound';

const ALL = '__all__';

// One run-book spine on the shelf.
const BOOK = {
  width: 540,
  height: 84,
  gap: 12,
  radius: 8,
  bandW: 10, // status cloth band down the left edge of the spine
  padX: 20,
  padY: 12,
} as const;

// Per-status cloth-band fill on a run-book's spine (the §1 status colour),
// keyed off the Rust task.status vocabulary — mirrors the board's badgeColor.
function statusColor(status: string): number {
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

function costText(cost: number | null): string {
  return cost === null ? STR.archiveCostUnknown : `${STR.archiveCostPrefix}${cost.toFixed(2)}`;
}

function projectName(project: string): string {
  const parts = project.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? project;
}

function durationHint(record: TaskRecord): string {
  const ms = Math.max(0, record.updatedAt - record.createdAt);
  if (ms <= 0) return '';
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(0)}s`;
  const m = Math.floor(s / 60);
  return `${m}m${Math.round(s - m * 60)}s`;
}

/**
 * The in-world 委托档案库 (P4). Clicking the Hall's bookshelf hotspot sleeps the
 * Hall (the world stays warm, not stopped) and launches this scene: a dim wash, a
 * carved timber archive cabinet, and one run-book spine per past run in a rexUI
 * ScrollablePanel — status cloth-band + prompt title + cost/duration.
 *
 * A parchment search cell (summoning the IME-safe React overlay) and two filter
 * dials (status / project, cycled in-world) narrow the shelf client-side over the
 * records useArchive already loaded — the SAME search/filter contract the old
 * React ArchiveView had, just diegetic. Clicking a book opens the LogbookScene.
 *
 * All archive data flows over the EventBus — the scene never touches IPC. It
 * subscribes in `create()` and tears every listener down on `shutdown`.
 */
export class ArchiveScene extends Phaser.Scene {
  declare rexUI: RexUIPlugin;

  private scrim?: Phaser.GameObjects.Rectangle;
  // an invisible interactive backstop over the cabinet so a stray pointerup left
  // behind when the IME overlay closes never falls through to the scrim → close.
  private backstop?: Phaser.GameObjects.Rectangle;
  private frame?: Phaser.GameObjects.Container;
  private controls?: Phaser.GameObjects.Container;
  private panel?: ScrollablePanel;
  private listSizer?: Sizer;
  private emptyText?: Phaser.GameObjects.Text;
  private backBtn?: Phaser.GameObjects.Container;
  private cardMaskShape?: Phaser.GameObjects.Graphics;
  private cardMask?: Phaser.Display.Masks.GeometryMask;

  private records: TaskRecord[] = [];
  private meta: ArchiveMeta = { error: null };

  // client-side search/filter state (mirrors the old ArchiveView).
  private query = '';
  private statusFilter = ALL;
  private projectFilter = ALL;
  private searchOpen = false;

  // book layout geometry, recomputed on (re)layout so books place correctly.
  private frameRect = { x: 0, y: 0, w: 0, h: 0 };

  constructor() {
    super(SCENE.archive);
  }

  create(): void {
    // Phaser reuses the scene instance across stop→start, so clear every stale
    // GameObject handle before re-layout (the §6 scene-recreate footgun).
    this.resetState();

    this.layoutChrome();
    this.scale.on('resize', this.layoutChrome, this);

    EventBus.on(BUS.archiveRecords, this.onRecords, this);
    EventBus.on(BUS.archiveMeta, this.onMeta, this);
    // Ask the bridge to flush the current archive snapshot now that we're up.
    EventBus.emit(BUS.sceneReady);

    // Dolly to the shelf with a camera fade; fade back in when a Logbook scroll
    // (opened from a book) closes and wakes us (the WAKE seam, like the hall).
    fadeIn(this, PALETTE.archiveScrim);
    this.events.on(Phaser.Scenes.Events.WAKE, this.onWake, this);

    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);

    if (import.meta.env.DEV) {
      const w = window as unknown as { __archiveScene?: unknown; __archiveNav?: unknown };
      w.__archiveScene = this;
      w.__archiveNav = {
        // open the first currently-filtered book; returns its taskId or null.
        openFirstBook: (): string | null => {
          const first = this.filteredRecords()[0];
          if (!first) return null;
          this.openBook(first);
          return first.id;
        },
        setQuery: (q: string): number => {
          this.query = q;
          this.rebuildControls();
          this.renderBooks();
          return this.filteredRecords().length;
        },
        visibleCount: (): number => this.filteredRecords().length,
        recordCount: (): number => this.records.length,
      };
    }
  }

  private resetState(): void {
    this.scrim = undefined;
    this.backstop = undefined;
    this.frame = undefined;
    this.controls = undefined;
    this.panel = undefined;
    this.listSizer = undefined;
    this.emptyText = undefined;
    this.backBtn = undefined;
    this.cardMaskShape = undefined;
    this.cardMask = undefined;
    this.query = '';
    this.statusFilter = ALL;
    this.projectFilter = ALL;
    this.searchOpen = false;
  }

  private teardown(): void {
    EventBus.off(BUS.archiveRecords, this.onRecords, this);
    EventBus.off(BUS.archiveMeta, this.onMeta, this);
    this.scale.off('resize', this.layoutChrome, this);
    this.events.off(Phaser.Scenes.Events.WAKE, this.onWake, this);
  }

  private onWake(): void {
    fadeIn(this, PALETTE.archiveScrim);
  }

  private close(): void {
    play('close');
    fadeOutThen(
      this,
      () => {
        this.scene.stop(SCENE.archive);
        // Wake the world back up (it was slept, not stopped, so it stays warm).
        this.scene.wake(SCENE.hall);
      },
      PALETTE.archiveScrim,
    );
  }

  // Pull a run off the shelf → unroll its Logbook scroll. The archive sleeps (it
  // stays warm) and the LogbookScene launches over it; closing the scroll wakes
  // the archive back here (the WAKE handler fades it back in).
  private openBook(record: TaskRecord): void {
    play('open');
    fadeOutThen(
      this,
      () => {
        this.scene.sleep(SCENE.archive);
        this.cameras.main.resetFX();
        this.scene.launch(SCENE.logbook, { record });
      },
      PALETTE.archiveScrim,
    );
  }

  // --- bus handlers -------------------------------------------------------

  private onRecords(records: TaskRecord[]): void {
    this.records = records;
    this.rebuildControls();
    this.renderBooks();
  }

  private onMeta(meta: ArchiveMeta): void {
    this.meta = meta;
    this.refreshEmptyState();
  }

  // --- chrome (frame / title / controls / scroll panel) -------------------

  private layoutChrome(): void {
    const { width, height } = this.scale;

    if (!this.scrim) {
      this.scrim = this.add
        .rectangle(0, 0, width, height, PALETTE.archiveScrim, 0.55)
        .setOrigin(0, 0)
        .setDepth(0)
        .setInteractive();
      this.scrim.on('pointerup', () => this.close());
    } else {
      this.scrim.setSize(width, height).setPosition(0, 0);
    }

    const frameW = Math.min(640, width - 80);
    const frameH = Math.min(660, height - 90);
    const cx = width / 2;
    const cy = height / 2 + 6;
    this.frameRect = { x: cx, y: cy, w: frameW, h: frameH };

    this.buildBackstop(cx, cy, frameW, frameH);
    this.buildFrame(cx, cy, frameW, frameH);
    this.buildBackButton(cx, cy, frameW, frameH);
    this.buildPanel(cx, cy, frameW, frameH);
    this.rebuildControls();
    this.renderBooks();
  }

  private buildBackstop(cx: number, cy: number, w: number, h: number): void {
    const padX = 24;
    const padY = 16;
    const bw = w + padX * 2;
    const bh = h + padY * 2;
    if (!this.backstop) {
      this.backstop = this.add.rectangle(cx, cy, bw, bh, 0x000000, 0).setDepth(5).setInteractive();
      this.backstop.on('pointerup', (p: Phaser.Input.Pointer) => p.event.stopPropagation());
    } else {
      this.backstop.setSize(bw, bh).setPosition(cx, cy);
    }
  }

  private buildFrame(cx: number, cy: number, w: number, h: number): void {
    this.frame?.destroy();
    const frame = this.add.container(cx, cy).setDepth(10);

    const shadow = this.add.graphics();
    shadow.fillStyle(0x000000, 0.35);
    shadow.fillRoundedRect(-w / 2 + 6, -h / 2 + 10, w, h, 20);

    // carved timber archive cabinet: dark border, plank face, light top sheen,
    // recessed back the run-book spines stand against.
    const g = this.add.graphics();
    g.fillStyle(PALETTE.shelfWoodDark, 1);
    g.fillRoundedRect(-w / 2, -h / 2, w, h, 20);
    g.fillStyle(PALETTE.shelfWood, 1);
    g.fillRoundedRect(-w / 2 + 10, -h / 2 + 10, w - 20, h - 20, 14);
    g.fillStyle(PALETTE.shelfWoodLight, 0.4);
    g.fillRoundedRect(-w / 2 + 10, -h / 2 + 10, w - 20, 12, { tl: 14, tr: 14, bl: 0, br: 0 });
    // recessed shelf interior (below the title + controls band).
    g.fillStyle(PALETTE.shelfBack, 1);
    g.fillRoundedRect(-w / 2 + 22, -h / 2 + 112, w - 44, h - 178, 8);

    const title = this.add
      .text(0, -h / 2 + 30, STR.archiveSceneTitle, {
        fontFamily: CJK_FONT,
        fontSize: '20px',
        color: PALETTE.hudInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);

    frame.add([shadow, g, title]);
    this.frame = frame;
  }

  private buildBackButton(cx: number, cy: number, w: number, h: number): void {
    this.backBtn?.destroy();
    this.backBtn = this.makeButton(
      cx - w / 2 + 70,
      cy - h / 2 + 30,
      STR.archiveSceneBack,
      PALETTE.shelfWoodDark,
      PALETTE.hudInk,
      () => this.close(),
    ).setDepth(30);
  }

  private buildPanel(cx: number, cy: number, w: number, h: number): void {
    this.panel?.destroy();
    const sizer = this.rexUI.add.sizer({ orientation: 'y', space: { item: BOOK.gap } });
    this.listSizer = sizer;

    const viewportW = w - 60;
    const viewportH = h - 196;
    const viewportY = cy + 48;

    // clip the (root-level) books to the recessed shelf interior.
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
        mouseWheelScroller: { focus: false, speed: 0.4 },
      })
      .setDepth(20)
      .layout() as unknown as ScrollablePanel;

    this.emptyText = this.add
      .text(cx, viewportY, '', {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color: PALETTE.bookInkDim,
        align: 'center',
        wordWrap: { width: viewportW - 40 },
      })
      .setOrigin(0.5, 0.5)
      .setDepth(25);
  }

  // --- search + filter controls band --------------------------------------

  // The distinct status / project values present in the data drive the filter
  // dials — no point cycling to a status that never occurred.
  private statusesPresent(): string[] {
    const set = new Set<string>();
    for (const r of this.records) set.add(r.status);
    return [...set];
  }

  private projectsPresent(): string[] {
    const set = new Set<string>();
    for (const r of this.records) set.add(r.project);
    return [...set];
  }

  private rebuildControls(): void {
    const { x: cx, y: cy, w, h } = this.frameRect;
    if (w === 0) return;
    this.controls?.destroy();
    const band = this.add.container(cx, cy - h / 2 + 74).setDepth(28);

    const innerW = w - 60;
    const left = -innerW / 2;

    // search cell (left, summons the IME overlay).
    const searchW = innerW * 0.42;
    const search = this.buildSearchCell(searchW);
    search.setPosition(left + searchW / 2, 0);
    band.add(search);

    // status dial.
    const statuses = this.statusesPresent();
    const statusDial = this.buildDial(
      this.statusFilter === ALL
        ? STR.archiveSceneFilterStatusAll
        : (TASK_STATUS_LABEL[this.statusFilter] ?? this.statusFilter),
      this.statusFilter !== ALL,
      () => {
        const cycle = [ALL, ...statuses];
        const i = cycle.indexOf(this.statusFilter);
        this.statusFilter = cycle[(i + 1) % cycle.length];
        this.rebuildControls();
        this.renderBooks();
      },
    );
    const dialW = innerW * 0.24;
    statusDial.setPosition(left + searchW + 12 + dialW / 2, 0);
    band.add(statusDial);

    // project dial.
    const projects = this.projectsPresent();
    const projectDial = this.buildDial(
      this.projectFilter === ALL ? STR.archiveSceneFilterProjectAll : projectName(this.projectFilter),
      this.projectFilter !== ALL,
      () => {
        const cycle = [ALL, ...projects];
        const i = cycle.indexOf(this.projectFilter);
        this.projectFilter = cycle[(i + 1) % cycle.length];
        this.rebuildControls();
        this.renderBooks();
      },
    );
    projectDial.setPosition(left + searchW + 12 + dialW + 12 + dialW / 2, 0);
    band.add(projectDial);

    // count chip, far right.
    const count = this.filteredRecords().length;
    const countText = this.add
      .text(
        innerW / 2,
        0,
        `${STR.archiveSceneCountPrefix}${count}${STR.archiveSceneCountSuffix}`,
        {
          fontFamily: CJK_FONT,
          fontSize: '13px',
          color: PALETTE.archiveCountInk,
        },
      )
      .setOrigin(1, 0.5);
    band.add(countText);

    this.controls = band;
  }

  private buildSearchCell(width: number): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const cellH = 34;
    const bg = this.add.graphics();
    bg.fillStyle(PALETTE.archiveField, 1);
    bg.fillRoundedRect(-width / 2, -cellH / 2, width, cellH, 8);
    bg.lineStyle(2, PALETTE.archiveFieldEdge, 1);
    bg.strokeRoundedRect(-width / 2, -cellH / 2, width, cellH, 8);
    c.add(bg);

    const display = this.query.trim() === '' ? STR.archiveSceneSearchHint : this.query;
    const isPlaceholder = this.query.trim() === '';
    const text = this.add
      .text(-width / 2 + 28, 0, display, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: isPlaceholder ? PALETTE.archiveFieldPlaceholder : PALETTE.archiveFieldInk,
        fontStyle: isPlaceholder ? 'italic' : 'normal',
      })
      .setOrigin(0, 0.5);
    text.setCrop(0, 0, width - 64, cellH);
    c.add(text);

    const glass = this.add
      .text(-width / 2 + 10, 0, '🔍', { fontFamily: CJK_FONT, fontSize: '13px' })
      .setOrigin(0.5, 0.5);
    c.add(glass);

    // a clear (✕) affordance when a query is active.
    if (!isPlaceholder) {
      const clear = this.add
        .text(width / 2 - 12, 0, '✕', {
          fontFamily: CJK_FONT,
          fontSize: '13px',
          color: PALETTE.archiveFieldPlaceholder,
        })
        .setOrigin(1, 0.5)
        .setInteractive({ useHandCursor: true });
      clear.on('pointerup', (p: Phaser.Input.Pointer) => {
        p.event.stopPropagation();
        this.query = '';
        this.rebuildControls();
        this.renderBooks();
      });
      c.add(clear);
    }

    const hit = this.add
      .zone(0, 0, width - (isPlaceholder ? 0 : 28), cellH)
      .setInteractive({ useHandCursor: true });
    hit.on('pointerup', () => void this.openSearch());
    hit.on('pointerover', () => c.setScale(1.02));
    hit.on('pointerout', () => c.setScale(1));
    c.add(hit);
    return c;
  }

  private buildDial(
    label: string,
    active: boolean,
    onClick: () => void,
  ): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const dialW = this.frameRect.w * 0.24 - 6;
    const dialH = 34;
    const bg = this.add.graphics();
    bg.fillStyle(active ? PALETTE.archiveDialActive : PALETTE.archiveDial, 1);
    bg.fillRoundedRect(-dialW / 2, -dialH / 2, dialW, dialH, 8);
    bg.lineStyle(2, PALETTE.archiveDialEdge, 1);
    bg.strokeRoundedRect(-dialW / 2, -dialH / 2, dialW, dialH, 8);
    c.add(bg);

    const text = this.add
      .text(0, 0, label, {
        fontFamily: CJK_FONT,
        fontSize: '13px',
        color: PALETTE.archiveDialInk,
        fontStyle: active ? 'bold' : 'normal',
      })
      .setOrigin(0.5, 0.5);
    text.setCrop(0, 0, dialW - 20, dialH);
    c.add(text);

    c.setSize(dialW, dialH);
    c.setInteractive({ useHandCursor: true });
    c.on('pointerup', () => {
      play('open');
      onClick();
    });
    c.on('pointerover', () => c.setScale(1.04));
    c.on('pointerout', () => c.setScale(1));
    return c;
  }

  // Summon the IME-safe React overlay to edit the search query.
  private async openSearch(): Promise<void> {
    if (this.searchOpen) return;
    this.searchOpen = true;
    const { width, height } = this.scale;
    const fieldW = Math.min(420, width - 80);
    const result = await requestTextInput({
      anchor: {
        x: width / 2 - fieldW / 2,
        y: height / 2 - 30,
        width: fieldW,
        height: 48,
      },
      value: this.query,
      multiline: false,
      placeholder: STR.archiveSceneSearchHint,
      submitLabel: STR.archiveSceneSearchSubmit,
    });
    this.searchOpen = false;
    // null = cancelled (keep current query); a string (incl. empty) replaces it.
    if (result !== null) this.query = result;
    this.rebuildControls();
    this.renderBooks();
  }

  // --- run-book spines ----------------------------------------------------

  // Client-side search + filter over the loaded records — the SAME predicate the
  // old React ArchiveView used (status / project exact match + prompt substring).
  private filteredRecords(): TaskRecord[] {
    const q = this.query.trim().toLowerCase();
    return this.records.filter(r => {
      if (this.statusFilter !== ALL && r.status !== this.statusFilter) return false;
      if (this.projectFilter !== ALL && r.project !== this.projectFilter) return false;
      if (q && !r.prompt.toLowerCase().includes(q)) return false;
      return true;
    });
  }

  private renderBooks(): void {
    if (!this.listSizer || !this.panel) return;

    this.listSizer.clear(true);
    const filtered = this.filteredRecords();
    for (const record of filtered) {
      const book = this.buildBook(record);
      this.listSizer.add(book, {
        expand: false,
        align: 'center',
        minWidth: BOOK.width,
        minHeight: BOOK.height,
      });
    }

    this.panel.layout();
    this.refreshEmptyState();
  }

  private refreshEmptyState(): void {
    if (!this.emptyText) return;
    if (this.meta.error) {
      this.emptyText.setText(this.meta.error).setVisible(true);
      return;
    }
    if (this.records.length === 0) {
      this.emptyText.setText(STR.archiveSceneEmpty).setVisible(true);
      return;
    }
    if (this.filteredRecords().length === 0) {
      this.emptyText.setText(STR.archiveSceneNoMatch).setVisible(true);
      return;
    }
    this.emptyText.setVisible(false);
  }

  // A run-book = a fixed-size container the Sizer lays out: a leather spine with
  // a status cloth band, the prompt as the title, and cost/project/time/duration
  // as the foot — kept clearly scannable (clarity > theme for dense data).
  private buildBook(record: TaskRecord): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    c.setSize(BOOK.width, BOOK.height);
    c.setDepth(22);
    if (this.cardMask) c.setMask(this.cardMask);

    const half = { w: BOOK.width / 2, h: BOOK.height / 2 };

    const bg = this.add.graphics();
    bg.fillStyle(PALETTE.bookBody, 1);
    bg.fillRoundedRect(-half.w, -half.h, BOOK.width, BOOK.height, BOOK.radius);
    bg.lineStyle(2, PALETTE.bookBodyEdge, 1);
    bg.strokeRoundedRect(-half.w, -half.h, BOOK.width, BOOK.height, BOOK.radius);
    // page block peeking out the right edge of the spine.
    bg.fillStyle(PALETTE.bookPaper, 1);
    bg.fillRoundedRect(half.w - 14, -half.h + 8, 8, BOOK.height - 16, 2);
    // status cloth band down the left edge.
    bg.fillStyle(statusColor(record.status), 1);
    bg.fillRoundedRect(-half.w, -half.h, BOOK.bandW, BOOK.height, {
      tl: BOOK.radius,
      tr: 0,
      bl: BOOK.radius,
      br: 0,
    });
    c.add(bg);

    const left = -half.w + BOOK.bandW + BOOK.padX;
    const top = -half.h + BOOK.padY;
    const innerW = BOOK.width - BOOK.bandW - BOOK.padX * 2 - 14;

    // status label + mode chip head.
    const statusLabel = TASK_STATUS_LABEL[record.status] ?? record.status;
    const modeLabel = record.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate;
    const head = this.add
      .text(left, top, `${statusLabel} · ${modeLabel} · ${projectName(record.project)}`, {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.bookInkDim,
      })
      .setOrigin(0, 0);
    head.setCrop(0, 0, innerW, 18);
    c.add(head);

    // prompt as the title (single clamped line).
    const prompt = this.add
      .text(left, top + 22, record.prompt, {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color: PALETTE.bookInk,
        fontStyle: 'bold',
        wordWrap: { width: innerW },
        maxLines: 1,
      })
      .setOrigin(0, 0);
    c.add(prompt);

    // cost + time + duration foot.
    const dur = durationHint(record);
    const foot =
      costText(record.costUsd) +
      `  ·  ${new Date(record.createdAt).toLocaleString()}` +
      (dur ? `  ·  ${dur}` : '') +
      (record.branch ? `  ·  ${STR.archiveBranchPrefix}${record.branch}` : '');
    const footText = this.add
      .text(left, half.h - BOOK.padY - 2, foot, {
        fontFamily: CJK_FONT,
        fontSize: '11px',
        color: PALETTE.bookInkDim,
        wordWrap: { width: innerW },
        maxLines: 1,
      })
      .setOrigin(0, 1);
    c.add(footText);

    const hit = new Phaser.Geom.Rectangle(-half.w, -half.h, BOOK.width, BOOK.height);
    c.setInteractive({ hitArea: hit, hitAreaCallback: Phaser.Geom.Rectangle.Contains, useHandCursor: true });
    c.on('pointerup', () => this.openBook(record));
    c.on('pointerover', () => c.setScale(1.01));
    c.on('pointerout', () => c.setScale(1));
    return c;
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
}
