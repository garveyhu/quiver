import type { MorningReportData } from '@/hooks/useMorningReport';
import type { TaskStatus } from '@/services/wire';

interface MorningReportProps {
  open: boolean;
  data: MorningReportData;
  onClose: () => void;
}

function statusLabel(status: TaskStatus): string {
  if (status === 'verified' || status === 'done') return '审计通过';
  if (status === 'failed') return '打回 / 失败';
  return status;
}

function statusClass(status: TaskStatus): string {
  if (status === 'verified' || status === 'done') return 'st-ok';
  if (status === 'failed') return 'st-bad';
  return 'st-meta';
}

/** 晨报 · 过夜战报:汇总过夜验收/失败/花费 + 近期已结束委托清单。复用 .panel 叠层基础设施。 */
export function MorningReport({ open, data, onClose }: MorningReportProps) {
  const { stats, recent } = data;

  const sub = stats
    ? `昨晚完成 ${stats.verifiedDay} · 失败 ${stats.failedDay} · 今夜花费 $${stats.spentDay.toFixed(2)} · 当前 Lv${stats.level}（累计验收 ${stats.verified}）`
    : '读取中…';

  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>晨报 · 过夜战报</h2>
      <div className="sub">{sub}</div>
      <div className="body">
        {recent.length === 0 ? (
          <div className="rev">还没有已结束的委托。</div>
        ) : (
          recent.map(t => (
            <div className="rev" key={t.id}>
              <span className="grow">{t.prompt}</span>
              <span className={statusClass(t.status)}>{statusLabel(t.status)}</span>
              <span className="st-meta">{t.costUsd != null ? `$${t.costUsd.toFixed(2)}` : '—'}</span>
            </div>
          ))
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
