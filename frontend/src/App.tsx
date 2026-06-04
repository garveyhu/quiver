import { useEffect, useRef, useState } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { useSettings } from '@/hooks/useSettings';
import { useTaskBoard } from '@/hooks/useTaskBoard';
import { useArchive } from '@/hooks/useArchive';
import { ProjectPicker } from '@/components/ProjectPicker';
import { RecentProjects } from '@/components/RecentProjects';
import { ModeToggle } from '@/components/ModeToggle';
import { TaskInput } from '@/components/TaskInput';
import { TaskBoard } from '@/components/board/TaskBoard';
import { PixelOffice } from '@/components/PixelOffice';
import { EventLogPanel } from '@/components/EventLogPanel';
import { SettingsButton } from '@/components/settings/SettingsButton';
import { SettingsPanel } from '@/components/settings/SettingsPanel';
import { SceneTabs, type Scene } from '@/components/SceneTabs';
import { ArchiveView } from '@/components/archive/ArchiveView';
import { LogbookViewer } from '@/components/archive/LogbookViewer';
import { STR } from '@/strings';
import type { RunMode } from '@/types/run.types';
import type { TaskRecord } from '@/types/persistence.types';

export function App() {
  const { events, error, projectPath, recentProjects, pickProject, selectRecentProject } =
    useSupervisor();
  const { settings, patch, saveStatus } = useSettings();
  const {
    tasks,
    error: boardError,
    freshIds,
    enqueue,
    reorder,
    cancel,
  } = useTaskBoard(projectPath);
  const { records, error: archiveError, loadEvents } = useArchive();

  const [mode, setMode] = useState<RunMode>('simulate');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [scene, setScene] = useState<Scene>('workshop');
  const [openRecord, setOpenRecord] = useState<TaskRecord | null>(null);

  const maxWorkers = settings?.maxWorkers ?? 1;

  // Pre-select the run mode from the saved default — but only once, before the
  // user has touched the toggle, so a manual choice is never overridden.
  const seededMode = useRef(false);
  useEffect(() => {
    if (!seededMode.current && settings) {
      seededMode.current = true;
      if (settings.defaultMode === 'real' || settings.defaultMode === 'simulate') {
        setMode(settings.defaultMode);
      }
    }
  }, [settings]);

  // Apply the effective appearance settings to the document: `uiScale` drives a
  // CSS variable the shell consumes, `theme` swaps the palette via a data attr.
  useEffect(() => {
    if (!settings) return;
    document.documentElement.style.setProperty('--ui-scale', String(settings.uiScale));
    document.documentElement.dataset.theme = settings.theme;
  }, [settings]);

  return (
    <main className="app">
      <SettingsButton onClick={() => setSettingsOpen(true)} />

      <header className="app-header">
        <h1 className="app-brand">{STR.brand}</h1>
        <p className="app-tagline">{STR.tagline}</p>
      </header>

      <section className="app-card">
        <ProjectPicker projectPath={projectPath} disabled={false} onPick={pickProject} />

        <RecentProjects
          projects={recentProjects}
          activePath={projectPath}
          disabled={false}
          onSelect={selectRecentProject}
        />

        <ModeToggle mode={mode} disabled={false} onChange={setMode} />

        <TaskInput disabled={!projectPath} onSubmit={prompt => enqueue(prompt, mode)} />

        {projectPath && <p className="app-hint">{STR.enqueueHint}</p>}
        {!projectPath && <p className="app-hint">{STR.noProjectHint}</p>}

        {(error || boardError) && (
          <p className="app-error">
            {STR.taskFailedPrefix}
            {error ?? boardError}
          </p>
        )}
      </section>

      <SceneTabs scene={scene} onChange={setScene} />

      {/* The workshop + board panels stay MOUNTED across tab switches (hidden via
          CSS, not unmounted) so the live Phaser scene and the event stream keep
          running regardless of which scene is shown. */}
      <div className={`scene-panel ${scene === 'workshop' ? '' : 'scene-hidden'}`}>
        <section className="app-office">
          <h2 className="app-office-title">{STR.officeTitle}</h2>
          <PixelOffice events={events} />
        </section>
        <EventLogPanel events={events} />
      </div>

      <div className={`scene-panel ${scene === 'board' ? '' : 'scene-hidden'}`}>
        <TaskBoard
          tasks={tasks}
          freshIds={freshIds}
          maxWorkers={maxWorkers}
          onReorder={reorder}
          onCancel={cancel}
        />
      </div>

      {scene === 'archive' && (
        <div className="scene-panel">
          <ArchiveView records={records} error={archiveError} onOpen={setOpenRecord} />
        </div>
      )}

      {openRecord && (
        <LogbookViewer
          record={openRecord}
          loadEvents={loadEvents}
          onClose={() => setOpenRecord(null)}
        />
      )}

      {settingsOpen && settings && (
        <SettingsPanel
          settings={settings}
          saveStatus={saveStatus}
          patch={patch}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </main>
  );
}
