import { useGameState } from '@/game/useGameState';
import type { Hotspot } from '@/game/EventBus';
import { STR } from '@/strings';
import type { RunMode } from '@/types/run.types';
import { TaskInput } from '@/components/TaskInput';
import { ProjectPicker } from '@/components/ProjectPicker';
import { RecentProjects } from '@/components/RecentProjects';
import { ModeToggle } from '@/components/ModeToggle';
import { CozyEmpty } from '@/components/CozyEmpty';

interface HotspotOverlayProps {
  /** Which world object was touched, or `null` for "no overlay open". */
  hotspot: Hotspot | null;
  onClose: () => void;
}

/**
 * TEMPORARY BRIDGE for the last not-yet-diegetic surface.
 *
 * The notice board (TaskBoardScene, P2), the ledger/settings (SettingsScene, P3)
 * and the run archive + Logbook (ArchiveScene / LogbookScene, P4) are now in-world
 * Phaser scenes, so they're gone from here. Only the door/project picker still
 * opens the existing React panel as a full-screen overlay; P5 absorbs it into the
 * door/sign. The panel reads all data + actions from the shared GameState (the
 * hooks the GameBridge owns), so no IPC contract is touched.
 */
export function HotspotOverlay({ hotspot, onClose }: HotspotOverlayProps) {
  const { supervisor, board, mode, setMode } = useGameState();

  const handleEnqueue = (prompt: string, runMode: RunMode) => {
    void board.enqueue(prompt, runMode);
  };

  if (!hotspot) return null;

  return (
    <div className="hotspot-overlay" role="dialog" aria-modal="true">
      <div className="hotspot-overlay-scrim" onClick={onClose} role="presentation" />

      <div className="hotspot-overlay-body">
        <button type="button" className="hotspot-overlay-close" onClick={onClose}>
          {STR.overlayClose}
        </button>

        <section className="hotspot-card">
          <ProjectPicker
            projectPath={supervisor.projectPath}
            disabled={false}
            onPick={supervisor.pickProject}
          />
          <RecentProjects
            projects={supervisor.recentProjects}
            activePath={supervisor.projectPath}
            disabled={false}
            onSelect={supervisor.selectRecentProject}
          />
          <ModeToggle mode={mode} disabled={false} onChange={setMode} />
          <TaskInput
            disabled={!supervisor.projectPath}
            onSubmit={prompt => handleEnqueue(prompt, mode)}
          />
          {supervisor.projectPath ? (
            <p className="app-hint">{STR.enqueueHint}</p>
          ) : (
            <div className="first-run">
              <CozyEmpty glyph="🗂️" title={STR.firstRunTitle} hint={STR.firstRunHint} />
            </div>
          )}
          {(supervisor.error || board.error) && (
            <p className="app-error">
              {STR.taskFailedPrefix}
              {supervisor.error ?? board.error}
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
