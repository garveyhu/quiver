import type { AgentEvent } from '@/types/agentEvent.types';
import { EventRow } from '@/components/EventRow';

interface EventListProps {
  events: AgentEvent[];
}

export function EventList({ events }: EventListProps) {
  if (events.length === 0) {
    return <p className="event-empty">No events yet. Run a task to watch the soul work.</p>;
  }
  return (
    <ul className="event-list">
      {events.map(event => (
        <EventRow key={`${event.taskId}-${event.seq}-${event.kind}`} event={event} />
      ))}
    </ul>
  );
}
