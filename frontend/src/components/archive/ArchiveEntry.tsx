import { STR, TASK_STATUS_LABEL } from '@/strings';
import type { TaskRecord } from '@/types/persistence.types';

interface ArchiveEntryProps {
  record: TaskRecord;
  onOpen: (record: TaskRecord) => void;
}

function statusClass(status: string): string {
  switch (status) {
    case 'queued':
      return 'task-status-queued';
    case 'running':
      return 'task-status-running';
    case 'verifying':
      return 'task-status-verifying';
    case 'verified':
    case 'done':
      return 'task-status-done';
    default:
      return 'task-status-failed';
  }
}

function costText(cost: number | null): string {
  return cost === null ? STR.archiveCostUnknown : `${STR.archiveCostPrefix}${cost.toFixed(2)}`;
}

function projectName(project: string): string {
  const parts = project.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? project;
}

function timeLabel(createdAt: number): string {
  return new Date(createdAt).toLocaleString();
}

/**
 * One bound "委托记录" on the shelf: a parchment ledger row showing the prompt,
 * status badge, mode, cost, project, branch (real-mode), and time. Clicking it
 * opens the full Logbook. Kept clearly scannable (clarity > theme for dense
 * data) while keeping the cozy bound-book spine motif.
 */
export function ArchiveEntry({ record, onOpen }: ArchiveEntryProps) {
  const modeLabel = record.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate;
  const statusLabel = TASK_STATUS_LABEL[record.status] ?? record.status;

  return (
    <li className="archive-entry">
      <button
        type="button"
        className="archive-entry-btn"
        title={STR.archiveOpenTip}
        onClick={() => onOpen(record)}
      >
        <span className="archive-entry-spine" aria-hidden />
        <div className="archive-entry-body">
          <div className="archive-entry-head">
            <span className={`task-badge ${statusClass(record.status)}`}>{statusLabel}</span>
            <span className={`task-mode task-mode-${record.mode}`}>{modeLabel}</span>
            <span className="archive-entry-project" title={record.project}>
              {projectName(record.project)}
            </span>
          </div>

          <p className="archive-entry-prompt">{record.prompt}</p>

          <div className="archive-entry-foot">
            <span className="archive-entry-cost">{costText(record.costUsd)}</span>
            <span className="archive-entry-time">{timeLabel(record.createdAt)}</span>
            {record.branch && (
              <span className="archive-entry-branch" title={record.branch}>
                {STR.archiveBranchPrefix}
                {record.branch}
              </span>
            )}
          </div>
        </div>
      </button>
    </li>
  );
}
