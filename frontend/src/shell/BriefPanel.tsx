import { Panel, PALETTE } from '@/assets';
import { useBrief } from '@/hooks/useBrief';

/** 可信度档 → 颜色(§6.2):权威=琥珀,已验证=绿,员工汇报=暗,不可信=红。 */
function trustColor(trust: string): string {
  if (trust === '权威') return PALETTE.amber;
  if (trust.startsWith('已验证')) return PALETTE.green;
  if (trust === '不可信') return PALETTE.red;
  return PALETTE.dim;
}

/** episode 终态 → 颜色:通过=绿,失败=红,其余=暗。 */
function verdictColor(verdict: string | null): string {
  if (verdict === 'verified' || verdict === 'done') return PALETTE.green;
  if (verdict === 'failed' || verdict === 'verify_failed') return PALETTE.red;
  return PALETTE.dim;
}

const sectionLabel: React.CSSProperties = {
  padding: '7px 9px 4px',
  fontSize: 9,
  letterSpacing: 1,
  color: PALETTE.dim2,
};

const rowTag: React.CSSProperties = {
  fontSize: 9,
  whiteSpace: 'nowrap',
  flexShrink: 0,
};

const rowText: React.CSSProperties = {
  color: PALETTE.dim,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
};

/**
 * 经理「记忆书 · 简报」(DESIGN §6 简报 / §22 领导区):当前真相事实(按
 * importance·trust·recency)+ 近期 episode。数据来自 `get_brief`(useBrief)。
 * 记忆为空(新项目 / 还没跑过)时不渲染,免占 Rail 空间。
 */
export function BriefPanel() {
  const { brief } = useBrief();
  const facts = brief?.facts ?? [];
  const episodes = brief?.recentEpisodes ?? [];
  if (facts.length === 0 && episodes.length === 0) return null;

  return (
    <Panel title="记忆书 · 简报" width={300} bodyPadding={0}>
      <div className="qv-scroll" style={{ maxHeight: '26vh', overflowY: 'auto', paddingBottom: 4 }}>
        {facts.length > 0 && (
          <>
            <div style={sectionLabel}>当前事实 · {facts.length}</div>
            {facts.map(f => (
              <div
                key={f.id}
                style={{ padding: '3px 10px', fontSize: 11, display: 'flex', gap: 6, alignItems: 'baseline' }}
              >
                <span style={{ ...rowTag, color: trustColor(f.trust) }}>
                  {f.trust}·{f.importance}
                </span>
                <span style={{ ...rowText, color: PALETTE.cream }}>{f.text}</span>
              </div>
            ))}
          </>
        )}
        {episodes.length > 0 && (
          <>
            <div
              style={{
                ...sectionLabel,
                borderTop: facts.length ? '1px solid rgba(255,255,255,0.06)' : undefined,
                marginTop: facts.length ? 4 : 0,
              }}
            >
              近期 · {episodes.length}
            </div>
            {episodes.map(e => (
              <div
                key={e.id}
                style={{ padding: '3px 10px', fontSize: 11, display: 'flex', gap: 6, alignItems: 'baseline' }}
              >
                <span style={{ ...rowTag, color: verdictColor(e.verifyResult) }}>
                  {e.verifyResult ?? '?'}
                </span>
                <span style={rowText}>{e.summary ?? ''}</span>
              </div>
            ))}
          </>
        )}
      </div>
    </Panel>
  );
}
