import Phaser from 'phaser';
import { fadeMs } from '@/game/palette';

/**
 * Shared scene-transition juice (P5 #4). Every Hall ↔ sub-scene hop fades the
 * camera instead of hard-cutting, so opening the board / ledger / shelf / scroll
 * reads as "the workshop dollies to the object" rather than a jarring swap.
 *
 * Motion is honoured: when the user opts out of motion (prefers-reduced-motion)
 * `fadeMs()` is 0, so the fade collapses to an instant cut and nothing animates.
 *
 * The fade colour matches each scene's own scrim/canvas so the wash never flashes
 * a foreign colour mid-transition.
 */

/** Fade a scene's camera in from `colour` on entry (call once at the end of create). */
export function fadeIn(scene: Phaser.Scene, colour = 0x000000): void {
  const cam = scene.cameras?.main;
  if (!cam) return;
  const ms = fadeMs();
  if (ms === 0) return;
  const [r, g, b] = splitColour(colour);
  cam.fadeIn(ms, r, g, b);
}

/**
 * Fade the camera out, then run `onDone` (the actual stop/sleep/wake). Honours
 * reduced-motion by running `onDone` immediately. Safe if called twice — the
 * camera's fade-out simply restarts. If the camera isn't ready, `onDone` runs
 * immediately so a hop is never swallowed.
 */
export function fadeOutThen(scene: Phaser.Scene, onDone: () => void, colour = 0x000000): void {
  const cam = scene.cameras?.main;
  const ms = fadeMs();
  if (!cam || ms === 0) {
    onDone();
    return;
  }
  const [r, g, b] = splitColour(colour);
  cam.once(Phaser.Cameras.Scene2D.Events.FADE_OUT_COMPLETE, () => {
    onDone();
  });
  cam.fadeOut(ms, r, g, b);
}

function splitColour(colour: number): [number, number, number] {
  return [(colour >> 16) & 0xff, (colour >> 8) & 0xff, colour & 0xff];
}
