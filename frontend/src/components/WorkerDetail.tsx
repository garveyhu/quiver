import type { WorkerView } from '@/office/types';
import { STR, EVENT_KIND_LABEL } from '@/strings';
import type { AgentEvent } from '@/types/agentEvent.types';

interface WorkerDetailProps {
  worker: WorkerView;
  onClose: () => void;
}

const POSE_LABEL: Record<WorkerView['pose'], string> = {
  arriving: '就位中',
  idle: '待命',
  working: '工作中',
  tense: '紧张作业',
  celebrate: '庆祝',
  sick: '受挫',
};

const KIND_GLYPH: Record<string, string> = {
  worker_started: '🟢',
  tool_use: '🔧',
  output_chunk: '💬',
  result: '💰',
  error: '🛑',
  finished: '🏁',
};

/**
 * Parchment detail panel for one archer: its current state, running cost, and a
 * live, append-only trail of the events it has processed (tool calls / outputs /
 * results). Opened by clicking a worker in the scene.
 */
export function WorkerDetail({ worker, onClose }: WorkerDetailProps) {
  const cost =
    worker.costUsd === null
      ? STR.workerDetailCostUnknown
      : `$${worker.costUsd.toFixed(4)}`;

  return (
    <aside className="worker-detail">
      <header className="worker-detail-head">
        <span className="worker-detail-title">
          {STR.workerDetailTitle} · #{worker.slot + 1}
        </span>
        <button type="button" className="worker-detail-close" onClick={onClose}>
          {STR.workerDetailClose}
        </button>
      </header>

      <div className="worker-detail-meta">
        <span className="worker-detail-pose">{POSE_LABEL[worker.pose]}</span>
        <span className="worker-detail-cost">
          {STR.workerDetailCost} {cost}
        </span>
      </div>

      <p className="worker-detail-action">{worker.detail}</p>

      <ul className="worker-detail-log">
        {worker.log.length === 0 && (
          <li className="worker-detail-log-empty">{STR.workerDetailLogEmpty}</li>
        )}
        {worker.log
          .slice()
          .reverse()
          .map(entry => (
            <li key={entry.seq} className="worker-detail-log-row">
              <span className="worker-detail-log-glyph">{KIND_GLYPH[entry.kind] ?? '•'}</span>
              <span className="worker-detail-log-kind">
                {EVENT_KIND_LABEL[entry.kind as AgentEvent['kind']] ?? entry.kind}
              </span>
              <span className="worker-detail-log-text">{entry.text}</span>
            </li>
          ))}
      </ul>
    </aside>
  );
}
