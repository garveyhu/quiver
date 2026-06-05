import Phaser from 'phaser';
import { EventBus, BUS, type TaskRecord, type StoredEvent } from '@/game/EventBus';
import { requestLogbook } from '@/game/requestLogbook';
import { PALETTE, CJK_FONT, SCENE } from '@/game/palette';
import { fadeIn, fadeOutThen } from '@/game/transition';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import { play } from '@/utils/sound';
import type { AgentEvent } from '@/types/agentEvent.types';
import {
  parseStoredPayload,
  transcriptLine,
  clockStamp,
  formatCost,
} from '@/utils/eventFormat';

// A monospace stack so the transcript stays a clean, scan-readable log (the §6
// two-layer rule: the scroll is a diegetic frame, the content is NOT handwritten).
const MONO_FONT = 'SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace';

// Where a LogbookScene is opened from: ArchiveScene passes a full TaskRecord; a
// click on a Hall archer passes a taskId (and whatever record the bridge has).
interface LogbookInit {
  record?: TaskRecord;
  taskId?: string;
  /** Which scene to wake on close — the archive (came from a book) or the hall
   * (came from an archer). Defaults to the hall. */
  from?: 'archive' | 'hall';
}

interface ParsedEvent {
  seq: number;
  tsMs: number;
  event: AgentEvent;
}

function projectName(project: string): string {
  const parts = project.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? project;
}

function durationLabel(ms: number): string {
  if (ms <= 0) return '—';
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${Math.round(s - m * 60)}s`;
}

// per-kind accent stripe down the transcript gutter.
function kindAccent(kind: AgentEvent['kind']): number {
  switch (kind) {
    case 'worker_started':
      return PALETTE.kindWorker;
    case 'tool_use':
      return PALETTE.kindTool;
    case 'output_chunk':
      return PALETTE.kindOutput;
    case 'result':
      return PALETTE.kindResult;
    case 'error':
      return PALETTE.kindError;
    case 'finished':
      return PALETTE.kindFinished;
  }
}

/**
 * The in-world 委托卷轴 / Logbook (P4): an unrolled parchment scroll that frames
 * ONE run's complete I/O. Opened two ways (§1 / Mockup C):
 *   ① from the ArchiveScene by pulling a book off the shelf;
 *   ② from the HallScene by clicking a working / finished archer.
 *
 * The §6 TWO-LAYER RULE is the whole point here: the SCROLL (wooden rollers,
 * parchment field) is the low-density diegetic frame, but the TRANSCRIPT inside
 * it is a high-density, monospace, scan-readable list — prompt → each tool_use →
 * output → result → finished, with clocks / cost / turns. It is never rendered as
 * "handwritten scroll script".
 *
 * A "▶ 回放" button hands the run's stored log back to the workshop (useReplay via
 * the bus): the scroll rolls up, the Hall wakes, and the archer re-enacts the run
 * by pose. Event data is loaded over the bus (requestLogbook → useArchive.loadEvents,
 * the only IPC caller) — the scene never touches `invoke`. It subscribes nothing
 * permanent; the one-shot load channel tears itself down, and the resize handler
 * is removed on `shutdown`.
 */
export class LogbookScene extends Phaser.Scene {
  private scrim?: Phaser.GameObjects.Rectangle;
  private backstop?: Phaser.GameObjects.Rectangle;
  private frame?: Phaser.GameObjects.Container;
  private header?: Phaser.GameObjects.Container;
  private statusText?: Phaser.GameObjects.Text;
  private backBtn?: Phaser.GameObjects.Container;
  private replayBtn?: Phaser.GameObjects.Container;

  // The transcript is rendered as a single tall content container, clipped to the
  // reading-well viewport by a geometry mask, and scrolled by nudging the
  // container's Y. This replaces the rexUI ScrollablePanel (which laid rows at the
  // scene root and left a stray un-masked panel rectangle on screen) with a
  // self-contained, precisely-clipped scroll — the §6 two-layer rule stays intact.
  private content?: Phaser.GameObjects.Container;
  private well?: Phaser.GameObjects.Graphics;
  private viewMaskShape?: Phaser.GameObjects.Graphics;
  private viewMask?: Phaser.Display.Masks.GeometryMask;
  private scrollbar?: Phaser.GameObjects.Rectangle;
  private scrollTrack?: Phaser.GameObjects.Rectangle;
  // viewport geometry (scene space), recomputed on (re)layout.
  private viewport = { x: 0, y: 0, w: 0, h: 0 };
  private contentH = 0;
  private scrollY = 0; // current scroll offset (0 = top), 0..maxScroll

  private record?: TaskRecord;
  private taskId = '';
  private from: 'archive' | 'hall' = 'hall';

  private raw: StoredEvent[] | null = null;
  private parsed: ParsedEvent[] = [];
  private loadError = false;
  // monotonic load token: a stale async load (scene reopened on another run
  // before the first resolved) is discarded by comparing against this.
  private loadToken = 0;

  // frame geometry, recomputed on (re)layout.
  private frameRect = { x: 0, y: 0, w: 0, h: 0 };

  constructor() {
    super(SCENE.logbook);
  }

  init(data: LogbookInit): void {
    this.record = data.record;
    this.taskId = data.record?.id ?? data.taskId ?? '';
    this.from = data.from ?? (data.record ? 'archive' : 'hall');
  }

  create(): void {
    this.resetState();

    this.layout();
    this.scale.on('resize', this.layout, this);
    this.input.on('wheel', this.onWheel, this);

    // The scroll unrolls in with a camera fade over the slept hall/archive.
    fadeIn(this, PALETTE.scrollScrim);

    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);

    if (import.meta.env.DEV) {
      const w = window as unknown as { __logbookScene?: unknown; __logbookNav?: unknown };
      w.__logbookScene = this;
      w.__logbookNav = {
        taskId: (): string => this.taskId,
        from: (): string => this.from,
        lineCount: (): number => this.parsed.length,
        loaded: (): boolean => this.raw != null,
        replay: () => this.startReplay(),
      };
    }

    void this.loadEvents();
  }

  private resetState(): void {
    this.scrim = undefined;
    this.backstop = undefined;
    this.frame = undefined;
    this.header = undefined;
    this.statusText = undefined;
    this.backBtn = undefined;
    this.replayBtn = undefined;
    this.content = undefined;
    this.well = undefined;
    this.viewMaskShape = undefined;
    this.viewMask = undefined;
    this.scrollbar = undefined;
    this.scrollTrack = undefined;
    this.contentH = 0;
    this.scrollY = 0;
    this.raw = null;
    this.parsed = [];
    this.loadError = false;
  }

  private teardown(): void {
    this.scale.off('resize', this.layout, this);
    this.input.off('wheel', this.onWheel, this);
  }

  // Roll the scroll up. If we came from a book, wake the archive shelf; if from an
  // archer, wake the hall (both were slept, not stopped, so they stay warm). Fade
  // out first; the woken scene fades itself back in (reduced-motion = instant cut).
  private close(): void {
    play('close');
    fadeOutThen(
      this,
      () => {
        this.scene.stop(SCENE.logbook);
        this.scene.wake(this.from === 'archive' ? SCENE.archive : SCENE.hall);
      },
      PALETTE.scrollScrim,
    );
  }

  // --- data load (over the bus; no IPC here) ------------------------------

  private async loadEvents(): Promise<void> {
    if (!this.taskId) {
      this.loadError = true;
      this.renderTranscript();
      return;
    }
    const token = ++this.loadToken;
    const events = await requestLogbook(this.taskId);
    if (token !== this.loadToken) return; // a newer open superseded this one
    if (events === null) {
      this.loadError = true;
      this.renderTranscript();
      return;
    }
    this.raw = events;
    this.parsed = [];
    for (const row of events) {
      const event = parseStoredPayload(row.payloadJson);
      if (event) this.parsed.push({ seq: row.seq, tsMs: row.tsMs, event });
    }
    this.renderHeader();
    this.renderTranscript();
    this.refreshReplayButton();
  }

  // --- layout -------------------------------------------------------------

  private layout(): void {
    const { width, height } = this.scale;

    if (!this.scrim) {
      this.scrim = this.add
        .rectangle(0, 0, width, height, PALETTE.scrollScrim, 0.62)
        .setOrigin(0, 0)
        .setDepth(0)
        .setInteractive();
      this.scrim.on('pointerup', () => this.close());
    } else {
      this.scrim.setSize(width, height).setPosition(0, 0);
    }

    const frameW = Math.min(720, width - 64);
    const frameH = Math.min(680, height - 80);
    const cx = width / 2;
    const cy = height / 2 + 4;
    this.frameRect = { x: cx, y: cy, w: frameW, h: frameH };

    this.buildBackstop(cx, cy, frameW, frameH);
    this.buildScroll(cx, cy, frameW, frameH);
    this.buildChrome(cx, cy, frameW, frameH);
    this.buildViewport(cx, cy, frameW, frameH);
    this.renderHeader();
    this.renderTranscript();
    this.refreshReplayButton();
  }

  private buildBackstop(cx: number, cy: number, w: number, h: number): void {
    const bw = w + 48;
    const bh = h + 48;
    if (!this.backstop) {
      this.backstop = this.add.rectangle(cx, cy, bw, bh, 0x000000, 0).setDepth(5).setInteractive();
      this.backstop.on('pointerup', (p: Phaser.Input.Pointer) => p.event.stopPropagation());
    } else {
      this.backstop.setSize(bw, bh).setPosition(cx, cy);
    }
  }

  // The unrolled scroll: a wooden roller bar top + bottom, a parchment field
  // between them with soft rolled-edge shading.
  private buildScroll(cx: number, cy: number, w: number, h: number): void {
    this.frame?.destroy();
    const frame = this.add.container(cx, cy).setDepth(10);

    const shadow = this.add.graphics();
    shadow.fillStyle(0x000000, 0.4);
    shadow.fillRoundedRect(-w / 2 + 6, -h / 2 + 10, w, h, 14);

    const rollerH = 26;
    const g = this.add.graphics();
    // parchment field.
    g.fillStyle(PALETTE.scrollParchment, 1);
    g.fillRect(-w / 2, -h / 2 + rollerH, w, h - rollerH * 2);
    g.lineStyle(2, PALETTE.scrollParchmentEdge, 1);
    g.strokeRect(-w / 2, -h / 2 + rollerH, w, h - rollerH * 2);
    // soft rolled-edge shading just inside each roller.
    g.fillStyle(PALETTE.scrollParchmentShade, 0.6);
    g.fillRect(-w / 2, -h / 2 + rollerH, w, 10);
    g.fillStyle(PALETTE.scrollParchmentShade, 0.6);
    g.fillRect(-w / 2, h / 2 - rollerH - 10, w, 10);
    // wooden rollers top + bottom (slightly over-wide, with end caps).
    for (const ry of [-h / 2 + rollerH / 2, h / 2 - rollerH / 2]) {
      g.fillStyle(PALETTE.scrollRoller, 1);
      g.fillRoundedRect(-w / 2 - 12, ry - rollerH / 2, w + 24, rollerH, rollerH / 2);
      g.fillStyle(PALETTE.scrollRollerCap, 1);
      g.fillCircle(-w / 2 - 12, ry, rollerH / 2);
      g.fillCircle(w / 2 + 12, ry, rollerH / 2);
    }

    frame.add([shadow, g]);
    this.frame = frame;
  }

  private buildChrome(cx: number, cy: number, w: number, h: number): void {
    const rollerH = 26;
    const rowY = cy - h / 2 + rollerH / 2;
    // back (roll up) button, top-left on the upper roller.
    this.backBtn?.destroy();
    const back = this.makeButton(STR.logbookSceneBack, PALETTE.scrollRollerCap, PALETTE.hudInk, () =>
      this.close(),
    ).setDepth(40);
    back.setPosition(cx - w / 2 + back.width / 2 + 10, rowY);
    this.backBtn = back;

    // replay button, top-right on the upper roller (only shown once events load).
    this.replayBtn?.destroy();
    const replay = this.makeButton(
      STR.logbookSceneReplay,
      PALETTE.replayBtn,
      PALETTE.replayBtnInk,
      () => this.startReplay(),
    ).setDepth(40);
    replay.setPosition(cx + w / 2 - replay.width / 2 - 10, rowY);
    replay.setVisible(false);
    this.replayBtn = replay;
  }

  // The transcript reading well + its self-clipping scroll content. A single
  // content container holds every line, stacked top-down; a geometry mask clips it
  // to the well; mouse-wheel + drag nudge its Y. No rexUI panel → no stray
  // un-masked rectangle, and rows are clipped exactly to the parchment.
  private buildViewport(cx: number, cy: number, w: number, h: number): void {
    const rollerH = 26;
    const headerH = 96; // the commission header band at the top of the parchment
    const viewportW = w - 56;
    const viewportH = h - rollerH * 2 - headerH - 28;
    const viewportY = cy - h / 2 + rollerH + headerH + viewportH / 2 + 8;
    const viewX = cx - viewportW / 2;
    const viewTop = viewportY - viewportH / 2;
    this.viewport = { x: viewX, y: viewTop, w: viewportW, h: viewportH };

    // the transcript reading well: a clean inset surface inside the parchment.
    // Drawn in SCENE space at the scene root (NOT inside the frame container — that
    // would double-offset it to the bottom-right, the old stray-rectangle bug).
    this.well?.destroy();
    const well = this.add.graphics().setDepth(14);
    well.fillStyle(PALETTE.transcriptWell, 1);
    well.fillRoundedRect(viewX - 6, viewTop - 6, viewportW + 12, viewportH + 12, 6);
    well.lineStyle(1.5, PALETTE.transcriptWellEdge, 1);
    well.strokeRoundedRect(viewX - 6, viewTop - 6, viewportW + 12, viewportH + 12, 6);
    this.well = well;

    // a thin scroll track + thumb down the well's right gutter (shown only when
    // the transcript overflows). Drawn under the mask so it's never clipped.
    this.scrollTrack?.destroy();
    this.scrollTrack = this.add
      .rectangle(viewX + viewportW + 1, viewportY, 4, viewportH, PALETTE.transcriptWellEdge, 0.5)
      .setOrigin(0.5, 0.5)
      .setDepth(24)
      .setVisible(false);
    this.scrollbar?.destroy();
    this.scrollbar = this.add
      .rectangle(viewX + viewportW + 1, viewTop, 4, 40, PALETTE.scrollRollerCap, 0.9)
      .setOrigin(0.5, 0)
      .setDepth(25)
      .setVisible(false);

    // the scrolling content container, masked to the well.
    this.content?.destroy();
    this.content = this.add.container(viewX, viewTop).setDepth(22);
    this.viewMaskShape?.destroy();
    const maskShape = this.add.graphics().setVisible(false);
    maskShape.fillStyle(0xffffff, 1);
    maskShape.fillRect(viewX, viewTop, viewportW, viewportH);
    this.viewMaskShape = maskShape;
    this.viewMask = maskShape.createGeometryMask();
    this.content.setMask(this.viewMask);
    this.scrollY = 0;

    this.statusText = this.add
      .text(cx, viewportY, '', {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: PALETTE.scrollInkDim,
        align: 'center',
        wordWrap: { width: viewportW - 40 },
      })
      .setOrigin(0.5, 0.5)
      .setDepth(25);
  }

  // --- scroll handling ----------------------------------------------------

  private maxScroll(): number {
    return Math.max(0, this.contentH - this.viewport.h);
  }

  private applyScroll(): void {
    if (!this.content) return;
    const max = this.maxScroll();
    this.scrollY = Phaser.Math.Clamp(this.scrollY, 0, max);
    this.content.y = this.viewport.y - this.scrollY;
    // sync the thumb.
    if (this.scrollbar && this.scrollTrack) {
      const overflow = max > 0.5;
      this.scrollbar.setVisible(overflow);
      this.scrollTrack.setVisible(overflow);
      if (overflow) {
        const ratio = this.viewport.h / this.contentH;
        const thumbH = Math.max(28, this.viewport.h * ratio);
        const travel = this.viewport.h - thumbH;
        this.scrollbar.height = thumbH;
        this.scrollbar.y = this.viewport.y + travel * (this.scrollY / max);
      }
    }
  }

  private onWheel(
    _pointer: Phaser.Input.Pointer,
    _over: unknown,
    _dx: number,
    dy: number,
  ): void {
    if (this.maxScroll() <= 0) return;
    this.scrollY += dy * 0.5;
    this.applyScroll();
  }

  // --- commission header (prompt + totals) --------------------------------

  private renderHeader(): void {
    this.header?.destroy();
    const { x: cx, y: cy, w, h } = this.frameRect;
    if (w === 0) return;
    const rollerH = 26;
    const top = cy - h / 2 + rollerH + 12;
    const left = cx - w / 2 + 28;
    const innerW = w - 56;
    const header = this.add.container(0, 0).setDepth(16);

    const record = this.record;
    const promptText = record?.prompt ?? this.taskId;
    // commission line: prompt (the §1 「委托 #id · 标题 · 模型 · 花费」 head).
    const idShort = this.taskId.length > 10 ? `${this.taskId.slice(0, 10)}…` : this.taskId;
    const title = this.add
      .text(left, top, `委托 #${idShort} · ${promptText}`, {
        fontFamily: CJK_FONT,
        fontSize: '17px',
        color: PALETTE.scrollInk,
        fontStyle: 'bold',
        wordWrap: { width: innerW },
        maxLines: 1,
      })
      .setOrigin(0, 0);
    header.add(title);

    // meta + totals line.
    const totals = this.totals();
    const metaBits: string[] = [];
    if (record) {
      metaBits.push(projectName(record.project));
      metaBits.push(record.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate);
      metaBits.push(TASK_STATUS_LABEL[record.status] ?? record.status);
      if (record.branch) metaBits.push(`${STR.archiveBranchPrefix}${record.branch}`);
    }
    metaBits.push(`${STR.logbookSceneTotalEvents}${totals.count}`);
    metaBits.push(`${STR.logbookSceneTotalCost}${formatCost(totals.cost)}`);
    metaBits.push(`${STR.logbookSceneTotalDuration}${durationLabel(totals.duration)}`);
    if (totals.turns != null) metaBits.push(`${STR.logbookSceneTurnsPrefix}${totals.turns}`);

    const meta = this.add
      .text(left, top + 28, metaBits.join('  ·  '), {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.scrollInkDim,
        wordWrap: { width: innerW },
        maxLines: 2,
      })
      .setOrigin(0, 0);
    header.add(meta);

    // a faint rule under the header.
    const rule = this.add.graphics();
    rule.lineStyle(1, PALETTE.scrollParchmentEdge, 0.9);
    rule.lineBetween(left, top + 66, left + innerW, top + 66);
    header.add(rule);

    this.header = header;
  }

  private totals(): { count: number; cost: number | null; duration: number; turns: number | null } {
    if (this.parsed.length === 0)
      return { count: 0, cost: this.record?.costUsd ?? null, duration: 0, turns: null };
    let cost: number | null = null;
    let turns: number | null = null;
    for (const p of this.parsed) {
      if (p.event.kind === 'finished' && p.event.costUsd != null) cost = p.event.costUsd;
      else if (p.event.kind === 'result') {
        if (p.event.costUsd != null && cost == null) cost = p.event.costUsd;
        turns = p.event.numTurns;
      }
    }
    if (cost == null) cost = this.record?.costUsd ?? null;
    const first = this.parsed[0].tsMs;
    const last = this.parsed[this.parsed.length - 1].tsMs;
    return { count: this.parsed.length, cost, duration: last - first, turns };
  }

  // --- transcript (monospace, high-density) -------------------------------

  private renderTranscript(): void {
    if (!this.content || !this.statusText) return;

    this.content.removeAll(true);
    this.contentH = 0;

    if (this.loadError) {
      this.statusText.setText(STR.logbookSceneError).setVisible(true);
      this.applyScroll();
      return;
    }
    if (this.raw === null) {
      this.statusText.setText(STR.logbookSceneLoading).setVisible(true);
      this.applyScroll();
      return;
    }
    if (this.parsed.length === 0) {
      this.statusText.setText(STR.logbookSceneEmpty).setVisible(true);
      this.applyScroll();
      return;
    }
    this.statusText.setVisible(false);

    // Stack each line top-down inside the (masked) content container. Width is the
    // exact viewport width so a row never spills past the parchment's right edge.
    const rowW = this.viewport.w;
    let y = 4;
    for (const p of this.parsed) {
      const row = this.buildLine(p, rowW);
      row.setY(y);
      this.content.add(row);
      y += (row.getData('rowH') as number) + 2;
    }
    this.contentH = y + 4;
    this.scrollY = 0;
    this.applyScroll();
  }

  // One transcript line: a clock stamp + per-kind accent stripe + monospace head,
  // and (for output / error) a verbatim block in a distinct ink below. The line is
  // a container in CONTENT space (the container is masked, not each line), so rows
  // clip cleanly to the well and stay equidistant.
  private buildLine(p: ParsedEvent, width: number): Phaser.GameObjects.Container {
    const line = transcriptLine(p.event);
    const c = this.add.container(0, 0);

    const padX = 10;
    const padY = 6;
    const stampW = 70;
    const bodyX = padX + stampW + 14;
    const bodyW = width - bodyX - padX;

    const stamp = this.add
      .text(padX, padY, clockStamp(p.tsMs), {
        fontFamily: MONO_FONT,
        fontSize: '12px',
        color: PALETTE.transcriptInkDim,
      })
      .setOrigin(0, 0);

    const head = this.add
      .text(bodyX, padY, `${line.glyph} ${line.head}`, {
        fontFamily: MONO_FONT,
        fontSize: '13px',
        color: PALETTE.transcriptInk,
        wordWrap: { width: bodyW },
      })
      .setOrigin(0, 0);

    c.add([stamp, head]);
    let bottom = padY + head.height;

    if (line.block) {
      const blockColor = line.kind === 'error' ? PALETTE.transcriptError : PALETTE.transcriptOutput;
      const block = this.add
        .text(bodyX, bottom + 2, line.block, {
          fontFamily: MONO_FONT,
          fontSize: '12px',
          color: blockColor,
          wordWrap: { width: bodyW },
          lineSpacing: 2,
        })
        .setOrigin(0, 0);
      c.add(block);
      bottom = block.y + block.height;
    }

    const rowH = bottom + padY;

    // per-kind accent stripe down the left gutter.
    const stripe = this.add.graphics();
    stripe.fillStyle(kindAccent(p.event.kind), 0.9);
    stripe.fillRect(0, 2, 3, rowH - 4);
    c.addAt(stripe, 0);

    // a hairline separator under the row.
    const rule = this.add.graphics();
    rule.lineStyle(1, PALETTE.transcriptWellEdge, 0.5);
    rule.lineBetween(padX, rowH - 0.5, width - padX, rowH - 0.5);
    c.add(rule);

    c.setSize(width, rowH);
    c.setData('rowH', rowH);
    return c;
  }

  // --- 回放 (hand the run back to the workshop) ---------------------------

  private refreshReplayButton(): void {
    if (!this.replayBtn) return;
    this.replayBtn.setVisible(this.raw != null && this.raw.length > 0);
  }

  private startReplay(): void {
    if (!this.raw || this.raw.length === 0) return;
    play('open');
    // hand the stored log to useReplay (via the bridge); the workshop re-enacts it.
    EventBus.emit(BUS.replayStart, this.raw);
    fadeOutThen(
      this,
      () => {
        // a replay is always shown in the hall, regardless of where we opened from.
        // If we came via the archive, fully stop it (don't leave it orphaned
        // sleeping behind the hall) so the scene graph stays clean.
        if (this.from === 'archive' && this.scene.isSleeping(SCENE.archive)) {
          this.scene.stop(SCENE.archive);
        }
        // roll the scroll up and return to the workshop so the archer is visible.
        this.scene.stop(SCENE.logbook);
        this.scene.wake(SCENE.hall);
      },
      PALETTE.scrollScrim,
    );
  }

  private makeButton(
    label: string,
    fill: number,
    ink: string,
    onClick: () => void,
  ): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const text = this.add
      .text(0, 0, label, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: ink,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    const padX = 14;
    const padY = 7;
    const bw = text.width + padX * 2;
    const bh = text.height + padY * 2;
    const bg = this.add.graphics();
    bg.fillStyle(fill, 1);
    bg.fillRoundedRect(-bw / 2, -bh / 2, bw, bh, 8);
    bg.lineStyle(2, 0x000000, 0.18);
    bg.strokeRoundedRect(-bw / 2, -bh / 2, bw, bh, 8);
    c.add([bg, text]);
    c.setSize(bw, bh);
    c.setInteractive({ useHandCursor: true });
    c.on('pointerup', onClick);
    c.on('pointerover', () => c.setScale(1.05));
    c.on('pointerout', () => c.setScale(1));
    return c;
  }
}
