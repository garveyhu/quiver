/**
 * 任务链路条:把一个任务从派活到交付走成一条**看得见、有方向**的线 —— 入队 → 执行 → 验证
 * → 交付,当前阶段高亮脉动、已过的点亮、失败的标红。让 CEO 一眼看清"这活走到哪了",不用
 * 在事件流里翻。状态机口径与 run.rs / 经理编排一致。
 */

interface Stage {
  key: string;
  label: string;
}

const STAGES: Stage[] = [
  { key: 'queued', label: '入队' },
  { key: 'run', label: '执行' },
  { key: 'verify', label: '验证' },
  { key: 'deliver', label: '交付' },
];

/** task.status → 当前阶段下标 + 哪一阶段失败(null=没失败)。 */
function progress(status: string): { idx: number; failedAt: number | null } {
  switch (status) {
    case 'queued':
    case 'planned':
      return { idx: 0, failedAt: null };
    case 'running':
    case 'needs_input':
    case 'escalated':
      return { idx: 1, failedAt: null };
    case 'verifying':
    case 'verified':
      return { idx: 2, failedAt: null };
    case 'merged':
    case 'done':
      return { idx: 3, failedAt: null };
    case 'failed':
    case 'verify_failed':
      return { idx: 1, failedAt: 1 }; // 执行/验证没过
    case 'needs_rebase':
      return { idx: 3, failedAt: 3 }; // 交付受阻(冲突/重验红)
    case 'cancelled':
      return { idx: 0, failedAt: 0 };
    default:
      return { idx: 0, failedAt: null };
  }
}

export function TaskJourney({ status }: { status: string }) {
  const { idx, failedAt } = progress(status);
  const allDone = status === 'merged' || status === 'done';

  return (
    <div className="tj">
      {STAGES.map((s, i) => {
        const state =
          failedAt === i ? 'fail' : i < idx || allDone ? 'done' : i === idx ? 'active' : 'pending';
        return (
          <div className={`tj-step tj-${state}`} key={s.key}>
            <span className="tj-dot" />
            <span className="tj-label">{s.label}</span>
            {i < STAGES.length - 1 && <span className="tj-line" />}
          </div>
        );
      })}
    </div>
  );
}
