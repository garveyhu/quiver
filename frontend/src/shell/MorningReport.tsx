import type { MorningReportData } from '@/hooks/useMorningReport';
import type { TaskRecord } from '@/services/wire';

interface MorningReportProps {
  open: boolean;
  data: MorningReportData;
  /** §12 一站式处理:把卡住/升级的任务直接打回重做(经理带记忆再派)。 */
  onRequeue: (taskId: string) => void;
  /** CEO 接受合并:把一个 verified 且有分支的任务合进 main(§12)。 */
  onMerge: (taskId: string) => void;
  onClose: () => void;
}

function cost(t: TaskRecord): string {
  return t.costUsd != null ? `$${t.costUsd.toFixed(2)}` : '—';
}

/**
 * 晨报 · 过夜战报(§12 验收台):你 8 点醒来的界面。按**结局**分三栏 —— 合进 main 的
 * (公司真推进了项目)、卡住等你的、升级等你拍板的 —— 一眼看清"昨晚公司干成了什么、
 * 还有什么需要我"。复用 .panel 叠层基础设施。
 */
export function MorningReport({ open, data, onRequeue, onMerge, onClose }: MorningReportProps) {
  const { stats, mergedToMain, needsYou, escalations, recent } = data;

  const sub = stats
    ? `昨晚合进 main ${mergedToMain.length} · 等你处理 ${needsYou.length + escalations.length} · 今夜花费 $${stats.spentDay.toFixed(2)} · Lv${stats.level}`
    : '读取中…';

  const nothing =
    mergedToMain.length === 0 && needsYou.length === 0 && escalations.length === 0 && recent.length === 0;

  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>晨报 · 过夜战报</h2>
      <div className="sub">{sub}</div>
      <div className="body">
        {nothing && <div className="rev">昨晚还没有结果 —— 派一件事给公司,过夜跑完这里就是你的验收台。</div>}

        {escalations.length > 0 && (
          <>
            <div className="kv">
              <b className="st-bad">⚠ 升级等你拍板</b>
              <span className="st-meta">超出经理自治能力,等你决定</span>
            </div>
            {escalations.map(d => (
              <div className="rev" key={`esc-${d.seq}-${d.tsMs}`}>
                <span className="grow">{d.reason ?? '(未给原因)'}</span>
                {d.taskId && (
                  <button className="pbtn" type="button" onClick={() => onRequeue(d.taskId as string)}>
                    打回重做 ↻
                  </button>
                )}
                <span className="st-meta">{d.taskId ?? ''}</span>
              </div>
            ))}
          </>
        )}

        {needsYou.length > 0 && (
          <>
            <div className="kv">
              <b className="st-bad">卡住 · 等你处理</b>
              <span className="st-meta">冲突待 rebase / 打回 —— main 没动</span>
            </div>
            {needsYou.map(t => (
              <div className="rev" key={t.id}>
                <span className="grow">{t.prompt}</span>
                <span className="st-bad">{t.status === 'needs_rebase' ? '冲突待处理' : '打回 / 失败'}</span>
                <button className="pbtn" type="button" onClick={() => onRequeue(t.id)}>
                  打回重做 ↻
                </button>
                <span className="st-meta">{cost(t)}</span>
              </div>
            ))}
          </>
        )}

        {mergedToMain.length > 0 && (
          <>
            <div className="kv">
              <b className="st-ok">✓ 合进 main</b>
              <span className="st-meta">公司昨晚真正推进了项目的部分</span>
            </div>
            {mergedToMain.map(t => (
              <div className="rev" key={t.id}>
                <span className="grow">{t.prompt}</span>
                <span className="st-ok">已合并</span>
                <span className="st-meta">{cost(t)}</span>
              </div>
            ))}
          </>
        )}

        {recent.length > 0 && (
          <>
            <div className="kv">
              <b>验收通过</b>
              <span className="st-meta">已过验收(simulate / 待你接受合并)</span>
            </div>
            {recent.map(t => {
              // real 任务过验收后产物留在分支上,等 CEO 点头合进 main(§12 接受)。simulate 无分支。
              const mergeable = t.status === 'verified' && !!t.branch;
              return (
                <div className="rev" key={t.id}>
                  <span className="grow">{t.prompt}</span>
                  <span className="st-ok">通过</span>
                  <span className="st-meta">{cost(t)}</span>
                  {mergeable && (
                    <button className="pbtn go" type="button" onClick={() => onMerge(t.id)}>
                      合进 main ✓
                    </button>
                  )}
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
