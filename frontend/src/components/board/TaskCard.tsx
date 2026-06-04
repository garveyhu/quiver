import { STR, TASK_STATUS_LABEL } from '@/strings';
import type { TaskRecord } from '@/types/persistence.types';

interface TaskCardProps {
  task: TaskRecord;
  /** Flash on first appearance (a freshly-pinned commission). */
  fresh: boolean;
  /** True while this card is the active drag source (dimmed). */
  dragging: boolean;
  onCancel: (id: string) => void;
  onDragStart: (id: string) => void;
  onDragEnter: (id: string) => void;
  onDragEnd: () => void;
}

// queued tasks are the only droppable/cancellable ones — running/finished
// commissions are owned by a live worker and can't be reordered or pulled.
function isQueued(status: string): boolean {
  return status === 'queued';
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
  return cost === null ? STR.taskCostUnknown : `${STR.taskCostPrefix}${cost.toFixed(2)}`;
}

/**
 * One pinned parchment commission card on the bulletin board: prompt, mode
 * badge, live status badge, running cost, and (real-mode, un-merged) the
 * worktree branch. Queued cards are draggable (reorder) + cancellable; running
 * and finished cards are read-only.
 */
export function TaskCard({
  task,
  fresh,
  dragging,
  onCancel,
  onDragStart,
  onDragEnter,
  onDragEnd,
}: TaskCardProps) {
  const queued = isQueued(task.status);
  const modeLabel = task.mode === 'real' ? STR.taskModeReal : STR.taskModeSimulate;
  const statusLabel = TASK_STATUS_LABEL[task.status] ?? task.status;

  const classes = [
    'task-card',
    fresh ? 'task-card-fresh' : '',
    dragging ? 'task-card-dragging' : '',
    queued ? 'task-card-queued' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <li
      className={classes}
      draggable={queued}
      onDragStart={() => queued && onDragStart(task.id)}
      onDragEnter={() => onDragEnter(task.id)}
      onDragEnd={onDragEnd}
      onDragOver={e => e.preventDefault()}
      title={queued ? STR.taskDragTitle : undefined}
    >
      <span className="task-card-pin" aria-hidden />

      <div className="task-card-head">
        <span className={`task-badge ${statusClass(task.status)}`}>{statusLabel}</span>
        <span className={`task-mode task-mode-${task.mode}`}>{modeLabel}</span>
        {queued && (
          <button
            type="button"
            className="task-cancel"
            data-tip={STR.taskCancelTitle}
            aria-label={STR.taskCancel}
            onClick={() => onCancel(task.id)}
          >
            ✕
          </button>
        )}
      </div>

      <p className="task-card-prompt">{task.prompt}</p>

      <div className="task-card-foot">
        <span className="task-card-cost">{costText(task.costUsd)}</span>
        {task.branch && (
          <span className="task-card-branch" title={task.branch}>
            {STR.taskBranchPrefix}
            {task.branch}
          </span>
        )}
      </div>
    </li>
  );
}
