import { createContext, useContext } from 'react';
import type { SupervisorState } from '@/hooks/useSupervisor';
import type { SettingsState } from '@/hooks/useSettings';
import type { TaskBoardState } from '@/hooks/useTaskBoard';
import type { ArchiveState } from '@/hooks/useArchive';
import type { ReplayState } from '@/hooks/useReplay';
import type { RunMode } from '@/types/run.types';

/**
 * The single bundle of data-layer state the GameBridge owns and the temporary
 * React overlays consume. Lives in Context so overlays read it without prop
 * drilling, and so the hooks are instantiated in exactly one place (GameBridge)
 * — keeping IPC ownership singular.
 */
export interface GameState {
  supervisor: SupervisorState;
  settings: SettingsState;
  board: TaskBoardState;
  archive: ArchiveState;
  replay: ReplayState;
  /** The effective run mode for new commissions (seeded from settings.defaultMode). */
  mode: RunMode;
  setMode: (mode: RunMode) => void;
}

export const GameStateContext = createContext<GameState | null>(null);

export function useGameState(): GameState {
  const ctx = useContext(GameStateContext);
  if (!ctx) throw new Error('useGameState 必须在 GameBridge 内部使用');
  return ctx;
}
