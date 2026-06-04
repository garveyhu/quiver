import type { AgentEvent } from '@/types/agentEvent.types';
import { EVENT_KIND_LABEL } from '@/strings';
import { EVENT_KIND_GLYPH, describeEvent } from '@/utils/eventFormat';

interface EventRowProps {
  event: AgentEvent;
}

export function EventRow({ event }: EventRowProps) {
  return (
    <li className="event-row">
      <span className="event-glyph">{EVENT_KIND_GLYPH[event.kind]}</span>
      <span className="event-kind">{EVENT_KIND_LABEL[event.kind]}</span>
      <span className="event-desc">{describeEvent(event)}</span>
    </li>
  );
}
