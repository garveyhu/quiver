import Phaser from 'phaser';
import { POSE_FRAME, type Pose, type WorkerView } from '@/office/types';

// Cozy warm palette mirrored from styles.css (kept in sync by hand — these are
// the few colours the canvas needs; the CSS vars are the source of truth).
const COLOR = {
  floor: 0xf3e8d2, // warm parchment floor
  floorEdge: 0xe6d3b0,
  stationFill: 0xfbedd3, // tinted amber station mat
  stationStroke: 0xe0cda8,
  campfire: 0xe0992e, // brand amber flame
  bubbleFill: 0xfffdf8,
  bubbleStroke: 0xe0cda8,
  bubbleText: '#3a2e25',
  sparkle: 0xe0992e,
} as const;

// Only the 4 clean fantasy-archer sheets are sliced (see scripts/slice_sprites.py);
// worker slots cycle through them.
const CHAR_COUNT = 4;
const POSE_NAMES: Pose[] = ['idle', 'working', 'tense', 'celebrate', 'sick'];

// One archer + its station + speech bubble.
interface ArcherNode {
  slot: number;
  container: Phaser.GameObjects.Container;
  sprite: Phaser.GameObjects.Image;
  bubble: Phaser.GameObjects.Container;
  bubbleText: Phaser.GameObjects.Text;
  pose: Pose;
  baseY: number;
  bobTween?: Phaser.Tweens.Tween;
}

/**
 * The pixel office. One Phaser scene that owns a sprite per task and swaps its
 * pose + bubble as `WorkerView`s are pushed in via `applyWorker`. The React host
 * (PixelOffice.tsx) drives it; the scene itself holds no event logic.
 */
export class OfficeScene extends Phaser.Scene {
  private nodes = new Map<string, ArcherNode>();
  // taskIds whose celebrate sparkle already fired — keeps it a one-shot even if
  // the (terminal) worker view is re-applied on later events.
  private sparkled = new Set<string>();
  private floor?: Phaser.GameObjects.Graphics;

  // true once preload + create finished and textures are ready to render.
  ready = false;
  // host callback fired when `ready` flips true, so React can replay events.
  onReady?: () => void;

  constructor() {
    super('office');
  }

  preload(): void {
    for (let c = 0; c < CHAR_COUNT; c++) {
      for (const pose of POSE_NAMES) {
        this.load.image(this.texKey(c, pose), `sprites/char${c}-${POSE_FRAME[pose]}.png`);
      }
    }
  }

  create(): void {
    this.drawFloor();
    this.scale.on('resize', this.drawFloor, this);
    this.ready = true;
    this.onReady?.();
  }

  private texKey(charIndex: number, pose: Pose): string {
    return `c${charIndex}-${pose}`;
  }

  private drawFloor(): void {
    this.floor?.destroy();
    const { width, height } = this.scale;
    const g = this.add.graphics();
    g.setDepth(-100);
    // warm floor band along the bottom two-thirds
    const floorTop = height * 0.42;
    g.fillStyle(COLOR.floor, 1);
    g.fillRect(0, floorTop, width, height - floorTop);
    g.lineStyle(2, COLOR.floorEdge, 1);
    g.beginPath();
    g.moveTo(0, floorTop);
    g.lineTo(width, floorTop);
    g.strokePath();
    this.floor = g;
    // keep stations under the archers anchored to the new layout
    for (const node of this.nodes.values()) this.layoutNode(node);
  }

  // --- layout -------------------------------------------------------------

  private slotPosition(slot: number, total: number): { x: number; y: number } {
    const { width, height } = this.scale;
    const cols = Math.min(total, 4);
    const col = slot % cols;
    const row = Math.floor(slot / cols);
    const cellW = width / cols;
    const x = cellW * col + cellW / 2;
    const y = height * 0.62 + row * (height * 0.3);
    return { x, y };
  }

  private layoutNode(node: ArcherNode): void {
    const total = this.nodes.size;
    const { x, y } = this.slotPosition(node.slot, total);
    node.container.setPosition(x, y);
    node.baseY = y;
  }

  // --- worker sync --------------------------------------------------------

  /** Drop all archers (called when a new run starts / events cleared). */
  reset(): void {
    for (const node of this.nodes.values()) node.container.destroy();
    this.nodes.clear();
    this.sparkled.clear();
  }

  /** Create or update the archer for one worker view. */
  applyWorker(view: WorkerView): void {
    let node = this.nodes.get(view.taskId);
    if (!node) {
      node = this.spawnNode(view);
      this.nodes.set(view.taskId, node);
      // re-layout everyone now that the count changed
      for (const n of this.nodes.values()) this.layoutNode(n);
    }
    this.setBubble(node, view.bubble);
    if (node.pose !== view.pose) this.setPose(node, view);
    if (view.sparkle && view.pose === 'celebrate' && !this.sparkled.has(view.taskId)) {
      this.sparkled.add(view.taskId);
      this.sparkle(node);
    }
  }

  private spawnNode(view: WorkerView): ArcherNode {
    const charIndex = view.slot % CHAR_COUNT;
    const container = this.add.container(0, 0);

    // station mat under the archer
    const mat = this.add.ellipse(0, 64, 132, 40, COLOR.stationFill, 1);
    mat.setStrokeStyle(2, COLOR.stationStroke, 1);

    // a tiny campfire dot for cozy flavour
    const fire = this.add.circle(0, 70, 6, COLOR.campfire, 0.9);

    const sprite = this.add.image(0, 0, this.texKey(charIndex, view.pose));
    sprite.setOrigin(0.5, 1); // feet on the mat
    sprite.setDisplaySize(110, 120);

    const bubble = this.makeBubble(view.bubble);
    bubble.setPosition(0, -132);

    container.add([mat, fire, sprite, bubble]);
    const node: ArcherNode = {
      slot: view.slot,
      container,
      sprite,
      bubble,
      bubbleText: bubble.getData('text') as Phaser.GameObjects.Text,
      pose: view.pose,
      baseY: 0,
    };
    this.startBob(node);
    return node;
  }

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
    const padX = 12;
    const padY = 7;
    const bg = this.add.graphics();
    this.paintBubbleBg(bg, label.width + padX * 2, label.height + padY * 2);
    c.add([bg, label]);
    c.setData('text', label);
    c.setData('bg', bg);
    return c;
  }

  private paintBubbleBg(bg: Phaser.GameObjects.Graphics, w: number, h: number): void {
    bg.clear();
    bg.fillStyle(COLOR.bubbleFill, 1);
    bg.lineStyle(2, COLOR.bubbleStroke, 1);
    bg.fillRoundedRect(-w / 2, -h / 2, w, h, 8);
    bg.strokeRoundedRect(-w / 2, -h / 2, w, h, 8);
    // little tail
    bg.fillTriangle(-6, h / 2 - 1, 6, h / 2 - 1, 0, h / 2 + 8);
  }

  private setBubble(node: ArcherNode, text: string): void {
    if (node.bubbleText.text === text) return;
    node.bubbleText.setText(text);
    const bg = node.bubble.getData('bg') as Phaser.GameObjects.Graphics;
    this.paintBubbleBg(bg, node.bubbleText.width + 24, node.bubbleText.height + 14);
  }

  private setPose(node: ArcherNode, view: WorkerView): void {
    const charIndex = view.slot % CHAR_COUNT;
    node.pose = view.pose;
    node.sprite.setTexture(this.texKey(charIndex, view.pose));
    node.sprite.setDisplaySize(110, 120);
  }

  // --- effects ------------------------------------------------------------

  private startBob(node: ArcherNode): void {
    // gentle 2-frame-feel idle bob (a few px) — not a full walk cycle.
    node.bobTween = this.tweens.add({
      targets: node.sprite,
      y: -6,
      duration: 900,
      yoyo: true,
      repeat: -1,
      ease: 'Sine.easeInOut',
    });
  }

  private sparkle(node: ArcherNode): void {
    const { x, y } = { x: node.container.x, y: node.container.y };
    for (let i = 0; i < 6; i++) {
      const star = this.add.star(
        x + Phaser.Math.Between(-50, 50),
        y - 80 + Phaser.Math.Between(-30, 20),
        4,
        3,
        8,
        COLOR.sparkle,
      );
      star.setDepth(50);
      this.tweens.add({
        targets: star,
        y: star.y - 40,
        alpha: 0,
        scale: 0.3,
        duration: 800,
        delay: i * 70,
        ease: 'Cubic.easeOut',
        onComplete: () => star.destroy(),
      });
    }
  }
}
