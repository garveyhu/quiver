import { useCallback, useState } from 'react';
import { PhaserGame } from '@/game/PhaserGame';
import { GameBridge } from '@/game/GameBridge';
import { HotspotOverlay } from '@/game/HotspotOverlay';
import { TextInputOverlay } from '@/game/TextInputOverlay';
import type { Hotspot } from '@/game/EventBus';

/**
 * The app shell, after the game-first flip. There is no tab bar, no header
 * chrome, no tiled form cards — the whole window is the Phaser workshop world
 * (PhaserGame). Everything else is wiring:
 *
 *  - GameBridge (headless) owns the IPC hooks and shuttles their state onto the
 *    EventBus for the scenes; it also routes world hotspot clicks back to us;
 *  - HotspotOverlay is the temporary bridge for the still-React surfaces
 *    (settings / archive / project); P3–P4 replace these with in-world scenes;
 *  - TextInputOverlay is the IME-safe keyboard seam (§0 option-b): any world
 *    scene that needs typed input summons a real DOM `<textarea>`/`<input>` over
 *    the canvas through it. The TaskBoardScene's "钉新委托" is its first user.
 */
export function App() {
  const [hotspot, setHotspot] = useState<Hotspot | null>(null);

  const openHotspot = useCallback((next: Hotspot) => setHotspot(next), []);
  const closeHotspot = useCallback(() => setHotspot(null), []);

  return (
    <main className="game-shell">
      <PhaserGame />
      <GameBridge onOpenHotspot={openHotspot}>
        <HotspotOverlay hotspot={hotspot} onClose={closeHotspot} />
        <TextInputOverlay />
      </GameBridge>
    </main>
  );
}
