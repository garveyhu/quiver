import { EventBus, BUS, type LogbookRequest, type StoredEvent } from '@/game/EventBus';

let channelSeq = 0;

/**
 * Ask the headless GameBridge to load ONE run's full §11 event log and await the
 * result (the LogbookScene's data seam). Mirrors {@link requestTextInput}: a
 * fresh one-shot reply channel per call so concurrent opens never cross wires,
 * and the listener is always torn down on resolve.
 *
 * The bridge satisfies it via the unchanged `useArchive.loadEvents` (the only IPC
 * caller) — the scene never touches `invoke`. Resolves with the run's stored
 * events, or `null` if the load failed.
 */
export function requestLogbook(taskId: string): Promise<StoredEvent[] | null> {
  const channel = `logbook:reply:${++channelSeq}`;
  const request: LogbookRequest = { channel, taskId };
  return new Promise(resolve => {
    const onReply = (result: StoredEvent[] | null) => {
      EventBus.off(channel, onReply);
      resolve(result);
    };
    EventBus.once(channel, onReply);
    EventBus.emit(BUS.logbookLoad, request);
  });
}
