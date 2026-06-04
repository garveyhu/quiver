import { useSupervisor } from '@/hooks/useSupervisor';
import { TaskInput } from '@/components/TaskInput';
import { EventList } from '@/components/EventList';

export function App() {
  const { events, running, error, runTask } = useSupervisor();

  return (
    <main className="app">
      <header className="app-header">
        <h1>Quiver</h1>
        <p className="app-tagline">
          Phase 3 shell — one task → one worktree → one (fake) agent → live event stream.
        </p>
      </header>

      <TaskInput running={running} onRun={runTask} />

      {error && <p className="app-error">Task failed: {error}</p>}

      <section className="app-stream">
        <h2>
          Agent events{' '}
          <span className="app-count">({events.length})</span>
        </h2>
        <EventList events={events} />
      </section>
    </main>
  );
}
