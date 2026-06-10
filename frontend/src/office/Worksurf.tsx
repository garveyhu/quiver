import { useState } from 'react';

import type { AgentRole, StoredEvent } from '@/services/wire';
import type { PlacedWorker } from '@/office/workers';

interface WorksurfProps {
  worker: PlacedWorker | null;
  /** 该工人当前任务的事件流(working 时有) */
  events: StoredEvent[];
  /** 全部角色配置 —— 用 workerRole 关联出这个员工的专长,让"专长分工"具体可见。 */
  roles: AgentRole[];
  onClose: () => void;
  /** 跳到追溯室看这个员工/任务的完整档案(可选)。 */
  onOpenTrace?: () => void;
}

/** 真实员工身份:有 workerRole 用它(§14 谁在干),否则退回位置编号。 */
function title(w: PlacedWorker): string {
  if (w.role === 'mgr') return '经理 · 领导区';
  if (w.role === 'aud') return '独立审计 · 质检台';
  if (w.workerRole) return w.workerRole;
  const n = Number(w.id.replace('emp', '')) + 1;
  return `员工 #${n}`;
}

/** 这个员工的专长(从 roles 用 workerRole 关联),用于副标题"· 专长"。 */
function specialtyOf(w: PlacedWorker, roles: AgentRole[]): string | undefined {
  if (!w.workerRole) return undefined;
  const sp = roles.find(r => r.name === w.workerRole)?.specialty?.trim();
  return sp || undefined;
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

function parsePayload(raw: string): Record<string, unknown> {
  try {
    const v = JSON.parse(raw);
    return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** claude 思考输出:保留换行、默认折叠、长文可展开(不再压成一行)。 */
function ThinkText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = text.length > 120;
  return (
    <div className="trc-ev">
      <span className="trc-k think">💭 思考</span>
      <div className="trc-t think">
        <div className={`trc-think-body${open || !long ? ' open' : ''}`}>{text}</div>
        {long && (
          <button className="trc-more" type="button" onClick={() => setOpen(o => !o)}>
            {open ? '收起 ▴' : '展开 ▾'}
          </button>
        )}
      </div>
    </div>
  );
}

/** 一条事件 → 可读行(开工/思考/工具/结果/终态),和追溯室一致的风格。 */
function EventRow({ ev }: { ev: StoredEvent }) {
  const p = parsePayload(ev.payloadJson);
  switch (ev.kind) {
    case 'worker_started':
      return (
        <div className="trc-ev">
          <span className="trc-k start">▶ 开工</span>
          <span className="trc-t">模型 {str(p.model) || '?'}</span>
        </div>
      );
    case 'tool_use':
      return (
        <div className="trc-ev">
          <span className="trc-k tool">🔧 {str(p.tool) || '工具'}</span>
          <span className="trc-t">{str(p.summary)}</span>
        </div>
      );
    case 'output_chunk': {
      const text = str(p.text).trim();
      return text ? <ThinkText text={text} /> : null;
    }
    case 'result':
      return (
        <div className="trc-ev">
          <span className="trc-k done">✓ 跑完</span>
          <span className="trc-t">
            {num(p.numTurns)} 轮 · ${num(p.costUsd).toFixed(4)} · {Math.round(num(p.durationMs) / 1000)}s
          </span>
        </div>
      );
    case 'error':
      return (
        <div className="trc-ev">
          <span className="trc-k err">✗ 错误</span>
          <span className="trc-t st-bad">{str(p.message) || str(p.code)}</span>
        </div>
      );
    case 'finished':
      return (
        <div className="trc-ev">
          <span className="trc-k fin">● 终态</span>
          <span className="trc-t">{str(p.status)}</span>
        </div>
      );
    default:
      return null;
  }
}

/**
 * 员工工作台(点小人 → 下钻):这位员工此刻在干什么 + claude 实时轨迹(思考/工具/结果,
 * 可读折叠)。在干活的小人是活的 —— 看得到 ta 此刻一步步在做什么,不再只是一句"待命"。
 */
export function Worksurf({ worker, events, roles, onClose, onOpenTrace }: WorksurfProps) {
  const hasTrace = !!worker?.taskId && events.length > 0;
  const specialty = worker ? specialtyOf(worker, roles) : undefined;
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
            {specialty && <span className="ws-spec">· {specialty}</span>}
          </div>
          <div className="wmeta">{state(worker)}</div>
          {hasTrace && (
            <div className="ws-events trc-events">
              {events.map(ev => (
                <EventRow key={ev.seq} ev={ev} />
              ))}
            </div>
          )}
          <div className="wfoot">
            {worker.taskId && onOpenTrace && (
              <button
                className="pbtn"
                type="button"
                onClick={() => {
                  onClose();
                  onOpenTrace();
                }}
              >
                完整档案 →
              </button>
            )}
            <button className="pbtn go" type="button" onClick={onClose}>
              收起 ▾
            </button>
          </div>
        </>
      )}
    </div>
  );
}
