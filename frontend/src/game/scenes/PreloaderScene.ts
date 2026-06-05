import Phaser from 'phaser';
import { SCENE } from '@/game/palette';

// Native cell size of a sliced character frame (scripts/slice_assets.py).
// Kept in lockstep with HallScene's CHAR_CELL.
const CHAR_CELL = { w: 234, h: 268 };
const CHAR_COUNT = 4;

/**
 * Loads every workshop asset once, up front, then launches the resident world
 * (HallScene) + its HUD (UIScene) in parallel. This is the home for the room /
 * prop / character spritesheet loads that used to live in OfficeScene.preload —
 * pulling them here means the textures are warm before any scene that draws
 * them runs, and the heavy load happens exactly once per game (not per mount).
 */
export class PreloaderScene extends Phaser.Scene {
  constructor() {
    super(SCENE.preloader);
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
    // The hall is the resident world; the HUD runs alongside it in parallel.
    this.scene.start(SCENE.hall);
    this.scene.launch(SCENE.ui);
  }
}
