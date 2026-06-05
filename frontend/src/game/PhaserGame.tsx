import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import Phaser from 'phaser';
import UIPlugin from 'phaser4-rex-plugins/templates/ui/ui-plugin.js';
import { BootScene } from '@/game/scenes/BootScene';
import { PreloaderScene } from '@/game/scenes/PreloaderScene';
import { HallScene } from '@/game/scenes/HallScene';
import { UIScene } from '@/game/scenes/UIScene';
import { TaskBoardScene } from '@/game/scenes/TaskBoardScene';
import { PALETTE } from '@/game/palette';

export interface PhaserGameHandle {
  game: Phaser.Game | null;
}

/**
 * The one and only `Phaser.Game` host (forwardRef, lifted from the
 * phaserjs/template-react-ts pattern). This replaces PixelOffice's anti-pattern
 * of newing up + destroying a Phaser.Game on every mount: the game is created
 * once, fills the whole window (Scale.RESIZE), and stays resident for the app's
 * life. The DOM container is enabled so a later text-input overlay can be
 * anchored to world coordinates, and the rexUI plugin is registered as a scene
 * plugin (`rexUI`) for in-world panels/toasts.
 */
export const PhaserGame = forwardRef<PhaserGameHandle>(function PhaserGame(_props, ref) {
  const hostRef = useRef<HTMLDivElement>(null);
  const gameRef = useRef<Phaser.Game | null>(null);

  useImperativeHandle(ref, () => ({ game: gameRef.current }), []);

  useEffect(() => {
    if (!hostRef.current || gameRef.current) return;

    const game = new Phaser.Game({
      type: Phaser.AUTO,
      parent: hostRef.current,
      backgroundColor: PALETTE.canvasBg,
      pixelArt: true,
      dom: { createContainer: true },
      scale: {
        mode: Phaser.Scale.RESIZE,
        width: '100%',
        height: '100%',
      },
      plugins: {
        scene: [{ key: 'rexUI', plugin: UIPlugin, mapping: 'rexUI' }],
      },
      scene: [BootScene, PreloaderScene, HallScene, UIScene, TaskBoardScene],
    });
    gameRef.current = game;

    if (import.meta.env.DEV) {
      (window as unknown as { __game?: Phaser.Game }).__game = game;
    }

    return () => {
      game.destroy(true);
      gameRef.current = null;
    };
  }, []);

  return <div ref={hostRef} className="game-host" />;
});
