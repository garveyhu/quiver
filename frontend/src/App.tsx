import { useEffect, useRef, useState } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { useSettings } from '@/hooks/useSettings';
import { ProjectPicker } from '@/components/ProjectPicker';
import { RecentProjects } from '@/components/RecentProjects';
import { RunHistory } from '@/components/RunHistory';
import { ModeToggle } from '@/components/ModeToggle';
import { TaskInput } from '@/components/TaskInput';
import { PixelOffice } from '@/components/PixelOffice';
import { EventLogPanel } from '@/components/EventLogPanel';
import { SettingsButton } from '@/components/settings/SettingsButton';
import { SettingsPanel } from '@/components/settings/SettingsPanel';
import { STR } from '@/strings';
import type { RunMode } from '@/types/run.types';

export function App() {
  const {
    events,
    running,
    error,
    projectPath,
    recentProjects,
    history,
    pickProject,
    selectRecentProject,
    runTask,
  } = useSupervisor();
  const { settings, patch, saveStatus } = useSettings();
  const [mode, setMode] = useState<RunMode>('simulate');
  const [settingsOpen, setSettingsOpen] = useState(false);

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
        <ProjectPicker projectPath={projectPath} disabled={running} onPick={pickProject} />

        <RecentProjects
          projects={recentProjects}
          activePath={projectPath}
          disabled={running}
          onSelect={selectRecentProject}
        />

        <ModeToggle mode={mode} disabled={running} onChange={setMode} />

        <TaskInput running={running} disabled={!projectPath} onRun={prompt => runTask(prompt, mode)} />

        {!projectPath && <p className="app-hint">{STR.noProjectHint}</p>}

        {error && (
          <p className="app-error">
            {STR.taskFailedPrefix}
            {error}
          </p>
        )}
      </section>

      <section className="app-office">
        <h2 className="app-office-title">{STR.officeTitle}</h2>
        <PixelOffice events={events} />
      </section>

      <section className="app-card app-history-card">
        <RunHistory history={history} />
      </section>

      <EventLogPanel events={events} />

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
