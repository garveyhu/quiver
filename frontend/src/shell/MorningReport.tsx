import { Panel, StatTile, PALETTE } from '@/assets';
import type { Stats } from '@/hooks/useStats';
import type { TaskRecord } from '@/types/persistence.types';

interface MorningReportProps {
  stats: Stats | null;
  /** 近期已完成的任务(终态),最新在前。 */
  tasks: TaskRecord[];
  onClose: () => void;
}

/** 任务终态 → 颜色 + 中文判词。 */
function verdict(status: string): { color: string; label: string } {
  if (status === 'verified' || status === 'done') return { color: PALETTE.green, label: '通过' };
  if (status === 'verify_failed') return { color: PALETTE.red, label: '验证未过' };
  if (status === 'failed') return { color: PALETTE.red, label: '失败' };
  return { color: PALETTE.dim, label: status };
}

/**
 * 晨报 · 昨晚战报(DESIGN §"晨报/验收"):把过夜运行的结果摊给你看 —— 进度统计
 * (来自 get_stats)+ 近期完成的委托清单。数据复用现成的 useStats / 看板任务,
 * 不新开 IPC。纯 CSS 像素风。
 */
export function MorningReport({ stats, tasks, onClose }: MorningReportProps) {
  const cost = stats ? `$${stats.spentDay.toFixed(2)}` : '$0.00';
  return (
    <Panel title="晨报 · 昨晚战报 ☀" width={460} onClose={onClose}>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: 12 }}>
        <StatTile value={stats?.verified ?? 0} label="已完成" />
        <StatTile value={stats?.failed ?? 0} label="失败" />
        <StatTile value={cost} label="今夜花费" />
        <StatTile value={`Lv${stats?.level ?? 1}`} label="等级" />
      </div>

      <div style={{ fontSize: 9, letterSpacing: 1, color: PALETTE.dim2, padding: '4px 0 6px' }}>
        近期完成 · {tasks.length}
      </div>
      {tasks.length === 0 ? (
        <div style={{ color: PALETTE.dim, fontSize: 11, padding: '6px 0' }}>
          昨晚静悄悄 —— 还没有完成的委托。
        </div>
      ) : (
        <div className="qv-scroll" style={{ maxHeight: '42vh', overflowY: 'auto' }}>
          {tasks.map(t => {
            const v = verdict(t.status);
            return (
              <div
                key={t.id}
                style={{
                  display: 'flex',
                  gap: 8,
                  alignItems: 'baseline',
                  padding: '4px 2px',
                  borderTop: '1px solid rgba(255,255,255,0.05)',
                }}
              >
                <span style={{ color: v.color, fontSize: 9, whiteSpace: 'nowrap', flexShrink: 0 }}>
                  {v.label}
                </span>
                <span
                  style={{
                    color: PALETTE.cream,
                    fontSize: 11,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    flex: 1,
                  }}
                >
                  {t.prompt}
                </span>
                {t.costUsd != null && (
                  <span style={{ color: PALETTE.dim, fontSize: 10, whiteSpace: 'nowrap' }}>
                    ${t.costUsd.toFixed(2)}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </Panel>
  );
}
