import type { StoredEvent } from '@/services/wire';
import type { PlacedWorker } from '@/office/workers';

interface WorksurfProps {
  worker: PlacedWorker | null;
  /** 该工人当前任务的事件流(working 时有) */
  events: StoredEvent[];
  onClose: () => void;
}

function title(w: PlacedWorker): string {
  if (w.role === 'mgr') return '经理 · 领导区';
  if (w.role === 'aud') return '独立审计 · 质检台';
  const n = Number(w.id.replace('emp', '')) + 1;
  return `员工 #${n}`;
}

function state(w: PlacedWorker): string {
  if (w.role === 'mgr') return '在领导区盯着公司 —— 读记忆、决策、派活。';
  if (w.role === 'aud') return '在质检台待命 —— 成果交来就在干净环境重验、查测试有没有被改弱。';
  if (w.working) return `在独立工作区敲键:${w.label ?? '任务'}。`;
  if (w.awaiting) return '等你拍板 —— 经理已复核,等下一步指令。';
  return '休息室待命 —— 没活时在这儿歇着。';
}

const num = (v: unknown): number => (typeof v === 'number' ? v : 0);
const str = (v: unknown): string => (typeof v === 'string' ? v : '');

/** 把一条 stored event 渲染成一行可读轨迹。 */
function eventLine(ev: StoredEvent): { text: string; cls: string } {
  let p: Record<string, unknown> = {};
  try {
    p = JSON.parse(ev.payloadJson) as Record<string, unknown>;
  } catch {
    // 非法 payload 退回 kind。
  }
  switch (ev.kind) {
    case 'worker_started':
      return { text: `开工 · ${str(p.model) || '模型未报'}`, cls: 'ev-meta' };
    case 'tool_use':
      return { text: `${str(p.tool) || '工具'} · ${str(p.summary)}`, cls: 'ev-tool' };
    case 'output_chunk':
      return { text: str(p.text).replace(/\s+/g, ' ').slice(0, 72), cls: 'ev-meta' };
    case 'result':
      return { text: `结果 · $${num(p.costUsd).toFixed(2)} · ${num(p.numTurns)} 轮`, cls: p.ok ? 'ev-ok' : 'ev-bad' };
    case 'error':
      return { text: `出错 · ${str(p.message)}`, cls: 'ev-bad' };
    case 'finished':
      return { text: `终态 · ${str(p.status)}`, cls: str(p.status) === 'verified' ? 'ev-ok' : 'ev-bad' };
    default:
      return { text: ev.kind, cls: 'ev-meta' };
  }
}

/**
 * 工作台 worksurf(原型 dive 落点):点小人 → 相机飞入 + 这张面板下钻其状态;working 工人
 * 再往里一层接 get_task_events 看真实事件轨迹(工具调用/输出/结果)。
 */
export function Worksurf({ worker, events, onClose }: WorksurfProps) {
  return (
    <div className={`worksurf${worker ? ' on' : ''}`}>
      {worker && (
        <>
          <div className="wbar">
            <span className="dot3">
              <i />
              <i />
              <i />
            </span>
            {title(worker)}
          </div>
          <div className="wmeta">{state(worker)}</div>
          {worker.taskId && events.length > 0 && (
            <div className="ws-events">
              {events.map(ev => {
                const line = eventLine(ev);
                return (
                  <div key={ev.seq} className={`ws-ev ${line.cls}`}>
                    {line.text}
                  </div>
                );
              })}
            </div>
          )}
          <div className="wfoot">
            <button className="pbtn go" type="button" onClick={onClose}>
              收起 ▾
            </button>
          </div>
        </>
      )}
    </div>
  );
}
