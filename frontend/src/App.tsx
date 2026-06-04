import { useState } from 'react';
import { useSupervisor } from '@/hooks/useSupervisor';
import { ProjectPicker } from '@/components/ProjectPicker';
import { ModeToggle } from '@/components/ModeToggle';
import { TaskInput } from '@/components/TaskInput';
import { EventList } from '@/components/EventList';
import type { RunMode } from '@/types/run.types';

export function App() {
  const { events, running, error, projectPath, pickProject, runTask } = useSupervisor();
  const [mode, setMode] = useState<RunMode>('simulate');

  return (
    <main className="app">
      <header className="app-header">
        <h1>Quiver</h1>
        <p className="app-tagline">
          Pick a git repo → describe a task → Simulate (free) or Real Claude → watch the events.
        </p>
      </header>

      <ProjectPicker projectPath={projectPath} disabled={running} onPick={pickProject} />

      <ModeToggle mode={mode} disabled={running} onChange={setMode} />

      <TaskInput running={running} disabled={!projectPath} onRun={prompt => runTask(prompt, mode)} />

      {!projectPath && (
        <p className="app-hint">Pick a project to enable Run.</p>
      )}

      {error && <p className="app-error">Task failed: {error}</p>}

      <section className="app-stream">
        <h2>
          Agent events <span className="app-count">({events.length})</span>
        </h2>
        <EventList events={events} />
      </section>
    </main>
  );
}
