import { useState } from 'react';
import type { AgentEvent } from '@/types/agentEvent.types';
import { EventList } from '@/components/EventList';
import { STR } from '@/strings';

interface EventLogPanelProps {
  events: AgentEvent[];
}

/**
 * The demoted, collapsible raw event log. The pixel office is the primary view;
 * this keeps the underlying data one click away without dominating the layout.
 */
export function EventLogPanel({ events }: EventLogPanelProps) {
  const [open, setOpen] = useState(false);

  return (
    <section className="event-log-panel">
      <button
        type="button"
        className="event-log-toggle"
        aria-expanded={open}
        onClick={() => setOpen(o => !o)}
      >
        <span className="event-log-caret">{open ? '▾' : '▸'}</span>
        {STR.eventsHeader} <span className="app-count">({events.length})</span>
      </button>
      {open && (
        <div className="event-log-body">
          <EventList events={events} />
        </div>
      )}
    </section>
  );
}
