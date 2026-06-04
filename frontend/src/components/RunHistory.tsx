import type { RunRecord } from '@/types/persistence.types';
import { RUN_STATUS_LABEL, STR } from '@/strings';

interface RunHistoryProps {
  history: RunRecord[];
}

function statusLabel(status: string): string {
  return RUN_STATUS_LABEL[status] ?? status;
}

function costLabel(costUsd: number | null): string {
  return costUsd == null ? STR.historyCostUnknown : `${STR.historyCostPrefix}${costUsd.toFixed(2)}`;
}

function timeLabel(createdAt: number): string {
  return new Date(createdAt).toLocaleString();
}

/**
 * The durable run-history list (DESIGN §11): past tasks with prompt, terminal
 * status, summed cost, and time. Newest first. Survives an app restart.
 */
export function RunHistory({ history }: RunHistoryProps) {
  return (
    <div className="run-history">
      <h3 className="panel-subtitle">{STR.historyHeader}</h3>
      {history.length === 0 ? (
        <p className="panel-empty">{STR.historyEmpty}</p>
      ) : (
        <ul className="history-list">
          {history.map(run => (
            <li key={run.id} className="history-item">
              <p className="history-prompt" title={run.prompt}>
                {run.prompt}
              </p>
              <div className="history-meta">
                <span className={`history-status history-status-${run.status}`}>
                  {statusLabel(run.status)}
                </span>
                <span className="history-mode">{run.mode}</span>
                <span className="history-cost">{costLabel(run.costUsd)}</span>
                <span className="history-time">{timeLabel(run.createdAt)}</span>
              </div>
              {run.branch && (
                <p className="history-branch">
                  {STR.historyBranchPrefix}
                  {run.branch}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
