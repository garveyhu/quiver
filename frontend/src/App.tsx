import { PhaserGame } from '@/game/PhaserGame';
import { GameBridge } from '@/game/GameBridge';
import { TextInputOverlay } from '@/game/TextInputOverlay';

/**
 * The app shell, after the game-first flip is complete. There is no tab bar, no
 * header chrome, no tiled form cards, no React panels — the whole window is the
 * Phaser workshop world. Just three elements:
 *
 *  - PhaserGame: the one resident `Phaser.Game` host (the full-window world);
 *  - GameBridge: headless — owns the IPC hooks and shuttles their state onto the
 *    EventBus for the scenes, and routes world commands (pick-project, board /
 *    settings / archive / logbook actions) back to the hooks. Renders nothing;
 *  - TextInputOverlay: the IME-safe keyboard seam (§0 option-b) — any world scene
 *    that needs typed input summons a real DOM `<textarea>`/`<input>` over the
 *    canvas through it (the board's "钉新委托", the ledger's cells, archive search).
 */
export function App() {
  return (
    <main className="game-shell">
      <PhaserGame />
      <GameBridge />
      <TextInputOverlay />
    </main>
  );
}
