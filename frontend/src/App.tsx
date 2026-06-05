import { useCallback, useState } from 'react';
import { PhaserGame } from '@/game/PhaserGame';
import { GameBridge } from '@/game/GameBridge';
import { HotspotOverlay } from '@/game/HotspotOverlay';
import type { Hotspot } from '@/game/EventBus';

/**
 * The app shell, after the game-first flip. There is no tab bar, no header
 * chrome, no tiled form cards — the whole window is the Phaser workshop world
 * (PhaserGame). Everything else is wiring:
 *
 *  - GameBridge (headless) owns the IPC hooks and shuttles their state onto the
 *    EventBus for the scenes; it also routes world hotspot clicks back to us;
 *  - HotspotOverlay is the milestone-1 temporary bridge: touching a world
 *    object opens the matching existing React panel as a full-screen overlay
 *    (P2–P4 replace these with in-world scenes).
 *
 * A text-input overlay layer will live here too once the board/settings scenes
 * need an on-canvas IME field (option-b); for now the overlays carry their own
 * inputs.
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
      </GameBridge>
    </main>
  );
}
