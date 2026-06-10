import { useEffect, useMemo, useState } from 'react';

import { useTaskEvents } from '@/hooks/useTaskEvents';
import { getDecisions, listTasks } from '@/services/commands';
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
};

function statusClass(status: string): string {
  if (status === 'merged' || status === 'verified' || status === 'done') return 'st-ok';
  if (status === 'failed' || status === 'needs_rebase' || status === 'cancelled') return 'st-bad';
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

/** 安全解析事件 payload(坏数据不崩,返回空对象)。 */
function parsePayload(raw: string): Record<string, unknown> {
  try {
    const v = JSON.parse(raw);
    return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** 一条事件渲染成人话:开工 / 思考 / 工具 / 结果 / 终态 / 错误。 */
function EventRow({ ev }: { ev: StoredEvent }) {
  const p = parsePayload(ev.payloadJson);
  switch (ev.kind) {
    case 'worker_started':
      return (
        <div className="trc-ev">
          <span className="trc-k start">▶ 开工</span>
          <span className="trc-t">模型 {String(p.model ?? '?')} · {String(p.authMode ?? '')}</span>
        </div>
      );
    case 'tool_use':
      return (
        <div className="trc-ev">
          <span className="trc-k tool">🔧 {String(p.tool ?? '工具')}</span>
          <span className="trc-t">{String(p.summary ?? '')}</span>
        </div>
      );
    case 'output_chunk': {
      const text = String(p.text ?? '').trim();
      if (!text) return null;
      return (
        <div className="trc-ev">
          <span className="trc-k think">💭 思考</span>
          <span className="trc-t think">{text}</span>
        </div>
      );
    }
    case 'result':
      return (
        <div className="trc-ev">
          <span className="trc-k done">✓ 跑完</span>
          <span className="trc-t">
            {Number(p.numTurns ?? 0)} 轮 · ${Number(p.costUsd ?? 0).toFixed(4)} ·{' '}
            {Math.round(Number(p.durationMs ?? 0) / 1000)}s
          </span>
        </div>
      );
    case 'error':
      return (
        <div className="trc-ev">
          <span className="trc-k err">✗ 错误</span>
          <span className="trc-t st-bad">{String(p.message ?? p.code ?? '')}</span>
        </div>
      );
    case 'finished':
      return (
        <div className="trc-ev">
          <span className="trc-k fin">● 终态</span>
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

/**
 * 追溯室(§10 复盘):系统跑过的每一个任务、每个员工干的每一步(claude 思考/工具/结果)、
 * 花了多少、最终结果如何 —— 一处可查、可追溯。回答"刚才那个任务到底发生了什么"。
 */
export function TraceRoom({ open, onClose }: TraceRoomProps) {
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [decisions, setDecisions] = useState<ManagerDecision[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

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

  return (
    <div className={`panel wide${open ? ' on' : ''}`}>
      <h2>追溯室 · 运行档案</h2>
      <div className="sub">系统跑过的每个任务、员工干的每一步、花费与结果 —— 点开看完整轨迹。</div>
      <div className="body">
        {tasks.length === 0 ? (
          <div className="rev st-meta">还没有任务。CEO 下个目标,跑完就能在这追溯每一步。</div>
        ) : (
          tasks.map(t => (
            <div className="trc-task" key={t.id}>
              <button
                className={`trc-head${selected === t.id ? ' on' : ''}`}
                type="button"
                onClick={() => setSelected(selected === t.id ? null : t.id)}
              >
                <span className="trc-arrow">{selected === t.id ? '▾' : '▸'}</span>
                <span className="grow">{t.prompt}</span>
                <span className={statusClass(t.status)}>{STATUS_CN[t.status] ?? t.status}</span>
                <span className="st-meta">
                  {t.mode === 'real' ? '真实' : '模拟'}
                  {t.costUsd ? ` · $${t.costUsd.toFixed(2)}` : ''}
                </span>
              </button>
              {selected === t.id && (
                <div className="trc-events">
                  {(decisionsByTask[t.id]?.length ?? 0) > 0 && (
                    <>
                      <div className="trc-sec">经理</div>
                      {decisionsByTask[t.id].map(d => (
                        <div className="trc-ev" key={`d${d.seq}-${d.tsMs}`}>
                          <span className="trc-k mgr">🧠 {DECISION_CN[d.action] ?? d.action}</span>
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
