import { useState } from 'react';
import { useGameState } from '@/game/useGameState';
import type { Hotspot } from '@/game/EventBus';
import { STR } from '@/strings';
import type { RunMode } from '@/types/run.types';
import type { StoredEvent, TaskRecord } from '@/types/persistence.types';
import { TaskInput } from '@/components/TaskInput';
import { SettingsPanel } from '@/components/settings/SettingsPanel';
import { ArchiveView } from '@/components/archive/ArchiveView';
import { LogbookViewer } from '@/components/archive/LogbookViewer';
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
 * TEMPORARY BRIDGE for the not-yet-diegetic surfaces.
 *
 * The notice board is now an in-world Phaser scene (TaskBoardScene, P2), so it's
 * gone from here. The remaining hotspots — settings (账本) / archive (书架) /
 * project (门) — still open the matching *existing* React panel as a full-screen
 * overlay over the canvas; P3–P4 replace each with an in-world scene. The panels
 * read all data + actions from the shared GameState (the hooks the GameBridge
 * owns), so no IPC contract is touched.
 */
export function HotspotOverlay({ hotspot, onClose }: HotspotOverlayProps) {
  const { supervisor, settings, board, archive, replay, mode, setMode } = useGameState();
  const [openRecord, setOpenRecord] = useState<TaskRecord | null>(null);

  // "回放": start the re-enactment in the workshop, then close the overlay so
  // the archer is visible doing the run again.
  const handleReplay = (stored: StoredEvent[]) => {
    setOpenRecord(null);
    onClose();
    replay.start(stored);
  };

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

        {hotspot === 'project' && (
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
        )}

        {hotspot === 'archive' && (
          <ArchiveView
            records={archive.records}
            error={archive.error}
            onOpen={setOpenRecord}
          />
        )}
      </div>

      {/* The settings ledger renders its own backdrop, so it sits outside body. */}
      {hotspot === 'settings' && settings.settings && (
        <SettingsPanel
          settings={settings.settings}
          saveStatus={settings.saveStatus}
          patch={settings.patch}
          onClose={onClose}
        />
      )}

      {openRecord && (
        <LogbookViewer
          record={openRecord}
          loadEvents={archive.loadEvents}
          onClose={() => setOpenRecord(null)}
          onReplay={handleReplay}
        />
      )}
    </div>
  );
}
