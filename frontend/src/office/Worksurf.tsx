import { useEffect, useState } from 'react';

import { MarkdownLite } from '@/components/MarkdownLite';
import { TaskJourney } from '@/components/TaskJourney';
import { getDecisions, listTasks } from '@/services/commands';
import type { AgentRole, ManagerDecision, StoredEvent } from '@/services/wire';

const OK = new Set(['verified', 'merged', 'done']);

/** 决策动作中文名 —— 经理面板里显示「拍了什么」。 */
const ACT_CN: Record<string, string> = {
  spawn: '派活',
  plan: '拆活分工',
  continue: '续跑改进',
  deliver: '验收交付',
  block: '拦下',
  escalate: '上报你',
  refresh_memory: '翻记忆',
  noop: '观望',
};

/** 一个员工的历史战绩(从所有任务里按 workerRole 聚合)。 */
interface Perf {
  total: number;
  ok: number;
  cost: number;
}
import type { PlacedWorker } from '@/office/workers';

/** 经理思考流里剥掉决策 JSON 对象(原生难读;决策已在「— 决策 —」分隔处人话显示),
 *  只留自然语言思路。决策 JSON 是单层的({"action":...}),非贪婪匹配到第一个 } 即够。 */
function stripDecisionJson(raw: string): string {
  return raw
    .replace(/\{[\s\S]*?"action"[\s\S]*?\}/g, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

interface WorksurfProps {
  worker: PlacedWorker | null;
  /** 该工人当前任务的事件流(working 时有) */
  events: StoredEvent[];
  /** 全部角色配置 —— 用 workerRole 关联出这个员工的专长,让"专长分工"具体可见。 */
  roles: AgentRole[];
  /** 经理实时思考流(点经理时显示):claude 决策过程的滚动文本,空=经理空闲/未用 claude。 */
  managerThinking?: string;
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
      <span className="trc-k think">思考</span>
      <div className="trc-t think">
        <div className={`trc-think-body${open || !long ? ' open' : ''}`}>
          <MarkdownLite text={text} />
        </div>
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
          <span className="trc-k start">开工</span>
          <span className="trc-t">模型 {str(p.model) || '?'}</span>
        </div>
      );
    case 'tool_use':
      return (
        <div className="trc-ev">
          <span className="trc-k tool">{str(p.tool) || '工具'}</span>
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
          <span className="trc-k done">跑完</span>
          <span className="trc-t">
            {num(p.numTurns)} 轮 · ${num(p.costUsd).toFixed(4)} · {Math.round(num(p.durationMs) / 1000)}s
          </span>
        </div>
      );
    case 'error':
      return (
        <div className="trc-ev">
          <span className="trc-k err">错误</span>
          <span className="trc-t st-bad">{str(p.message) || str(p.code)}</span>
        </div>
      );
    case 'finished':
      return (
        <div className="trc-ev">
          <span className="trc-k fin">终态</span>
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
export function Worksurf({ worker, events, roles, managerThinking, onClose, onOpenTrace }: WorksurfProps) {
  const hasTrace = !!worker?.taskId && events.length > 0;
  const specialty = worker ? specialtyOf(worker, roles) : undefined;
  // 这个小人对应的员工角色配置(休息员工身份卡用:专长/模型/大脑)。
  const role = worker?.workerRole ? roles.find(r => r.name === worker.workerRole) : undefined;
  // 休息中的员工(emp + 没在干活):点开看 ta 是谁、能力、怎么配 —— 不再只一句待命。
  const idleEmp = worker?.role === 'emp' && !worker.taskId;
  // 这个员工的历史战绩:干过多少件、成了多少、花了多少 —— 让 CEO 看团队每个人的真实表现。
  const [perf, setPerf] = useState<Perf | null>(null);
  const empName = idleEmp ? worker?.workerRole : undefined;
  useEffect(() => {
    if (!empName) {
      setPerf(null);
      return;
    }
    let alive = true;
    void listTasks()
      .then(ts => {
        if (!alive) return;
        const mine = ts.filter(t => t.workerRole === empName);
        setPerf({
          total: mine.length,
          ok: mine.filter(t => OK.has(t.status)).length,
          cost: mine.reduce((s, t) => s + (t.costUsd ?? 0), 0),
        });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [empName]);
  // 经理:点开看实时思考流(claude 决策过程),而非任务事件 —— 经理不绑单个任务。
  // 剥掉难读的决策 JSON,只留自然语言思路(规则经理/fake 只吐 JSON → 滤后为空,显示提示)。
  const isMgr = worker?.role === 'mgr';
  const thinking = stripDecisionJson(managerThinking ?? '');
  // 实时思考流只从「点开这一刻」起订阅,之前的决策没捕获 → 第一次点开常是空的。补拉经理最近的
  // 决策历史(get_decisions,落库、重启不丢),让点经理总能看到 ta 刚做了什么,而不是空白提示。
  const [recentDec, setRecentDec] = useState<ManagerDecision[]>([]);
  useEffect(() => {
    if (!isMgr) {
      setRecentDec([]);
      return;
    }
    let alive = true;
    void getDecisions(8)
      .then(d => alive && setRecentDec(d.filter(x => x.action !== 'noop').slice(0, 6)))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [isMgr]);
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
          <div className="wmeta">
            {isMgr
              ? thinking
                ? '正在用 claude 思考决策 —— 下面是它此刻的思路。'
                : '在领导区盯着公司。它一开始决策,这里就实时流出 claude 的思考过程。'
              : state(worker)}
          </div>
          {isMgr && (
            <div className="ws-events trc-events">
              {thinking ? (
                <div className="trc-ev">
                  <span className="trc-k think">经理思考</span>
                  <div className="trc-t think">
                    <div className="trc-think-body open">
                      <MarkdownLite text={thinking} />
                    </div>
                  </div>
                </div>
              ) : recentDec.length > 0 ? (
                <>
                  <div className="trc-ev st-meta">
                    经理此刻没在出声思考。下面是 ta 最近拍的决策 —— 点右上角「经理」看完整工作台。
                  </div>
                  {recentDec.map(d => (
                    <div className="trc-ev" key={`${d.seq}-${d.tsMs}`}>
                      <span className="trc-k">{ACT_CN[d.action] ?? d.action}</span>
                      <div className="trc-t">
                        {d.taskPrompt
                          ? `派的活:${d.taskPrompt.length > 44 ? `${d.taskPrompt.slice(0, 44)}…` : d.taskPrompt}`
                          : d.reason || '—'}
                      </div>
                    </div>
                  ))}
                </>
              ) : (
                <div className="trc-ev st-meta">
                  经理还没拍过决策。派一件事给公司(右上角「CEO 下目标」/⌘K),它就会开始决策;
                  把经理大脑切成 claude(人事部)能看到它实时的思考过程。
                </div>
              )}
            </div>
          )}
          {worker.taskId && !isMgr && worker.taskStatus && (
            <TaskJourney status={worker.taskStatus} />
          )}
          {hasTrace && !isMgr && (
            <div className="ws-events trc-events">
              {events.map(ev => (
                <EventRow key={ev.seq} ev={ev} />
              ))}
            </div>
          )}
          {idleEmp && (
            <div className="ws-id">
              <div className="ws-id-row">
                <span>专长</span>
                <b>{specialty ?? '通用'}</b>
              </div>
              <div className="ws-id-row">
                <span>模型</span>
                <b>{role?.model ?? '—'}</b>
              </div>
              <div className="ws-id-row">
                <span>大脑</span>
                <b>{role?.brain === 'claude' ? 'claude · 真思考(烧额度)' : '规则 · 免费'}</b>
              </div>
              <div className="ws-id-row">
                <span>战绩</span>
                <b>
                  {perf && perf.total > 0
                    ? `干过 ${perf.total} 件 · 成 ${perf.ok}(${Math.round((perf.ok / perf.total) * 100)}%) · 花 $${perf.cost.toFixed(2)}`
                    : '还没干过活'}
                </b>
              </div>
              <div className="ws-id-hint">没活时在休息室待命。去人事部能改 ta 的专长 / 模型 / 预算 / 大脑;有活时它会被派到对口的任务上。</div>
            </div>
          )}
          <div className="wfoot">
            {((worker.taskId && onOpenTrace) || idleEmp) && onOpenTrace && (
              <button
                className="pbtn"
                type="button"
                onClick={() => {
                  onClose();
                  onOpenTrace();
                }}
              >
                {idleEmp ? 'ta 干过的活 →' : '完整档案 →'}
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
