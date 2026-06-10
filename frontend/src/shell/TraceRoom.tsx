import { Fragment, useEffect, useMemo, useState } from 'react';

import { useTaskEvents } from '@/hooks/useTaskEvents';
import { getDecisions, listTasks } from '@/services/commands';
import { MarkdownLite } from '@/components/MarkdownLite';
import { TaskJourney } from '@/components/TaskJourney';
import type { ManagerDecision, StoredEvent, TaskRecord } from '@/services/wire';

interface TraceRoomProps {
  open: boolean;
  onClose: () => void;
}

const STATUS_CN: Record<string, string> = {
  queued: '排队中',
  running: '干活中',
  verifying: '验收中',
  verified: '过验收·待合并',
  merged: '已合进 main',
  needs_rebase: '冲突待处理',
  failed: '失败',
  done: '完成',
  cancelled: '已取消',
  planned: '已拆解·分工中',
  needs_input: 'worker 请示中',
  escalated: '经理升级·等你',
};

function statusClass(status: string): string {
  if (status === 'merged' || status === 'verified' || status === 'done') return 'st-ok';
  if (status === 'failed' || status === 'needs_rebase' || status === 'cancelled') return 'st-bad';
  if (status === 'needs_input') return 'st-warn'; // worker 请示中,等经理回答 → 警示橙
  if (status === 'escalated') return 'st-warn';
  return 'st-meta';
}

/** 经理决策动作中文(追溯室里"经理这条线"的标签)。 */
const DECISION_CN: Record<string, string> = {
  spawn: '派活',
  continue: '续跑',
  deliver: '交付',
  block: '拦下',
  escalate: '升级等你',
  refresh_memory: '刷新记忆',
  noop: '按兵不动',
};

/** 一次展示的任务条数上限(防一屏渲染上千条卡死;超出靠搜索/筛选收窄)。 */
const RENDER_CAP = 60;

/** 安全解析事件 payload(坏数据不崩,返回空对象)。 */
function parsePayload(raw: string): Record<string, unknown> {
  try {
    const v = JSON.parse(raw);
    return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** claude 的思考输出:保留换行(markdown 原貌)、默认折叠、长文可展开 —— 一坨文本变可读。 */
function ThinkText({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const long = text.length > 180;
  return (
    <div className="trc-ev">
      <span className="trc-k think">思考</span>
      <div className="trc-t think">
        <div className={`trc-think-body${expanded || !long ? ' open' : ''}`}>
          <MarkdownLite text={text} />
        </div>
        {long && (
          <button className="trc-more" type="button" onClick={() => setExpanded(e => !e)}>
            {expanded ? '收起 ▴' : '展开全文 ▾'}
          </button>
        )}
      </div>
    </div>
  );
}

/** 一条事件渲染成人话:开工 / 思考 / 工具 / 结果 / 终态 / 错误。 */
function EventRow({ ev }: { ev: StoredEvent }) {
  const p = parsePayload(ev.payloadJson);
  switch (ev.kind) {
    case 'worker_started':
      return (
        <div className="trc-ev">
          <span className="trc-k start">开工</span>
          <span className="trc-t">模型 {String(p.model ?? '?')} · {String(p.authMode ?? '')}</span>
        </div>
      );
    case 'tool_use':
      return (
        <div className="trc-ev">
          <span className="trc-k tool">{String(p.tool ?? '工具')}</span>
          <span className="trc-t">{String(p.summary ?? '')}</span>
        </div>
      );
    case 'output_chunk': {
      const text = String(p.text ?? '').trim();
      if (!text) return null;
      return <ThinkText text={text} />;
    }
    case 'result':
      return (
        <div className="trc-ev">
          <span className="trc-k done">跑完</span>
          <span className="trc-t">
            {Number(p.numTurns ?? 0)} 轮 · ${Number(p.costUsd ?? 0).toFixed(4)} ·{' '}
            {Math.round(Number(p.durationMs ?? 0) / 1000)}s
          </span>
        </div>
      );
    case 'error':
      return (
        <div className="trc-ev">
          <span className="trc-k err">错误</span>
          <span className="trc-t st-bad">{String(p.message ?? p.code ?? '')}</span>
        </div>
      );
    case 'finished':
      return (
        <div className="trc-ev">
          <span className="trc-k fin">终态</span>
          <span className="trc-t">
            {STATUS_CN[String(p.status ?? '')] ?? String(p.status ?? '')}
            {p.branch ? ` · 分支 ${String(p.branch)}` : ''}
          </span>
        </div>
      );
    default:
      return null;
  }
}

/** 选中任务的事件轨迹(订阅式,实时填充)。返回 fragment,融入外层 .trc-events。 */
function TaskTrace({ taskId }: { taskId: string }) {
  const events = useTaskEvents(taskId);
  if (events.length === 0) {
    return <div className="trc-ev st-meta">这个任务还没有留下事件轨迹(simulate 旧任务或刚入队)。</div>;
  }
  return (
    <>
      {events.map(e => (
        <EventRow key={e.seq} ev={e} />
      ))}
    </>
  );
}

type ModeFilter = 'all' | 'real' | 'simulate';
type StatusFilter = 'all' | 'merged' | 'failed' | 'pending';

const STATUS_BUCKETS: Record<Exclude<StatusFilter, 'all'>, string[]> = {
  merged: ['merged'],
  failed: ['failed', 'needs_rebase', 'cancelled'],
  pending: ['queued', 'running', 'verifying', 'verified'],
};

/**
 * 追溯室(§10 复盘):系统跑过的每个任务、经理决策 + 每个员工干的每一步、花费与结果 ——
 * 支持文本搜索 / 模式 / 状态筛选,思考输出折叠可读,渲染封顶防大数据卡死。
 */
export function TraceRoom({ open, onClose }: TraceRoomProps) {
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [decisions, setDecisions] = useState<ManagerDecision[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [modeF, setModeF] = useState<ModeFilter>('all');
  const [statusF, setStatusF] = useState<StatusFilter>('all');
  const [workerF, setWorkerF] = useState<string>('all');

  useEffect(() => {
    if (!open) return;
    void listTasks()
      .then(ts => setTasks([...ts].sort((a, b) => b.createdAt - a.createdAt)))
      .catch(() => {});
    void getDecisions(200)
      .then(setDecisions)
      .catch(() => {});
  }, [open]);

  // 经理决策按任务归集 —— 一个任务展开时,先看经理为它做了什么(派活理由 / 复核裁定)。
  const decisionsByTask = useMemo(() => {
    const m: Record<string, ManagerDecision[]> = {};
    for (const d of decisions) {
      if (d.taskId) (m[d.taskId] ??= []).push(d);
    }
    for (const k of Object.keys(m)) m[k].sort((a, b) => a.tsMs - b.tsMs);
    return m;
  }, [decisions]);

  // 派过活的员工名单(从任务的 workerRole 去重),给"按人追溯"做筛选。
  const workers = useMemo(() => {
    const set = new Set<string>();
    for (const t of tasks) if (t.workerRole) set.add(t.workerRole);
    return [...set].sort();
  }, [tasks]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return tasks.filter(t => {
      if (q && !t.prompt.toLowerCase().includes(q)) return false;
      if (modeF !== 'all' && t.mode !== modeF) return false;
      if (statusF !== 'all' && !STATUS_BUCKETS[statusF].includes(t.status)) return false;
      if (workerF !== 'all' && t.workerRole !== workerF) return false;
      return true;
    });
  }, [tasks, query, modeF, statusF, workerF]);

  const shown = filtered.slice(0, RENDER_CAP);

  return (
    <div className={`panel wide${open ? ' on' : ''}`}>
      <h2>追溯室 · 运行档案</h2>
      <div className="sub">系统跑过的每个任务、经理决策 + 员工每一步、花费与结果。搜索 / 筛选 / 点开看轨迹。</div>
      <div className="trc-filter">
        <input
          className="field trc-search"
          placeholder="搜任务内容…"
          value={query}
          onChange={e => setQuery(e.target.value)}
        />
        <div className="trc-chips">
          {(['all', 'real', 'simulate'] as ModeFilter[]).map(m => (
            <button
              key={m}
              className={`trc-chip${modeF === m ? ' on' : ''}`}
              type="button"
              onClick={() => setModeF(m)}
            >
              {m === 'all' ? '全部' : m === 'real' ? '真实' : '模拟'}
            </button>
          ))}
          <span className="trc-chip-sep" />
          {(['all', 'merged', 'pending', 'failed'] as StatusFilter[]).map(s => (
            <button
              key={s}
              className={`trc-chip${statusF === s ? ' on' : ''}`}
              type="button"
              onClick={() => setStatusF(s)}
            >
              {s === 'all' ? '全部' : s === 'merged' ? '已合并' : s === 'pending' ? '进行中' : '失败'}
            </button>
          ))}
          {workers.length > 0 && (
            <>
              <span className="trc-chip-sep" />
              <select
                className="trc-chip trc-worker-sel"
                value={workerF}
                onChange={e => setWorkerF(e.target.value)}
              >
                <option value="all">全部员工</option>
                {workers.map(w => (
                  <option key={w} value={w}>
                    {w}
                  </option>
                ))}
              </select>
            </>
          )}
        </div>
      </div>
      <div className="body">
        {tasks.length === 0 ? (
          <div className="rev st-meta">还没有任务。CEO 下个目标,跑完就能在这追溯每一步。</div>
        ) : filtered.length === 0 ? (
          <div className="rev st-meta">没有匹配的任务,换个搜索词或筛选。</div>
        ) : (
          <>
            <div className="trc-count">
              {filtered.length} 条
              {filtered.length > RENDER_CAP ? `(显示前 ${RENDER_CAP},搜索缩小范围)` : ''}
              {filtered.length !== tasks.length ? ` · 共 ${tasks.length}` : ''}
            </div>
            {shown.map((t, i) => {
              // 协作树:同 parentGoal 的子任务前插一个目标组头,子任务缩进 —— 一眼看出"这几个
              // 是同一个目标拆出来分工的"(§5 协作)。
              const groupHead = t.parentGoal && t.parentGoal !== shown[i - 1]?.parentGoal;
              const sibs = t.parentGoal ? shown.filter(x => x.parentGoal === t.parentGoal) : [];
              const sibDone = sibs.filter(x => ['verified', 'merged', 'done'].includes(x.status)).length;
              const allDone = sibs.length > 0 && sibDone === sibs.length;
              // 参与协作的员工(从子任务 workerRole 去重)—— 一眼看出谁在这个目标上分工。
              const sibWorkers = [...new Set(sibs.map(x => x.workerRole).filter(Boolean))];
              return (
                <Fragment key={t.id}>
                  {groupHead && (
                    <div className="trc-goal-head">
                      协作目标：{t.parentGoal!.length > 22 ? `${t.parentGoal!.slice(0, 22)}…` : t.parentGoal} ·{' '}
                      <span className={allDone ? 'st-ok' : 'st-meta'}>
                        {sibDone}/{sibs.length} 完成{allDone ? '' : ''}
                      </span>
                      {sibWorkers.length > 0 && (
                        <span className="trc-goal-team"> · {sibWorkers.join('、')} 分工</span>
                      )}
                    </div>
                  )}
                  <div className={`trc-task${t.parentGoal ? ' trc-sub' : ''}`}>
                <button
                  className={`trc-head${selected === t.id ? ' on' : ''}`}
                  type="button"
                  onClick={() => setSelected(selected === t.id ? null : t.id)}
                >
                  <span className="trc-arrow">{selected === t.id ? '▾' : '▸'}</span>
                  <span className="grow">{t.prompt}</span>
                  {t.workerRole && <span className="trc-by">{t.workerRole}</span>}
                  <span className={statusClass(t.status)}>{STATUS_CN[t.status] ?? t.status}</span>
                  <span className="st-meta">
                    {t.mode === 'real' ? '真实' : '模拟'}
                    {t.costUsd ? ` · $${t.costUsd.toFixed(2)}` : ''}
                  </span>
                </button>
                {selected === t.id && (
                  <div className="trc-events">
                    <TaskJourney status={t.status} />
                    {t.question &&
                      (t.status === 'escalated' ? (
                        <div className="trc-ask">
                          经理升级给你:「{t.question}」 —— 补充信息后重派,经理就能继续
                        </div>
                      ) : (
                        <div className="trc-ask">
                          worker 卡住请示:「{t.question}」 —— 经理 continue 回答后,它带着答案继续
                        </div>
                      ))}
                    {(decisionsByTask[t.id]?.length ?? 0) > 0 && (
                      <>
                        <div className="trc-sec">经理</div>
                        {decisionsByTask[t.id].map(d => (
                          <div className="trc-ev" key={`d${d.seq}-${d.tsMs}`}>
                            <span className="trc-k mgr">{DECISION_CN[d.action] ?? d.action}</span>
                            <span className="trc-t">{d.reason || (d.taskPrompt ?? '')}</span>
                          </div>
                        ))}
                        <div className="trc-sec">员工</div>
                      </>
                    )}
                    <TaskTrace taskId={t.id} />
                  </div>
                )}
                  </div>
                </Fragment>
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
