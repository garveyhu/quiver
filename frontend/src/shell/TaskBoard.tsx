import { useTaskBoard } from '@/hooks/useTaskBoard';

interface TaskBoardProps {
  open: boolean;
  onClose: () => void;
}

const STATUS_CN: Record<string, string> = {
  queued: '排队中',
  running: '干活中',
  verifying: '验收中',
};

function statusClass(status: string): string {
  if (status === 'running') return 'st-ok';
  if (status === 'verifying') return 'st-meta';
  return 'st-meta';
}

/**
 * 任务看板 / 调度台(§11):CEO 掌控自治公司的工作队列 —— 看排队/在跑的活,调优先级
 * (上移/下移,经理按 position 认领)、取消。让"绝对自治"也在 CEO 手里。
 */
export function TaskBoard({ open, onClose }: TaskBoardProps) {
  const { tasks, moveUp, moveDown, cancel } = useTaskBoard(open);

  return (
    <div className={`panel wide${open ? ' on' : ''}`}>
      <h2>调度台 · 工作队列</h2>
      <div className="sub">排队 / 在跑的活。排队的可调优先级(经理按这个顺序认领)、取消。在跑的可叫停。</div>
      <div className="body">
        {tasks.length === 0 ? (
          <div className="rev st-meta">队列空 —— 没有排队或在跑的活。CEO 下个目标试试。</div>
        ) : (
          <>
            <div className="trc-count">{tasks.length} 个活跃任务</div>
            {tasks.map((t, i) => {
              const queued = t.status === 'queued';
              return (
                <div className="rev tb-task" key={t.id}>
                  <span className="tb-pos">{i + 1}</span>
                  <span className="grow">{t.prompt}</span>
                  <span className={statusClass(t.status)}>{STATUS_CN[t.status] ?? t.status}</span>
                  {queued && (
                    <>
                      <button
                        className="pbtn tb-mv"
                        type="button"
                        title="上移(更早被派)"
                        disabled={i === 0 || tasks[i - 1].status !== 'queued'}
                        onClick={() => void moveUp(i)}
                      >
                        ▲
                      </button>
                      <button
                        className="pbtn tb-mv"
                        type="button"
                        title="下移(更晚被派)"
                        disabled={i === tasks.length - 1}
                        onClick={() => void moveDown(i)}
                      >
                        ▼
                      </button>
                    </>
                  )}
                  <button
                    className="pbtn tb-cancel"
                    type="button"
                    title={queued ? '从队列移除' : '叫停在跑的活'}
                    onClick={() => void cancel(t.id)}
                  >
                    {queued ? '移除' : '叫停'}
                  </button>
                </div>
              );
            })}
          </>
        )}
      </div>
      <div className="foot">
        <button className="pbtn go" type="button" onClick={onClose}>
          进办公室
        </button>
      </div>
    </div>
  );
}
