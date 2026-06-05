import Phaser from 'phaser';
import { type Pose, type WorkerView } from '@/office/types';
import { createWorker, reduceWorker } from '@/office/poseMachine';
import type { AgentEvent } from '@/types/agentEvent.types';
import { play } from '@/utils/sound';
import { EventBus, BUS, type Hotspot } from '@/game/EventBus';
import { PALETTE, CJK_FONT, SCENE } from '@/game/palette';
import { STR } from '@/strings';

const COLOR = {
  bubbleFill: PALETTE.bubbleFill,
  bubbleStroke: PALETTE.bubbleStroke,
  bubbleText: PALETTE.bubbleText,
  sparkle: PALETTE.sparkle,
  glow: PALETTE.glow,
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

// A clickable world object. The notice board now opens an in-world scene
// (TaskBoardScene, P2); the rest still open the temporary React overlays via the
// bus (settings / archive / project) — so each node carries its own click action.
interface HotspotNode {
  zone: Phaser.GameObjects.Zone;
  label: Phaser.GameObjects.Container;
  glint: Phaser.GameObjects.Arc;
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

  constructor() {
    super(SCENE.hall);
  }

  create(): void {
    this.makeAnims();
    this.layoutRoom();
    this.makeHotspots();
    this.scale.on('resize', this.layoutRoom, this);

    // Subscribe to the bus; the bridge flushes the current snapshot on ready.
    EventBus.on(BUS.officeEvents, this.onEvents, this);
    EventBus.on(BUS.settings, this.onSettings, this);
    EventBus.emit(BUS.sceneReady);

    // Tear down every subscription + resize handler when the scene shuts down,
    // so a restart never double-subscribes (the one good footgun, §6).
    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);

    // DEV-ONLY visual harness: drive one archer through states without a backend.
    if (import.meta.env.DEV) {
      const w = window as unknown as { __hall?: unknown; __hallScene?: unknown };
      w.__hall = (v: WorkerView) => {
        this.workers.set(v.taskId, v);
        this.applyWorker(v);
      };
      w.__hallScene = this;
    }
  }

  private teardown(): void {
    EventBus.off(BUS.officeEvents, this.onEvents, this);
    EventBus.off(BUS.settings, this.onSettings, this);
    this.scale.off('resize', this.layoutRoom, this);
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

  // Budget → hearth: nightly budget is the primary cap, monthly is the fallback.
  // Without a cap or a spend figure we can't compute a ratio → neutral fire.
  private onSettings(settings: { nightlyBudgetUsd: number | null; monthlyCreditCapUsd: number | null } | null): void {
    if (!settings) {
      this.budgetRatio = null;
    } else {
      const cap = settings.nightlyBudgetUsd ?? settings.monthlyCreditCapUsd;
      // Spend isn't surfaced by the current IPC contract, so with a cap set we
      // show a "full" hearth (placeholder); without one, a neutral hearth.
      this.budgetRatio = cap != null && cap > 0 ? 1 : null;
    }
    this.applyHearth();
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
  }

  // The hearth's height + glow tint = the budget gauge (full=tall warm fire,
  // depleted=banked low embers). Neutral when no budget data is available.
  private applyHearth(): void {
    if (!this.fire) return;
    const { height } = this.scale;
    const ratio = this.budgetRatio ?? 0.7; // neutral, warm middle
    const base = height * 0.16;
    this.fire.setScale((base * (0.55 + 0.45 * ratio)) / 100);
    if (this.fireGlow) this.fireGlow.setFillStyle(COLOR.glow, 0.16 + 0.18 * ratio);
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
    // The temporary React overlays (settings / archive / project) emit a bus
    // command the App routes; the notice board opens its in-world scene directly.
    const overlay = (h: Hotspot) => () => EventBus.emit(BUS.openHotspot, { hotspot: h });

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
      // 桌上账本 (设置) — right side, mid
      {
        label: STR.hotspotSettings,
        anchor: (w, h) => ({ x: w * 0.9, y: h * 0.56 }),
        onClick: overlay('settings'),
      },
      // 书架/档案柜 (档案) — right side, lower
      {
        label: STR.hotspotArchive,
        anchor: (w, h) => ({ x: w * 0.9, y: h * 0.8 }),
        onClick: overlay('archive'),
      },
      // 门 (选项目) — bottom-left
      {
        label: STR.hotspotProject,
        anchor: (w, h) => ({ x: w * 0.1, y: h * 0.86 }),
        onClick: overlay('project'),
      },
    ];

    for (const spec of specs) {
      const label = this.makeHotspotLabel(spec.label);
      const glint = this.add.circle(0, 0, 7, PALETTE.glint, 0.9);
      glint.setDepth(8001);
      this.tweens.add({
        targets: glint,
        alpha: 0.25,
        scale: 1.5,
        duration: 1100,
        yoyo: true,
        repeat: -1,
        ease: 'Sine.easeInOut',
      });
      const zone = this.add.zone(0, 0, 150, 64).setInteractive({ useHandCursor: true });
      zone.setDepth(8000);
      zone.on('pointerup', spec.onClick);
      zone.on('pointerover', () => label.setScale(1.06));
      zone.on('pointerout', () => label.setScale(1));

      const node: HotspotNode = { zone, label, glint, anchor: spec.anchor, onClick: spec.onClick };
      this.hotspots.push(node);
      this.placeHotspot(node);
    }
  }

  // Sleep the world (keep it warm, don't stop) and bring the notice board up.
  private openBoard(): void {
    play('open');
    this.scene.sleep(SCENE.hall);
    this.scene.launch(SCENE.board);
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

    // Drag-to-reposition (kept from OfficeScene). Tap/hover detail is deferred
    // to a later phase (WorkerDetail → in-world card), so no React callbacks.
    container.on('drag', (_p: Phaser.Input.Pointer, dx: number, dy: number) => {
      node.dragged = true;
      container.setPosition(dx, dy);
      container.setDepth(dy);
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

    for (let i = 0; i < 10; i++) {
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

    const costStr = view.costUsd === null ? '—' : `$${view.costUsd.toFixed(2)}`;
    const xp = 10 + Math.floor((view.log.length || 0) / 2) * 5;
    const popup = this.add.text(x, y - ARCHER_H * 1.35, `+${xp} XP · 花费 ${costStr}`, {
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
