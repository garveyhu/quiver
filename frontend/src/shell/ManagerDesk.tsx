import { useState } from 'react';

import { useRoles } from '@/hooks/useRoles';
import type { Brief, Decision, ManagerDecision, ManagerPreview } from '@/services/wire';

interface ManagerDeskProps {
  open: boolean;
  decisions: ManagerDecision[];
  preview: ManagerPreview | null;
  brief: Brief | null;
  autonomous: boolean;
  /** 当前运行模式 —— 配合经理大脑判断是「claude·预演」还是「claude·真思考」。 */
  mode: 'simulate' | 'real';
  onToggleAutonomous: (on: boolean) => void;
  /** CEO 录入一条权威事实给公司(可带主题,同主题旧事实会被作废)。 */
  onAddFact: (text: string, topic?: string) => void;
  /** §12 回流:把被拦下/升级的任务打回重做(新任务入队,经理带记忆再派)。 */
  onRequeue: (taskId: string) => void;
  onClose: () => void;
}

/** 经理大脑当前状态的人话:规则免费 / claude 预演(simulate) / claude 真思考(real)。 */
function brainLabel(brain: string | null, mode: 'simulate' | 'real'): string {
  if (brain !== 'claude') return '规则 · 免费';
  return mode === 'real' ? 'claude · 真思考(烧额度)' : 'claude · 预演(免费)';
}

const ACTION_CN: Record<Decision['action'], string> = {
  spawn: '派活 ▸',
  plan: '拆活分工 ⑃',
  continue: '续跑',
  deliver: '交付',
  block: '拦下',
  escalate: '升级给你',
  refresh_memory: '刷新记忆',
  noop: '按兵不动',
};

function hhmmss(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** 决策的状态色:落地的派活/续跑=绿,拦下/升级=红,其余(noop/未落地)=中性。 */
function actionClass(d: ManagerDecision): string {
  if (!d.executed) return 'st-meta';
  if (d.action === 'spawn' || d.action === 'plan' || d.action === 'continue' || d.action === 'deliver') return 'st-ok';
  if (d.action === 'block' || d.action === 'escalate') return 'st-bad';
  return 'st-meta';
}

/** 一条决策的右侧补充:派的具体活 > 理由 > 调动到的任务 id。让"派活"不再抽象。 */
function detail(d: ManagerDecision): string {
  if (d.taskPrompt) {
    const t = d.taskPrompt.length > 42 ? `${d.taskPrompt.slice(0, 42)}…` : d.taskPrompt;
    return d.reason ? `「${t}」 · ${d.reason}` : `「${t}」`;
  }
  if (d.reason) return d.reason;
  if (d.taskId) return `→ ${d.taskId}`;
  return '';
}

/** 一拍的局面尾注:在途/上限 + 排队 + 经理这拍看到的预算 —— 决策依据可见。 */
function situation(d: ManagerDecision): string {
  const budget = d.budgetRemainingUsd >= 1_000_000 ? '预算充裕' : `预算$${d.budgetRemainingUsd.toFixed(2)}`;
  return `${d.inflight}/${d.maxInflight}在途 排队${d.queued} ${budget}`;
}

/**
 * 经理工作台 · 决策流(DESIGN §5):经理控制循环每一拍 emit 一条决策,这里逐拍显示 ——
 * 让 CEO **亲眼看到经理在自治、在基于决策调动员工**(派活/交付/拦下/升级),而不是黑箱跑。
 * 顶部是当前局面 + 自治开关(关=旧无脑流水线,开=经理驱动)。
 */
export function ManagerDesk({ open, decisions, preview, brief, autonomous, mode, onToggleAutonomous, onAddFact, onRequeue, onClose }: ManagerDeskProps) {
  const [showBrief, setShowBrief] = useState(false);
  // 打开工作台时拉最新角色,读经理大脑(在人事部/设置页切了能即时反映)。
  const { roles } = useRoles(open);
  const brain = roles.find(r => r.id === 'manager')?.brain ?? null;
  const briefCount = (brief?.facts.length ?? 0) + (brief?.recentEpisodes.length ?? 0);
  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>经理工作台 · 决策流</h2>
      <div className="sub">经理每一拍看局面、拍决策、调动员工 —— 这是它的实时决策流。每条都基于此刻的在途 / 排队 / 预算。</div>
      <div className="body">
        <div className="kv">
          <b>自治</b>
          <span>
            <button className={`pbtn${autonomous ? ' go' : ''}`} type="button" onClick={() => onToggleAutonomous(!autonomous)}>
              {autonomous ? '经理驱动 · 开' : '旧流水线 · 关'}
            </button>
            <span className="st-meta" style={{ marginLeft: 8 }}>
              {autonomous ? '经理控制循环在基于决策调度' : '打开后由经理拍决策派活,而非有排队就跑'}
            </span>
          </span>
        </div>
        <div className="kv">
          <b>大脑</b>
          <span>
            <span className={brain === 'claude' ? 'st-ok' : 'st-meta'}>{brainLabel(brain, mode)}</span>
            <span className="st-meta" style={{ marginLeft: 8 }}>
              下面这些决策由它拍 · 在人事部 / 设置页切
            </span>
          </span>
        </div>
        <div className="kv">
          <b>局面</b>
          <span>
            在途 {preview?.inflight ?? 0} / {preview?.maxInflight ?? 0} · 排队 {preview?.queued ?? 0} · 预算{' '}
            {preview && preview.budgetRemainingUsd >= 1_000_000 ? '充裕' : `剩 $${(preview?.budgetRemainingUsd ?? 0).toFixed(2)}`}
          </span>
        </div>
        <div className="kv">
          <b>记忆</b>
          <span>
            <button className="pbtn" type="button" onClick={() => setShowBrief(s => !s)}>
              {showBrief ? '收起简报 ▴' : `经理在想什么 ▾ (${briefCount} 条)`}
            </button>
          </span>
        </div>
        <div className="kv">
          <b>注入知识</b>
          <span>
            <input
              className="field role-num"
              style={{ width: 90, minWidth: 90 }}
              placeholder="主题(如 数据库)"
              id="md-fact-topic"
            />
            <input
              className="field"
              style={{ marginLeft: 6 }}
              placeholder="一条事实(如:数据库迁到 SQLite),回车录入"
              onKeyDown={e => {
                if (e.key === 'Enter') {
                  const v = (e.target as HTMLInputElement).value.trim();
                  if (v) {
                    const topicEl = document.getElementById('md-fact-topic') as HTMLInputElement | null;
                    const topic = topicEl?.value.trim() || undefined;
                    onAddFact(v, topic);
                    (e.target as HTMLInputElement).value = '';
                    if (topicEl) topicEl.value = '';
                  }
                }
              }}
            />
            <span className="st-meta" style={{ marginLeft: 8 }}>
              权威档·同主题旧事实会被作废
            </span>
          </span>
        </div>
        {showBrief &&
          (briefCount === 0 ? (
            <div className="rev st-meta">记忆还空 —— 跑几单任务,事实与裁决会沉淀到这里,经理决策时随简报注入。</div>
          ) : (
            <>
              {brief?.facts.map(f => (
                <div className="rev" key={`f${f.id}`}>
                  <span className="st-meta">事实·{f.trust}</span>
                  <span className="grow">{f.text}</span>
                  <span className="st-meta">重要度{f.importance}</span>
                </div>
              ))}
              {brief?.recentEpisodes.map(e => (
                <div className="rev" key={`e${e.id}`}>
                  <span className="st-meta">经历</span>
                  <span className="grow">{e.summary ?? '(无摘要)'}</span>
                  <span className={e.verifyResult === 'verified' ? 'st-ok' : 'st-meta'}>{e.verifyResult ?? '—'}</span>
                </div>
              ))}
            </>
          ))}

        {decisions.length === 0 ? (
          <div className="rev">
            {autonomous
              ? '经理待命中 —— 派一件事给公司(⌘K「派活」/「新任务」),这里就逐拍显示它的决策。'
              : '自治模式未开 —— 打开上面的开关,让经理控制循环驱动调度,决策流就会在这里流动。'}
          </div>
        ) : (
          decisions.map(d => (
            <div className="rev" key={`${d.seq}-${d.tsMs}`}>
              <span className="st-meta">{hhmmss(d.tsMs)}</span>
              <span className="st-meta">#{d.seq}</span>
              <span className={actionClass(d)}>{ACTION_CN[d.action] ?? d.action}</span>
              <span className="grow">{detail(d)}</span>
              {d.action === 'block' && d.taskId && (
                <button className="pbtn" type="button" onClick={() => onRequeue(d.taskId as string)}>
                  打回重做 ↻
                </button>
              )}
              <span className="st-meta">{situation(d)}</span>
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
