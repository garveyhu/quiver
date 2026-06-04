import { useMemo, useRef, useState } from 'react';
import { STR } from '@/strings';
import type { TaskRecord } from '@/types/persistence.types';
import { TaskCard } from '@/components/board/TaskCard';
import { CozyEmpty } from '@/components/CozyEmpty';

interface TaskBoardProps {
  tasks: TaskRecord[];
  freshIds: Set<string>;
  maxWorkers: number;
  onReorder: (id: string, position: number) => void;
  onCancel: (id: string) => void;
}

// A queued card dropped above `targetId` should land at a position strictly
// below the card above it and above the target — we just hand the scheduler a
// fractional position; `list_tasks` re-sorts by position so any value between
// neighbours works. Use the target's position minus a small epsilon.
const POSITION_EPSILON = 0.5;

/**
 * The 任务公告板: a timber notice-board of pinned parchment commission cards.
 * Shows a live concurrency summary (running / queued vs maxWorkers), renders one
 * card per task, and supports HTML5 drag-to-reorder for queued cards (running /
 * finished cards are fixed). Status, cost, and order all update live off the
 * `task-updated` stream the parent hook listens to.
 */
export function TaskBoard({ tasks, freshIds, maxWorkers, onReorder, onCancel }: TaskBoardProps) {
  // The drag source/target are kept in refs (not state) so the dragend handler
  // reads the CURRENT source even when dragstart/dragend fire in the same
  // synchronous event burst — React state would still be stale there. A separate
  // `dragId` state drives only the visual dimming of the source card.
  const [dragId, setDragId] = useState<string | null>(null);
  const dragIdRef = useRef<string | null>(null);
  const overIdRef = useRef<string | null>(null);

  const runningCount = useMemo(
    () => tasks.filter(t => t.status === 'running' || t.status === 'verifying').length,
    [tasks],
  );
  const queuedCount = useMemo(() => tasks.filter(t => t.status === 'queued').length, [tasks]);

  const handleDragStart = (id: string) => {
    dragIdRef.current = id;
    setDragId(id);
  };

  const handleDragEnter = (id: string) => {
    overIdRef.current = id;
  };

  const handleDragEnd = () => {
    const source = dragIdRef.current;
    const target = overIdRef.current;
    dragIdRef.current = null;
    overIdRef.current = null;
    setDragId(null);
    if (!source || !target || source === target) return;
    const targetTask = tasks.find(t => t.id === target);
    if (!targetTask) return;
    // Drop just ahead of the target card in board order.
    onReorder(source, targetTask.position - POSITION_EPSILON);
  };

  return (
    <section className="task-board">
      <header className="task-board-head">
        <h2 className="task-board-title">{STR.boardTitle}</h2>
        <p className="task-board-subtitle">{STR.boardSubtitle}</p>
        <div className="task-board-meta">
          <span className="task-board-chip task-board-chip-running">
            {STR.boardRunningPrefix}
            {runningCount}/{maxWorkers}
          </span>
          <span className="task-board-chip task-board-chip-queued">
            {STR.boardQueuedPrefix}
            {queuedCount}
          </span>
        </div>
      </header>

      {tasks.length === 0 ? (
        <div className="task-board-empty">
          <CozyEmpty glyph="📌" title={STR.boardEmptyTitle} hint={STR.boardEmpty} />
        </div>
      ) : (
        <ul className="task-board-list">
          {tasks.map(task => (
            <TaskCard
              key={task.id}
              task={task}
              fresh={freshIds.has(task.id)}
              dragging={dragId === task.id}
              onCancel={onCancel}
              onDragStart={handleDragStart}
              onDragEnter={handleDragEnter}
              onDragEnd={handleDragEnd}
            />
          ))}
        </ul>
      )}
    </section>
  );
}
