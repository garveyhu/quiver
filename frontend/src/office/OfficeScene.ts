import Phaser from 'phaser';
import { type Pose, type WorkerView } from '@/office/types';
import { play } from '@/utils/sound';

// Cozy warm palette mirrored from styles.css (the few colours the canvas needs;
// the CSS vars are the source of truth).
const COLOR = {
  bubbleFill: 0xfffdf8,
  bubbleStroke: 0xe0cda8,
  bubbleText: '#3a2e25',
  sparkle: 0xffd27a,
  glow: 0xffb347,
  popup: '#ffe9b8',
  popupStroke: '#7a4f2e',
} as const;

const CHAR_COUNT = 4;

// Native cell size of a sliced character frame (scripts/slice_assets.py).
const CHAR_CELL = { w: 234, h: 256 };
const WALK_FRAMES = 6;
const BOW_FRAMES = 6;
const FIRE_FRAMES = 3;

// How big an archer is drawn on the canvas (height in px). The station is sized
// relative to this so the archer is believably seated/standing at it.
const ARCHER_H = 168;

interface ArcherNode {
  slot: number;
  view: WorkerView;
  container: Phaser.GameObjects.Container; // moves around the room
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

/**
 * The cozy pixel workshop. Renders the room backdrop, an animated hearth fire,
 * lantern glow, and one archer-at-a-station per task. Pose changes are driven
 * by `applyWorker(WorkerView)`; the scene owns animation + juice, no event
 * logic (that lives in poseMachine.ts / PixelOffice.tsx).
 */
export class OfficeScene extends Phaser.Scene {
  private nodes = new Map<string, ArcherNode>();
  private sparkled = new Set<string>();
  private bg?: Phaser.GameObjects.Image;
  private fire?: Phaser.GameObjects.Sprite;
  private fireGlow?: Phaser.GameObjects.Ellipse;
  private lanternGlows: Phaser.GameObjects.Ellipse[] = [];

  ready = false;
  onReady?: () => void;
  // host callbacks for React-side interactions (tooltip / detail / drag).
  onHover?: (taskId: string | null) => void;
  onSelect?: (taskId: string) => void;

  constructor() {
    super('office');
  }

  preload(): void {
    this.load.image('room', 'bg/room.png');
    this.load.image('station', 'props/station.png');
    this.load.spritesheet('fire', 'props/fire.png', { frameWidth: 156, frameHeight: 100 });
    for (let c = 0; c < CHAR_COUNT; c++) {
      this.load.spritesheet(`c${c}-walk`, `sprites/char${c}-walk.png`, {
        frameWidth: CHAR_CELL.w,
        frameHeight: CHAR_CELL.h,
      });
      this.load.spritesheet(`c${c}-bow`, `sprites/char${c}-bow.png`, {
        frameWidth: CHAR_CELL.w,
        frameHeight: CHAR_CELL.h,
      });
      for (const single of ['idle', 'reading', 'celebrate', 'sick', 'portrait']) {
        this.load.image(`c${c}-${single}`, `sprites/char${c}-${single}.png`);
      }
    }
  }

  create(): void {
    this.makeAnims();
    this.layoutRoom();
    this.scale.on('resize', this.layoutRoom, this);
    this.ready = true;
    this.onReady?.();
  }

  private makeAnims(): void {
    this.anims.create({
      key: 'fire',
      frames: this.anims.generateFrameNumbers('fire', { start: 0, end: FIRE_FRAMES - 1 }),
      frameRate: 8,
      repeat: -1,
    });
    for (let c = 0; c < CHAR_COUNT; c++) {
      this.anims.create({
        key: `c${c}-walk`,
        frames: this.anims.generateFrameNumbers(`c${c}-walk`, { start: 0, end: WALK_FRAMES - 1 }),
        frameRate: 9,
        repeat: -1,
      });
      this.anims.create({
        key: `c${c}-bow`,
        frames: this.anims.generateFrameNumbers(`c${c}-bow`, { start: 0, end: BOW_FRAMES - 1 }),
        frameRate: 6,
        repeat: -1,
        yoyo: true,
      });
    }
  }

  // --- room layout --------------------------------------------------------

  private layoutRoom(): void {
    const { width, height } = this.scale;

    // backdrop: cover-fit the room art, anchored to fill the canvas.
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
    this.fire.setScale((height * 0.16) / 100);

    // soft lantern glows (two hanging lanterns near the top centre/right).
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
  }

  // archers stand on a floor band in the lower half of the room.
  private slotPosition(slot: number, total: number): { x: number; y: number } {
    const { width, height } = this.scale;
    const cols = Math.min(Math.max(total, 1), 4);
    const col = slot % cols;
    const row = Math.floor(slot / cols);
    // keep them off the far-left fireplace; spread across the warm floor.
    const left = width * 0.24;
    const right = width * 0.86;
    const span = right - left;
    const x = total === 1 ? width * 0.52 : left + (span * (col + 0.5)) / cols;
    // The container's local origin sits a little above the feet (sprite bottom is
    // at +58, station base at +60). Anchor so feet land on the floor band with
    // headroom for the speech bubble (which floats at -ARCHER_H-16 above origin).
    const floorY = height - 70; // feet line, leaving a margin below the rug
    const y = floorY - 58 - row * height * 0.14;
    return { x, y };
  }

  private placeNode(node: ArcherNode): void {
    if (node.dragged) return;
    const { x, y } = this.slotPosition(node.slot, this.nodes.size);
    node.homeX = x;
    node.homeY = y;
    node.container.setPosition(x, y);
    // depth-sort by y so lower archers paint in front.
    node.container.setDepth(y);
  }

  // --- worker sync --------------------------------------------------------

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

    // station prop (workbench + stool + rug) sits behind the archer.
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

    // interactive: hover -> tooltip, click -> detail, drag -> reposition.
    // Hit area in container-local space: covers the archer body + station base.
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

    let downAt = 0;
    let moved = false;
    container.on('pointerover', () => this.onHover?.(node.view.taskId));
    container.on('pointerout', () => this.onHover?.(null));
    container.on('pointerdown', () => {
      downAt = this.time.now;
      moved = false;
    });
    container.on('pointerup', () => {
      // a tap (no meaningful drag) opens the detail panel.
      if (!moved && this.time.now - downAt < 400) this.onSelect?.(node.view.taskId);
    });
    container.on('dragstart', () => {
      moved = true;
    });
    container.on('drag', (_p: Phaser.Input.Pointer, dx: number, dy: number) => {
      node.dragged = true;
      moved = true;
      container.setPosition(dx, dy);
      container.setDepth(dy);
    });

    this.applyState(node, view);
    return node;
  }

  private fitSprite(sprite: Phaser.GameObjects.Sprite): void {
    const scale = ARCHER_H / CHAR_CELL.h;
    sprite.setScale(scale);
  }

  // --- pose / animation state machine -------------------------------------

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
        this.breathe(node, -4, 900); // small focused bob
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
    // enter from the left of the home position, walk to the station.
    node.sprite.play(`c${c}-walk`);
    node.sprite.setX(-90);
    node.walkTween = this.tweens.add({
      targets: node.sprite,
      x: -18,
      duration: 650,
      ease: 'Linear',
      onComplete: () => {
        // settle into the working pose once seated.
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

  // --- speech bubble ------------------------------------------------------

  private makeBubble(text: string): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const label = this.add.text(0, 0, text, {
      fontFamily: 'PingFang SC, Hiragino Sans GB, Microsoft YaHei, sans-serif',
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

  // --- juice --------------------------------------------------------------

  private celebrate(node: ArcherNode, view: WorkerView): void {
    play('complete'); // cozy completion chime (no-op until audio ships)
    const x = node.container.x;
    const y = node.container.y;

    // sparkle burst
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

    // floating "+XP · 花费 $x.xx" popup
    const costStr = view.costUsd === null ? '—' : `$${view.costUsd.toFixed(2)}`;
    const xp = 10 + Math.floor((view.log.length || 0) / 2) * 5; // playful, derived from activity
    const popup = this.add.text(x, y - ARCHER_H * 1.35, `+${xp} XP · 花费 ${costStr}`, {
      fontFamily: 'PingFang SC, Hiragino Sans GB, Microsoft YaHei, sans-serif',
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
  }
}
