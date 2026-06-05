import Phaser from 'phaser';
import { SCENE } from '@/game/palette';

/**
 * The first scene the game boots into. Intentionally tiny: it owns no assets and
 * exists only to hand off to the Preloader. (A real splash / config-load step
 * would live here later.)
 */
export class BootScene extends Phaser.Scene {
  constructor() {
    super(SCENE.boot);
  }

  create(): void {
    this.scene.start(SCENE.preloader);
  }
}
