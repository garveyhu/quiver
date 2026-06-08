import { useState } from 'react';
import { Panel, StatusTag, CostTag, Button, TextField, PALETTE } from '@/assets';
import { useArchive } from '@/hooks/useArchive';
import type { StoredEvent } from '@/types/persistence.types';
import type { AgentEvent } from '@/types/agentEvent.types';
import { TAG_OF, shortTime } from './util';
import { Transcript } from './Transcript';

export interface ArchiveViewProps {
  /** 在工坊回放该任务(把事件喂给大厅) */
  onReplay?: (events: StoredEvent[]) => void;
}

/** 档案库:已完成委托列表 → 点开看事件卷轴,可在工坊回放。 */
export function ArchiveView({ onReplay }: ArchiveViewProps) {
  const { records, loadEvents } = useArchive();
  const [sel, setSel] = useState<string | null>(null);
  const [events, setEvents] = useState<StoredEvent[]>([]);
  const [q, setQ] = useState('');

  const open = async (id: string) => {
    setSel(id);
    setEvents(await loadEvents(id));
  };

  const filtered = q.trim()
    ? records.filter((r) => r.prompt.toLowerCase().includes(q.trim().toLowerCase()))
    : records;
  const selRecord = records.find((r) => r.id === sel);
  // 防御性解析:一条损坏的存储事件不该让整个档案视图崩溃,跳过即可。
  const selAgentEvents = events.flatMap((e) => {
    try {
      return [JSON.parse(e.payloadJson) as AgentEvent];
    } catch {
      return [];
    }
  });

  return (
    <div style={{ display: 'flex', gap: 14, alignItems: 'flex-start', flexWrap: 'wrap' }}>
      <Panel title={`档案库 (${filtered.length}${q.trim() ? `/${records.length}` : ''})`} width={340} bodyPadding={0}>
        <div style={{ padding: 8 }}>
          <TextField placeholder="搜索委托内容…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <div className="qv-scroll" style={{ maxHeight: 380, overflowY: 'auto' }}>
          {filtered.length === 0 && (
            <div style={{ padding: 14, color: PALETTE.dim, fontSize: 11 }}>
              {records.length === 0 ? '还没有完成的委托' : '没有匹配的委托'}
            </div>
          )}
          {filtered.map((r) => (
            <div
              key={r.id}
              className="qv-eventrow"
              style={{ cursor: 'pointer', background: sel === r.id ? PALETTE.wall : undefined }}
              onClick={() => void open(r.id)}
            >
              <span
                title={r.mode === 'real' ? '真实(消耗额度)' : '模拟(免费)'}
                style={{ flexShrink: 0, fontSize: 9, color: r.mode === 'real' ? PALETTE.red : PALETTE.green }}
              >
                {r.mode === 'real' ? '真' : '模'}
              </span>
              <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.prompt}</span>
              {r.costUsd != null && <CostTag usd={r.costUsd} />}
              <StatusTag kind={TAG_OF[r.status] ?? 'verified'} />
              <span style={{ fontSize: 9, color: PALETTE.dim2, flexShrink: 0 }}>{shortTime(r.createdAt)}</span>
            </div>
          ))}
        </div>
      </Panel>

      {sel && (
        <Panel title="委托卷轴 ▥" width={380} bodyPadding={0} onClose={() => setSel(null)}>
          {onReplay && (
            <div style={{ padding: 8 }}>
              <Button variant="ghost" onClick={() => onReplay(events)}>
                ▶ 在工坊回放
              </Button>
            </div>
          )}
          <div className="qv-scroll" style={{ maxHeight: 380, overflowY: 'auto' }}>
            {events.length === 0 ? (
              <div style={{ padding: 12, color: PALETTE.dim, fontSize: 11 }}>暂无事件</div>
            ) : (
              <Transcript
                events={selAgentEvents}
                prompt={selRecord?.prompt}
                mode={selRecord?.mode}
                branch={selRecord?.branch}
              />
            )}
          </div>
        </Panel>
      )}
    </div>
  );
}
