import { useMemoryRoom } from '@/hooks/useMemoryRoom';
import type { FactRecord } from '@/services/wire';

interface MemoryRoomProps {
  open: boolean;
  onClose: () => void;
  /** 录入/作废后给 CEO 的状态反馈。 */
  onToast: (msg: string) => void;
}

/** 可信度档的视觉权重(高档更醒目)。 */
function trustClass(trust: string): string {
  if (trust === '权威') return 'mem-t-auth';
  if (trust.startsWith('已验证')) return 'mem-t-verified';
  if (trust === '员工汇报') return 'mem-t-report';
  return 'mem-t-low';
}

/** 可信度排序权重(§6.2,大=更可信,记忆库里高档在前)。 */
function trustRank(trust: string): number {
  if (trust === '权威') return 5;
  if (trust === '已验证·机械') return 4;
  if (trust === '已验证·印证') return 3;
  if (trust === '员工汇报') return 2;
  return 1;
}

export function MemoryRoom({ open, onClose, onToast }: MemoryRoomProps) {
  const { facts, addFact, retire } = useMemoryRoom(open);
  const sorted = [...facts].sort(
    (a, b) => trustRank(b.trust) - trustRank(a.trust) || b.importance - a.importance,
  );

  const submit = (textEl: HTMLInputElement, topicEl: HTMLInputElement | null) => {
    const text = textEl.value.trim();
    if (!text) return;
    const topic = topicEl?.value.trim() || undefined;
    void addFact(text, topic)
      .then(() => onToast(`已注入记忆:「${text}」${topic ? `(主题 ${topic})` : ''}`))
      .catch(e => onToast(`录入失败:${e instanceof Error ? e.message : String(e)}`));
    textEl.value = '';
    if (topicEl) topicEl.value = '';
  };

  return (
    <div className={`panel wide${open ? ' on' : ''}`}>
      <h2>记忆库 · 公司知识</h2>
      <div className="sub">公司当前记住的所有事实(按可信度排)。CEO 可注入新知识、作废记错/过时的。经理决策时读这些。</div>
      <div className="trc-filter">
        <input className="field role-num" style={{ width: 96, minWidth: 96 }} placeholder="主题(可选)" id="mem-topic" />
        <input
          className="field"
          style={{ marginLeft: 6 }}
          placeholder="录入一条公司事实(如:数据库用 SQLite),回车"
          onKeyDown={e => {
            if (e.key === 'Enter') {
              submit(e.target as HTMLInputElement, document.getElementById('mem-topic') as HTMLInputElement | null);
            }
          }}
        />
        <span className="st-meta" style={{ marginLeft: 8 }}>
          权威档·同主题旧事实会被作废
        </span>
      </div>
      <div className="body">
        {sorted.length === 0 ? (
          <div className="rev st-meta">记忆还空 —— 录入一条事实,或跑几单任务让公司沉淀知识。</div>
        ) : (
          <>
            <div className="trc-count">{sorted.length} 条当前事实</div>
            {sorted.map((f: FactRecord) => (
              <div className="rev mem-fact" key={f.id}>
                <span className={`mem-trust ${trustClass(f.trust)}`}>{f.trust}</span>
                <span className="grow">{f.text}</span>
                {f.entity && <span className="mem-topic">{f.entity}</span>}
                <span className="st-meta">重{f.importance}</span>
                <button
                  className="pbtn mem-retire"
                  type="button"
                  title="作废这条(记错了/过时了)"
                  onClick={() =>
                    void retire(f.id)
                      .then(() => onToast(`已作废:「${f.text}」`))
                      .catch(e => onToast(`作废失败:${e instanceof Error ? e.message : String(e)}`))
                  }
                >
                  作废
                </button>
              </div>
            ))}
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
