import type { AgentRole, RolePatch } from '@/services/wire';

interface PersonnelDeskProps {
  open: boolean;
  roles: AgentRole[];
  onPatch: (id: string, patch: RolePatch) => void;
  /** §14 雇一个新员工。 */
  onHire: () => void;
  /** §14 裁掉一个员工(仅员工)。 */
  onFire: (id: string) => void;
  onClose: () => void;
}

const KIND_CN: Record<string, string> = {
  manager: '经理 · 领导区',
  worker: '员工 · 工位区',
  librarian: '记忆官 · 记忆库',
};

/** 各角色「大脑」开关的双态文案:关(免费/休眠) vs claude(烧 headless 额度)。 */
const BRAIN_CN: Record<string, [string, string]> = {
  manager: ['规则 · 免费', 'claude · 真思考'],
  librarian: ['休眠 · 不提炼', 'claude · 提炼事实'],
};

/** 数字输入的解析:空串=显式清除(null),非法=不动(undefined)。 */
function parseNum(raw: string): number | null | undefined {
  const t = raw.trim();
  if (t === '') return null;
  const n = Number(t);
  return Number.isFinite(n) && n >= 0 ? n : undefined;
}

/**
 * 人事部(DESIGN §14):每个人物一张配置卡 —— 经理卡带「大脑」开关(rule 免费规则 /
 * claude 真想·走 headless 额度,**烧钱大脑只能在这里显式打开**),员工卡的模型/预算/
 * 轮数真接到 run 路径。每次改动 version+1,卡片上可见。
 */
export function PersonnelDesk({ open, roles, onPatch, onHire, onFire, onClose }: PersonnelDeskProps) {
  const workerCount = roles.filter(r => r.kind === 'worker').length;
  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>人事部 · 角色配置</h2>
      <div className="sub">每个人物的大脑、模型与硬限。改动立即生效(经理下一拍 / 员工下一个任务),版本号随改动 +1。</div>
      <div className="body">
        {roles.length === 0 ? (
          <div className="rev">读取中…</div>
        ) : (
          roles.map(r => (
            <div className="rev role-card" key={r.id}>
              <div className="role-head">
                <b>{r.name}</b>
                <span className="st-meta"> {KIND_CN[r.kind] ?? r.kind} · v{r.version}</span>
                {/* 只能裁员工,且至少留一个(不能裁到没人干活)。 */}
                {r.kind === 'worker' && workerCount > 1 && (
                  <button className="pbtn" type="button" style={{ float: 'right' }} onClick={() => onFire(r.id)}>
                    裁员 ✕
                  </button>
                )}
              </div>
              {BRAIN_CN[r.kind] && (
                <div className="kv">
                  <b>大脑</b>
                  <span>
                    <button
                      className={`pbtn${r.brain === 'rule' ? ' go' : ''}`}
                      type="button"
                      onClick={() => onPatch(r.id, { brain: 'rule' })}
                    >
                      {BRAIN_CN[r.kind][0]}
                    </button>
                    <button
                      className={`pbtn${r.brain === 'claude' ? ' go' : ''}`}
                      type="button"
                      style={{ marginLeft: 6 }}
                      onClick={() => onPatch(r.id, { brain: 'claude' })}
                    >
                      {BRAIN_CN[r.kind][1]}
                    </button>
                    {r.brain === 'claude' && (
                      <span className="st-bad" style={{ marginLeft: 8 }}>
                        {r.kind === 'manager' ? '每拍决策都走 headless 额度' : '每次提炼走 headless 额度(交付后限频)'}
                      </span>
                    )}
                  </span>
                </div>
              )}
              <div className="kv">
                <b>模型</b>
                <span>
                  <input
                    className="field"
                    defaultValue={r.model}
                    onBlur={e => {
                      const v = e.target.value.trim();
                      if (v && v !== r.model) onPatch(r.id, { model: v });
                    }}
                  />
                </span>
              </div>
              {r.kind === 'worker' && (
                <div className="kv">
                  <b>专长</b>
                  <span>
                    <input
                      className="field"
                      defaultValue={r.specialty}
                      placeholder="通用 / 测试 / 前端 / 安全…"
                      onBlur={e => {
                        const v = e.target.value.trim();
                        if (v !== r.specialty) onPatch(r.id, { specialty: v });
                      }}
                    />
                    <span className="st-meta" style={{ marginLeft: 8 }}>
                      任务描述含此词 → 经理派给他
                    </span>
                  </span>
                </div>
              )}
              {r.kind === 'worker' && (
                <div className="kv">
                  <b>硬限</b>
                  <span>
                    单任务 $
                    <input
                      className="field role-num"
                      defaultValue={r.budgetUsd ?? ''}
                      placeholder="不限"
                      onBlur={e => {
                        const v = parseNum(e.target.value);
                        if (v !== undefined && v !== r.budgetUsd) onPatch(r.id, { budgetUsd: v });
                      }}
                    />
                    · 轮数
                    <input
                      className="field role-num"
                      defaultValue={r.maxTurns ?? ''}
                      placeholder="不限"
                      onBlur={e => {
                        const v = parseNum(e.target.value);
                        if (v !== undefined && v !== r.maxTurns) onPatch(r.id, { maxTurns: v });
                      }}
                    />
                  </span>
                </div>
              )}
            </div>
          ))
        )}
      </div>
      <div className="foot">
        <button className="pbtn" type="button" onClick={onHire}>
          + 雇个员工
        </button>
        <button className="pbtn go" type="button" onClick={onClose}>
          进办公室
        </button>
      </div>
    </div>
  );
}
