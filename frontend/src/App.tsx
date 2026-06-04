import { useState } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { ProjectPicker } from '@/components/ProjectPicker';
import { ModeToggle } from '@/components/ModeToggle';
import { TaskInput } from '@/components/TaskInput';
import { PixelOffice } from '@/components/PixelOffice';
import { EventLogPanel } from '@/components/EventLogPanel';
import { STR } from '@/strings';
import type { RunMode } from '@/types/run.types';

export function App() {
  const { events, running, error, projectPath, pickProject, runTask } = useSupervisor();
  const [mode, setMode] = useState<RunMode>('simulate');

  return (
    <main className="app">
      <header className="app-header">
        <h1 className="app-brand">{STR.brand}</h1>
        <p className="app-tagline">{STR.tagline}</p>
      </header>

      <section className="app-card">
        <ProjectPicker projectPath={projectPath} disabled={running} onPick={pickProject} />

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

      <EventLogPanel events={events} />
    </main>
  );
}
