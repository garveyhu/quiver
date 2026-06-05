import Phaser from 'phaser';
import { type Pose, type WorkerView } from '@/office/types';
import { createWorker, reduceWorker } from '@/office/poseMachine';
import type { AgentEvent } from '@/types/agentEvent.types';
import { play } from '@/utils/sound';
import { EventBus, BUS } from '@/game/EventBus';
import { PALETTE, CJK_FONT, SCENE, prefersReducedMotion } from '@/game/palette';
import { fadeIn, fadeOutThen } from '@/game/transition';
import { STR } from '@/strings';

const COLOR = {
  bubbleFill: PALETTE.bubbleFill,
  bubbleStroke: PALETTE.bubbleStroke,
  bubbleText: PALETTE.bubbleText,
  sparkle: PALETTE.sparkle,
  glow: PALETTE.glow,
  ember: PALETTE.emberLow,
  popup: PALETTE.popup,
  popupStroke: PALETTE.popupStroke,
} as const;

const CHAR_COUNT = 4;

// Native cell size of a sliced character frame (scripts/slice_assets.py).
const CHAR_CELL = { w: 234, h: 256 };
const WALK_FRAMES = 6;
const BOW_FRAMES = 6;
const FIRE_FRAMES = 3;

// How big an archer is drawn on the canvas (height in px).
const ARCHER_H = 168;

interface ArcherNode {
  slot: number;
  view: WorkerView;
  container: Phaser.GameObjects.Container;
  station: Phaser.GameObjects.Image;
  sprite: Phaser.GameObjects.Sprite;
  shadow: Phaser.GameObjects.Ellipse;
  bubble: Phaser.GameObjects.Container;
  bubbleText: Phaser.GameObjects.Text;
  pose: Pose;
  homeX: number;
  homeY: number;
  poseTween?: Phaser.Tweens.Tween;
  walkTween?: Phaser.Tweens.Tween;
  dragged: boolean;
}

// A clickable world object. The notice board (TaskBoardScene, P2), the ledger
// (SettingsScene, P3) and the bookshelf (ArchiveScene, P4) now open in-world
// scenes; only the door/project picker still opens a temporary React overlay via
// the bus — so each node carries its own click action.
interface HotspotNode {
  zone: Phaser.GameObjects.Zone;
  label: Phaser.GameObjects.Container;
  glint: Phaser.GameObjects.Arc;
  halo: Phaser.GameObjects.Arc;
  anchor: (w: number, h: number) => { x: number; y: number };
  onClick: () => void;
}

/**
 * The resident, full-window workshop world (升格 from OfficeScene). It renders
 * the room backdrop, an animated hearth fire whose glow tracks the budget,
 * lantern glow, one archer-at-a-station per task, and the diegetic navigation
 * hotspots (notice board / ledger / bookshelf / door) that — for milestone 1 —
 * open the existing React panels as overlays.
 *
 * Unlike the old OfficeScene (which only rendered and leaned on PixelOffice for
 * the event loop), HallScene owns the whole pipeline: it subscribes to the
 * EventBus in `create()`, runs the pure pose state machine over the incoming
 * AgentEvent stream itself, and tears every subscription down on `shutdown` so
 * no listener leaks across a scene restart.
 */
export class HallScene extends Phaser.Scene {
  private nodes = new Map<string, ArcherNode>();
  private sparkled = new Set<string>();
  private bg?: Phaser.GameObjects.Image;
  private fire?: Phaser.GameObjects.Sprite;
  private fireGlow?: Phaser.GameObjects.Ellipse;
  private lanternGlows: Phaser.GameObjects.Ellipse[] = [];
  private hotspots: HotspotNode[] = [];

  // per-task view-model + stable slot bookkeeping (absorbed from PixelOffice).
  private workers = new Map<string, WorkerView>();
  private nextSlot = 0;
  private cursor = 0;

  // budget 0..1 (hearth fire height / colour). null = unknown → neutral.
  private budgetRatio: number | null = null;

  // Whether a project (workshop) is selected. Until one is, the workshop sleeps:
  // a cold dim wash, banked embers, a dozing archer, and the door glint pulsing
  // "点亮工坊 · 选择项目" (the §6 no-tutorial empty-state cue, P3 #14).
  private hasProject = true;
  private dimWash?: Phaser.GameObjects.Rectangle;
  private sleeper?: Phaser.GameObjects.Container;
  private wakeHint?: Phaser.GameObjects.Container;
  // 夜幕 theme cool wash over the whole world (P4 #16).
  private nightWash?: Phaser.GameObjects.Rectangle;

  constructor() {
    super(SCENE.hall);
  }

  create(): void {
    this.makeAnims();
    this.layoutRoom();
    this.makeHotspots();
    this.scale.on('resize', this.layoutRoom, this);

    // Fade the workshop in on first entry and every time a sub-scene closes back
    // to it (the world was slept, not stopped, so create() doesn't re-run — the
    // WAKE event is the seam). Honours reduced-motion via fadeIn().
    fadeIn(this, PALETTE.canvasBgInt);
    this.events.on(Phaser.Scenes.Events.WAKE, this.onWake, this);

    // Subscribe to the bus; the bridge flushes the current snapshot on ready.
    EventBus.on(BUS.officeEvents, this.onEvents, this);
    EventBus.on(BUS.settings, this.onSettings, this);
    EventBus.on(BUS.project, this.onProject, this);
    EventBus.emit(BUS.sceneReady);

    // Tear down every subscription + resize handler when the scene shuts down,
    // so a restart never double-subscribes (the one good footgun, §6).
    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);

    // DEV-ONLY visual harness: drive one archer through states without a backend,
    // and expose the diegetic navigation actions so the headless verification
    // harness can exercise the REAL click handlers (open the shelf, click an
    // archer to unroll its Logbook) without simulating canvas pixel clicks.
    if (import.meta.env.DEV) {
      const w = window as unknown as {
        __hall?: unknown;
        __hallScene?: unknown;
        __hallNav?: unknown;
      };
      w.__hall = (v: WorkerView) => {
        this.workers.set(v.taskId, v);
        this.applyWorker(v);
      };
      w.__hallScene = this;
      w.__hallNav = {
        openArchive: () => this.openArchive(),
        openSettings: () => this.openSettings(),
        openBoard: () => this.openBoard(),
        // click the first spawned archer (Mockup C); returns its taskId or null.
        clickFirstArcher: (): string | null => {
          const first = this.nodes.values().next().value as ArcherNode | undefined;
          if (!first) return null;
          this.openArcherLogbook(first.view.taskId);
          return first.view.taskId;
        },
        archerTaskIds: (): string[] => [...this.nodes.keys()],
      };
    }
  }

  private teardown(): void {
    EventBus.off(BUS.officeEvents, this.onEvents, this);
    EventBus.off(BUS.settings, this.onSettings, this);
    EventBus.off(BUS.project, this.onProject, this);
    this.scale.off('resize', this.layoutRoom, this);
    this.events.off(Phaser.Scenes.Events.WAKE, this.onWake, this);
  }

  // The workshop woke back up (a sub-scene closed). Fade it in from the wash so the
  // return reads as a soft dolly-back rather than a hard cut.
  private onWake(): void {
    fadeIn(this, PALETTE.canvasBgInt);
  }

  // --- bus handlers -------------------------------------------------------

  // The full live event array (live stream or an active replay). We diff against
  // our cursor; a shrink means a reset (live↔replay swap), matching the old
  // PixelOffice shrink-to-reset contract.
  private onEvents(events: AgentEvent[]): void {
    if (events.length < this.cursor) {
      this.workers.clear();
      this.nextSlot = 0;
      this.cursor = 0;
      this.reset();
    }
    for (let i = this.cursor; i < events.length; i++) {
      const event = events[i];
      let view = this.workers.get(event.taskId);
      if (!view) view = createWorker(event.taskId, this.nextSlot++);
      view = reduceWorker(view, event);
      this.workers.set(event.taskId, view);
      this.applyWorker(view);
    }
    this.cursor = events.length;
  }

  // Project gate: an unselected workshop sleeps cold + dim until the door is used.
  private onProject(payload: { projectPath: string | null }): void {
    const has = payload.projectPath != null;
    if (has === this.hasProject && (this.dimWash || has)) return;
    this.hasProject = has;
    this.applyWorkshopState();
  }

  // Budget → hearth + theme → world tint. Nightly budget is the primary cap,
  // monthly the fallback. Without a cap or spend figure we can't compute a ratio →
  // neutral fire. The appearance theme (暖阳 / 夜幕) tints the whole world, not just
  // the DOM overlay (P4 #16): 夜幕 lays a soft cool wash over the workshop.
  private onSettings(
    settings:
      | { nightlyBudgetUsd: number | null; monthlyCreditCapUsd: number | null; theme?: string }
      | null,
  ): void {
    if (!settings) {
      this.budgetRatio = null;
    } else {
      const cap = settings.nightlyBudgetUsd ?? settings.monthlyCreditCapUsd;
      // Spend isn't surfaced by the current IPC contract, so with a cap set we
      // show a "full" hearth (placeholder); without one, a neutral hearth.
      this.budgetRatio = cap != null && cap > 0 ? 1 : null;
    }
    this.applyHearth();
    this.applyTheme(settings?.theme === 'midnight');
  }

  // A soft cool wash over the world for the 夜幕 theme (kept under the dim/empty
  // wash + HUD, above the room art). 暖阳 removes it entirely.
  private applyTheme(night: boolean): void {
    const { width, height } = this.scale;
    if (night) {
      if (!this.nightWash) {
        this.nightWash = this.add
          .rectangle(0, 0, width, height, PALETTE.nightWash, 0.22)
          .setOrigin(0, 0)
          .setDepth(40);
      } else {
        this.nightWash.setSize(width, height).setVisible(true);
      }
    } else {
      this.nightWash?.setVisible(false);
    }
  }

  private makeAnims(): void {
    if (!this.anims.exists('fire')) {
      this.anims.create({
        key: 'fire',
        frames: this.anims.generateFrameNumbers('fire', { start: 0, end: FIRE_FRAMES - 1 }),
        frameRate: 8,
        repeat: -1,
      });
    }
    for (let c = 0; c < CHAR_COUNT; c++) {
      if (!this.anims.exists(`c${c}-walk`)) {
        this.anims.create({
          key: `c${c}-walk`,
          frames: this.anims.generateFrameNumbers(`c${c}-walk`, { start: 0, end: WALK_FRAMES - 1 }),
          frameRate: 9,
          repeat: -1,
        });
      }
      if (!this.anims.exists(`c${c}-bow`)) {
        this.anims.create({
          key: `c${c}-bow`,
          frames: this.anims.generateFrameNumbers(`c${c}-bow`, { start: 0, end: BOW_FRAMES - 1 }),
          frameRate: 6,
          repeat: -1,
          yoyo: true,
        });
      }
    }
  }

  // --- room layout --------------------------------------------------------

  private layoutRoom(): void {
    const { width, height } = this.scale;

    if (!this.bg) {
      this.bg = this.add.image(0, 0, 'room').setOrigin(0, 0).setDepth(-100);
    }
    const tex = this.textures.get('room').getSourceImage();
    const scale = Math.max(width / tex.width, height / tex.height);
    this.bg.setScale(scale);
    this.bg.setPosition((width - tex.width * scale) / 2, (height - tex.height * scale) / 2);

    // hearth fire overlaid on the stone fireplace (left ~9%, ~48% down).
    const fx = width * 0.092;
    const fy = height * 0.52;
    if (!this.fireGlow) {
      this.fireGlow = this.add.ellipse(fx, fy, 120, 80, COLOR.glow, 0.22).setDepth(-60);
      this.tweens.add({
        targets: this.fireGlow,
        alpha: 0.34,
        scaleX: 1.12,
        scaleY: 1.12,
        duration: 700,
        yoyo: true,
        repeat: -1,
        ease: 'Sine.easeInOut',
      });
    } else {
      this.fireGlow.setPosition(fx, fy);
    }
    if (!this.fire) {
      this.fire = this.add.sprite(fx, fy, 'fire').setDepth(-55).setOrigin(0.5, 0.62);
      this.fire.play('fire');
      // a subtle alpha flicker on the flame so the hearth feels alive (ambient,
      // §8). On its own property so it never fights the glow's breathing tween.
      // Skipped under reduced-motion (the flame holds steady).
      if (!prefersReducedMotion()) {
        this.tweens.add({
          targets: this.fire,
          alpha: { from: 1, to: 0.86 },
          duration: 140,
          yoyo: true,
          repeat: -1,
          ease: 'Sine.easeInOut',
          delay: 300,
        });
      }
    } else {
      this.fire.setPosition(fx, fy);
    }
    this.applyHearth();

    const lanternSpots: Array<[number, number]> = [
      [width * 0.5, height * 0.12],
      [width * 0.85, height * 0.18],
    ];
    if (this.lanternGlows.length === 0) {
      for (const [lx, ly] of lanternSpots) {
        const g = this.add.ellipse(lx, ly, 90, 70, COLOR.glow, 0.16).setDepth(-58);
        this.tweens.add({
          targets: g,
          alpha: 0.26,
          duration: 1400 + Math.random() * 600,
          yoyo: true,
          repeat: -1,
          ease: 'Sine.easeInOut',
        });
        this.lanternGlows.push(g);
      }
    } else {
      this.lanternGlows.forEach((g, i) => g.setPosition(...lanternSpots[i]));
    }

    for (const node of this.nodes.values()) this.placeNode(node);
    for (const h of this.hotspots) this.placeHotspot(h);

    // re-place / re-size the empty-state small-play (dim wash, sleeper, door hint).
    this.applyWorkshopState();
    // keep the night wash full-bleed on resize.
    if (this.nightWash?.visible) this.nightWash.setSize(width, height);
  }

  // The hearth's height + glow tint = the budget gauge (full=tall warm fire,
  // depleted=banked low embers). Neutral when no budget data is available. When
  // banked (ratio ≤ 0.15, e.g. budget paused) the flame shrinks to embers, the
  // glow cools, and the lanterns dim — the §1 "限额暂停 → 火 banked、灯暗" state.
  private applyHearth(): void {
    if (!this.fire) return;
    const { height } = this.scale;
    const ratio = this.budgetRatio ?? 0.7; // neutral, warm middle
    const base = height * 0.16;
    this.fire.setScale((base * (0.55 + 0.45 * ratio)) / 100);
    const banked = ratio <= 0.15;
    if (this.fireGlow) {
      this.fireGlow.setFillStyle(banked ? COLOR.ember : COLOR.glow, 0.12 + 0.2 * ratio);
    }
    // dim the lanterns when banked so the workshop reads as "powered down".
    const lanternAlpha = banked ? 0.06 : 0.16;
    for (const g of this.lanternGlows) g.setFillStyle(COLOR.glow, lanternAlpha);
  }

  // The §1 empty-state small-play (P3 #14): no project → a cold, dim, sleeping
  // workshop with the door glint shouting "点亮工坊 · 选择项目"; a project → the
  // workshop lights back up. Idempotent; safe to call on (re)layout + on change.
  private applyWorkshopState(): void {
    const { width, height } = this.scale;
    if (!this.hasProject) {
      // a cold blue wash chills the whole room (deep enough to read as "asleep",
      // but the hearth + candle + door hint still glow warmly through it).
      if (!this.dimWash) {
        this.dimWash = this.add
          .rectangle(0, 0, width, height, PALETTE.coldWash, 0.62)
          .setOrigin(0, 0)
          .setDepth(50);
      } else {
        this.dimWash.setSize(width, height).setVisible(true);
      }
      // bank the hearth to embers + dim the lanterns.
      this.budgetRatio = 0.1;
      this.applyHearth();
      this.showSleeper();
      this.emphasizeDoorGlint(true);
    } else {
      this.dimWash?.setVisible(false);
      this.sleeper?.setVisible(false);
      this.wakeHint?.setVisible(false);
      this.emphasizeDoorGlint(false);
      // restore the hearth to its budget-driven level (neutral if unknown).
      if (this.budgetRatio === 0.1) this.budgetRatio = null;
      this.applyHearth();
    }
  }

  // A single dozing archer slumped at the centre station — the workshop isn't
  // empty, it's asleep, waiting to be lit.
  private showSleeper(): void {
    const { width, height } = this.scale;
    const x = width * 0.46;
    const y = height - 128;
    if (!this.sleeper) {
      const c = this.add.container(x, y).setDepth(60);
      const station = this.add.image(6, 60, 'station').setOrigin(0.5, 1);
      const stationH = ARCHER_H * 1.35;
      station.setDisplaySize(stationH * (station.width / station.height), stationH);
      const sprite = this.add.sprite(-18, 58, 'c0-sick').setOrigin(0.5, 1);
      sprite.setScale(ARCHER_H / CHAR_CELL.h);
      const zzz = this.add
        .text(20, -ARCHER_H - 4, '💤', { fontFamily: CJK_FONT, fontSize: '22px' })
        .setOrigin(0.5, 0.5);
      c.add([station, sprite, zzz]);
      // a slow dozing breath, unless motion is reduced.
      if (!prefersReducedMotion()) {
        this.tweens.add({
          targets: zzz,
          y: zzz.y - 6,
          alpha: 0.4,
          duration: 1600,
          yoyo: true,
          repeat: -1,
          ease: 'Sine.easeInOut',
        });
      }
      this.sleeper = c;
    } else {
      this.sleeper.setPosition(x, y).setVisible(true);
    }
  }

  // Pulse the door glint hard + show "点亮工坊 · 选择项目" so an empty workshop
  // points unambiguously at the one action that matters.
  private emphasizeDoorGlint(on: boolean): void {
    const door = this.hotspots[this.hotspots.length - 1]; // door is the last spec
    if (!door) return;
    if (on) {
      door.glint.setRadius(11).setFillStyle(PALETTE.glint, 1).setDepth(70);
      door.label.setDepth(70);
      const { x, y } = door.anchor(this.scale.width, this.scale.height);
      if (!this.wakeHint) {
        const hint = this.add.container(x, y - 44).setDepth(70);
        const text = this.add
          .text(0, 0, STR.firstRunWake, {
            fontFamily: CJK_FONT,
            fontSize: '15px',
            color: PALETTE.hudInk,
            fontStyle: 'bold',
            align: 'center',
          })
          .setOrigin(0.5, 0.5);
        const pad = 12;
        const bg = this.add.graphics();
        bg.fillStyle(PALETTE.hudAccentInt, 0.96);
        bg.fillRoundedRect(-text.width / 2 - pad, -text.height / 2 - 7, text.width + pad * 2, text.height + 14, 9);
        hint.add([bg, text]);
        this.wakeHint = hint;
        if (!prefersReducedMotion()) {
          this.tweens.add({
            targets: hint,
            y: hint.y - 6,
            duration: 900,
            yoyo: true,
            repeat: -1,
            ease: 'Sine.easeInOut',
          });
        }
      } else {
        this.wakeHint.setPosition(x, y - 44).setVisible(true);
      }
    } else {
      door.glint.setRadius(7);
      this.wakeHint?.setVisible(false);
    }
  }

  private slotPosition(slot: number, total: number): { x: number; y: number } {
    const { width, height } = this.scale;
    const cols = Math.min(Math.max(total, 1), 4);
    const col = slot % cols;
    const row = Math.floor(slot / cols);
    const left = width * 0.24;
    const right = width * 0.7; // leave the right edge clear for the ledger/shelf
    const span = right - left;
    const x = total === 1 ? width * 0.46 : left + (span * (col + 0.5)) / cols;
    const floorY = height - 70;
    const y = floorY - 58 - row * height * 0.14;
    return { x, y };
  }

  private placeNode(node: ArcherNode): void {
    if (node.dragged) return;
    const { x, y } = this.slotPosition(node.slot, this.nodes.size);
    node.homeX = x;
    node.homeY = y;
    node.container.setPosition(x, y);
    node.container.setDepth(y);
  }

  // --- diegetic navigation hotspots --------------------------------------

  private makeHotspots(): void {
    const specs: Array<{
      label: string;
      anchor: (w: number, h: number) => { x: number; y: number };
      onClick: () => void;
    }> = [
      // 墙上公告板 (任务) — top-right wall → in-world TaskBoardScene
      {
        label: STR.hotspotBoard,
        anchor: (w, h) => ({ x: w * 0.84, y: h * 0.26 }),
        onClick: () => this.openBoard(),
      },
      // 桌上账本 (设置) — right side, mid → in-world SettingsScene
      {
        label: STR.hotspotSettings,
        anchor: (w, h) => ({ x: w * 0.9, y: h * 0.56 }),
        onClick: () => this.openSettings(),
      },
      // 书架/档案柜 (档案) — right side, lower → in-world ArchiveScene
      {
        label: STR.hotspotArchive,
        anchor: (w, h) => ({ x: w * 0.9, y: h * 0.8 }),
        onClick: () => this.openArchive(),
      },
      // 门 (选项目) — bottom-left. Drives useSupervisor.pickProject directly over
      // the bus (the native folder dialog); no React overlay anymore (P5).
      {
        label: STR.hotspotProject,
        anchor: (w, h) => ({ x: w * 0.1, y: h * 0.86 }),
        onClick: () => {
          play('open');
          EventBus.emit(BUS.pickProject);
        },
      },
    ];

    const reduce = prefersReducedMotion();
    for (const spec of specs) {
      const label = this.makeHotspotLabel(spec.label);
      // a soft affordance halo behind the label, plus a small breathing glint dot
      // hugging its corner — the §6 "no-tutorial diegetic nav" cue that an object
      // is touchable. The breathing is skipped under reduced-motion.
      const halo = this.add.circle(0, 0, 30, PALETTE.glint, 0.0).setDepth(7999);
      const glint = this.add.circle(0, 0, 7, PALETTE.glint, 0.9);
      glint.setDepth(8001);
      if (!reduce) {
        this.tweens.add({
          targets: glint,
          alpha: 0.25,
          scale: 1.5,
          duration: 1100,
          yoyo: true,
          repeat: -1,
          ease: 'Sine.easeInOut',
        });
      }
      const zone = this.add.zone(0, 0, 150, 64).setInteractive({ useHandCursor: true });
      zone.setDepth(8000);
      zone.on('pointerup', spec.onClick);
      // hover: lift + brighten the label and bloom the halo so the target is
      // unmistakable on approach (affordance glint on hover, §6).
      zone.on('pointerover', () => {
        label.setScale(1.08);
        glint.setScale(1.4).setAlpha(1);
        this.tweens.add({ targets: halo, fillAlpha: 0.22, duration: reduce ? 0 : 160 });
      });
      zone.on('pointerout', () => {
        label.setScale(1);
        glint.setScale(1).setAlpha(0.9);
        this.tweens.add({ targets: halo, fillAlpha: 0, duration: reduce ? 0 : 160 });
      });

      const node: HotspotNode = { zone, label, glint, halo, anchor: spec.anchor, onClick: spec.onClick };
      this.hotspots.push(node);
      this.placeHotspot(node);
    }
  }

  // Sleep the world (keep it warm, don't stop) and bring a sub-scene up — fading
  // the camera out first so the hop reads as a dolly to the object, not a cut. The
  // sub-scene fades itself in on create; the world fades back in on WAKE. When the
  // user opts out of motion the fade collapses to an instant launch.
  private openScene(key: string, data?: object): void {
    play('open');
    fadeOutThen(
      this,
      () => {
        this.scene.sleep(SCENE.hall);
        // reset the (now-slept) camera so its next WAKE fade-in starts cleanly.
        this.cameras.main.resetFX();
        if (data) this.scene.launch(key, data);
        else this.scene.launch(key);
      },
      PALETTE.canvasBgInt,
    );
  }

  private openBoard(): void {
    this.openScene(SCENE.board);
  }

  private openSettings(): void {
    this.openScene(SCENE.settings);
  }

  private openArchive(): void {
    this.openScene(SCENE.archive);
  }

  // Mockup C: click a station archer → unroll THAT run's Logbook scroll. Works for
  // live + finished archers; closing the scroll wakes the hall back.
  private openArcherLogbook(taskId: string): void {
    this.openScene(SCENE.logbook, { taskId, from: 'hall' });
  }

  private makeHotspotLabel(text: string): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0).setDepth(8002);
    const label = this.add.text(0, 0, text, {
      fontFamily: CJK_FONT,
      fontSize: '15px',
      color: PALETTE.hudInk,
      fontStyle: 'bold',
      align: 'center',
    });
    label.setOrigin(0.5, 0.5);
    const pad = 12;
    const bg = this.add.graphics();
    bg.fillStyle(PALETTE.hudPanel, 0.92);
    bg.lineStyle(2, PALETTE.hudPanelEdge, 1);
    const w = label.width + pad * 2;
    const h = label.height + pad;
    bg.fillRoundedRect(-w / 2, -h / 2, w, h, 9);
    bg.strokeRoundedRect(-w / 2, -h / 2, w, h, 9);
    c.add([bg, label]);
    return c;
  }

  private placeHotspot(node: HotspotNode): void {
    const { width, height } = this.scale;
    const { x, y } = node.anchor(width, height);
    node.label.setPosition(x, y);
    node.zone.setPosition(x, y);
    node.halo.setPosition(x, y);
    // the breathing glint hugs the label's top-right corner as an affordance cue.
    node.glint.setPosition(x + node.label.width * 0.42 + 14, y - 16);
  }

  // --- worker sync (unchanged behaviour from OfficeScene) -----------------

  reset(): void {
    for (const node of this.nodes.values()) node.container.destroy();
    this.nodes.clear();
    this.sparkled.clear();
  }

  applyWorker(view: WorkerView): void {
    let node = this.nodes.get(view.taskId);
    if (!node) {
      node = this.spawnNode(view);
      this.nodes.set(view.taskId, node);
      for (const n of this.nodes.values()) this.placeNode(n);
    }
    node.view = view;
    this.setBubble(node, view.bubble);
    if (node.pose !== view.pose) this.applyState(node, view);
    if (view.sparkle && view.pose === 'celebrate' && !this.sparkled.has(view.taskId)) {
      this.sparkled.add(view.taskId);
      this.celebrate(node, view);
    }
  }

  private spawnNode(view: WorkerView): ArcherNode {
    const charIndex = view.slot % CHAR_COUNT;
    const container = this.add.container(0, 0);

    const station = this.add.image(6, 60, 'station').setOrigin(0.5, 1);
    const stationH = ARCHER_H * 1.35;
    station.setDisplaySize(stationH * (station.width / station.height), stationH);

    const shadow = this.add.ellipse(0, 60, ARCHER_H * 0.6, ARCHER_H * 0.18, 0x2a1c12, 0.22);

    const sprite = this.add.sprite(-18, 58, `c${charIndex}-idle`);
    sprite.setOrigin(0.5, 1);
    this.fitSprite(sprite);

    const bubble = this.makeBubble(view.bubble);
    bubble.setPosition(0, -ARCHER_H - 16);

    container.add([station, shadow, sprite, bubble]);

    const hitW = 150;
    const hitH = ARCHER_H + 80;
    const hit = new Phaser.Geom.Rectangle(-hitW / 2, -ARCHER_H - 10, hitW, hitH);
    container.setInteractive({
      hitArea: hit,
      hitAreaCallback: Phaser.Geom.Rectangle.Contains,
      draggable: true,
      useHandCursor: true,
    });

    const node: ArcherNode = {
      slot: view.slot,
      view,
      container,
      station,
      sprite,
      shadow,
      bubble,
      bubbleText: bubble.getData('text') as Phaser.GameObjects.Text,
      pose: 'idle',
      homeX: 0,
      homeY: 0,
      dragged: false,
    };

    // Tap vs drag (Mockup C): a clean tap opens this archer's Logbook scroll; a
    // drag just repositions the station. We track whether the pointer moved past a
    // small threshold between down and up to tell them apart.
    let downX = 0;
    let downY = 0;
    let moved = false;
    container.on('pointerdown', (p: Phaser.Input.Pointer) => {
      downX = p.x;
      downY = p.y;
      moved = false;
    });
    container.on('drag', (_p: Phaser.Input.Pointer, dx: number, dy: number) => {
      node.dragged = true;
      moved = true;
      container.setPosition(dx, dy);
      container.setDepth(dy);
    });
    container.on('pointerup', (p: Phaser.Input.Pointer) => {
      const dist = Phaser.Math.Distance.Between(downX, downY, p.x, p.y);
      if (!moved && dist < 6) this.openArcherLogbook(view.taskId);
    });

    this.applyState(node, view);
    return node;
  }

  private fitSprite(sprite: Phaser.GameObjects.Sprite): void {
    sprite.setScale(ARCHER_H / CHAR_CELL.h);
  }

  // --- pose / animation state machine (unchanged from OfficeScene) --------

  private applyState(node: ArcherNode, view: WorkerView): void {
    node.pose = view.pose;
    const c = view.slot % CHAR_COUNT;
    node.poseTween?.stop();
    node.poseTween = undefined;
    node.walkTween?.stop();
    node.walkTween = undefined;
    node.sprite.setAngle(0);
    node.sprite.setScale(ARCHER_H / CHAR_CELL.h);

    switch (view.pose) {
      case 'arriving':
        this.playWalkIn(node, c);
        break;
      case 'idle':
        node.sprite.stop();
        node.sprite.setTexture(`c${c}-idle`);
        this.fitSprite(node.sprite);
        this.breathe(node, -8, 1500);
        break;
      case 'working':
        node.sprite.stop();
        node.sprite.setTexture(`c${c}-reading`);
        this.fitSprite(node.sprite);
        this.breathe(node, -4, 900);
        break;
      case 'tense':
        node.sprite.play(`c${c}-bow`);
        this.breathe(node, -3, 600);
        break;
      case 'celebrate':
        node.sprite.stop();
        node.sprite.setTexture(`c${c}-celebrate`);
        this.fitSprite(node.sprite);
        this.hop(node);
        break;
      case 'sick':
        node.sprite.stop();
        node.sprite.setTexture(`c${c}-sick`);
        this.fitSprite(node.sprite);
        this.slump(node);
        break;
    }
  }

  private playWalkIn(node: ArcherNode, c: number): void {
    node.sprite.play(`c${c}-walk`);
    node.sprite.setX(-90);
    node.walkTween = this.tweens.add({
      targets: node.sprite,
      x: -18,
      duration: 650,
      ease: 'Linear',
      onComplete: () => {
        node.sprite.stop();
        node.sprite.setTexture(`c${c}-reading`);
        this.fitSprite(node.sprite);
        node.pose = 'working';
        this.breathe(node, -4, 900);
      },
    });
  }

  private breathe(node: ArcherNode, dy: number, duration: number): void {
    node.sprite.setY(58);
    node.poseTween = this.tweens.add({
      targets: node.sprite,
      y: 58 + dy,
      duration,
      yoyo: true,
      repeat: -1,
      ease: 'Sine.easeInOut',
    });
  }

  private hop(node: ArcherNode): void {
    node.sprite.setY(58);
    node.poseTween = this.tweens.add({
      targets: node.sprite,
      y: 58 - 22,
      duration: 380,
      yoyo: true,
      repeat: -1,
      hold: 120,
      ease: 'Quad.easeOut',
    });
  }

  private slump(node: ArcherNode): void {
    node.sprite.setY(58);
    node.poseTween = this.tweens.add({
      targets: node.sprite,
      y: 58 + 4,
      angle: -3,
      duration: 1800,
      yoyo: true,
      repeat: -1,
      ease: 'Sine.easeInOut',
    });
  }

  // --- speech bubble (unchanged from OfficeScene) -------------------------

  private makeBubble(text: string): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const label = this.add.text(0, 0, text, {
      fontFamily: CJK_FONT,
      fontSize: '13px',
      color: COLOR.bubbleText,
      align: 'center',
      wordWrap: { width: 150 },
    });
    label.setOrigin(0.5, 0.5);
    const bg = this.add.graphics();
    this.paintBubbleBg(bg, label.width + 24, label.height + 14);
    c.add([bg, label]);
    c.setData('text', label);
    c.setData('bg', bg);
    return c;
  }

  private paintBubbleBg(bg: Phaser.GameObjects.Graphics, w: number, h: number): void {
    bg.clear();
    bg.fillStyle(COLOR.bubbleFill, 0.96);
    bg.lineStyle(2, COLOR.bubbleStroke, 1);
    bg.fillRoundedRect(-w / 2, -h / 2, w, h, 8);
    bg.strokeRoundedRect(-w / 2, -h / 2, w, h, 8);
    bg.fillTriangle(-6, h / 2 - 1, 6, h / 2 - 1, 0, h / 2 + 8);
  }

  private setBubble(node: ArcherNode, text: string): void {
    if (node.bubbleText.text === text) return;
    node.bubbleText.setText(text);
    const bg = node.bubble.getData('bg') as Phaser.GameObjects.Graphics;
    this.paintBubbleBg(bg, node.bubbleText.width + 24, node.bubbleText.height + 14);
  }

  // --- juice (unchanged from OfficeScene) ---------------------------------

  private celebrate(node: ArcherNode, view: WorkerView): void {
    play('complete');
    const x = node.container.x;
    const y = node.container.y;
    const reduce = prefersReducedMotion();

    // confetti storm — skipped under reduced-motion (the +XP/−$cost float still
    // shows, just without the particle burst).
    for (let i = 0; i < (reduce ? 0 : 10); i++) {
      const star = this.add.star(
        x + Phaser.Math.Between(-55, 55),
        y - ARCHER_H * 0.7 + Phaser.Math.Between(-30, 20),
        4,
        3,
        9,
        COLOR.sparkle,
      );
      star.setDepth(9000);
      this.tweens.add({
        targets: star,
        y: star.y - 50,
        alpha: 0,
        scale: 0.2,
        angle: Phaser.Math.Between(-90, 90),
        duration: 850,
        delay: i * 55,
        ease: 'Cubic.easeOut',
        onComplete: () => star.destroy(),
      });
    }

    // the spend reads as a cost (−$x.xx), the XP as a gain (+N), per Mockup C.
    const costStr = view.costUsd === null ? '—' : `−$${view.costUsd.toFixed(2)}`;
    const xp = 10 + Math.floor((view.log.length || 0) / 2) * 5;
    const popup = this.add.text(x, y - ARCHER_H * 1.35, `+${xp} XP · ${costStr}`, {
      fontFamily: CJK_FONT,
      fontSize: '15px',
      color: COLOR.popup,
      stroke: COLOR.popupStroke,
      strokeThickness: 3,
      fontStyle: 'bold',
    });
    popup.setOrigin(0.5, 1).setDepth(9001);
    this.tweens.add({
      targets: popup,
      y: popup.y - 60,
      alpha: { from: 1, to: 0 },
      scale: { from: 0.8, to: 1.1 },
      duration: 1600,
      ease: 'Cubic.easeOut',
      onComplete: () => popup.destroy(),
    });

    // Surface the completion to the HUD toast (UIScene) so it's felt globally.
    EventBus.emit('hud:toast', { xp, cost: view.costUsd });
  }
}
