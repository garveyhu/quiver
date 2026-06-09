import type { EpisodeRecord } from '@/services/wire';

interface TimelineProps {
  open: boolean;
  episodes: EpisodeRecord[];
  onClose: () => void;
}

function hhmm(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

function verifyLabel(v: string | null): string {
  if (v === 'verified') return '审计通过';
  if (v === 'failed') return '打回';
  return v ?? '—';
}

function verifyClass(v: string | null): string {
  if (v === 'verified') return 'st-ok';
  if (v === 'failed') return 'st-bad';
  return 'st-meta';
}

/**
 * 整夜时间线 · 交付记录(接 get_episodes):按时间倒序列出过夜每笔交付 —— 时刻 + 目标 +
 * 验收结果 + diff/提交。原型的 scrub+ghost 位置回放依赖未记录的历史坐标,故这里落地为真实
 * episode 列表(同样回答"过夜发生了什么")。
 */
export function Timeline({ open, episodes, onClose }: TimelineProps) {
  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>整夜时间线 · 交付记录</h2>
      <div className="sub">公司过夜每一笔交付,按时间倒序。每条都机械绑 git(提交 + 验收结果)。</div>
      <div className="body">
        {episodes.length === 0 ? (
          <div className="rev">还没有交付记录 —— 派一件事给公司就会在这里留痕。</div>
        ) : (
          episodes.map(ep => (
            <div className="rev" key={ep.id}>
              <span className="st-meta tl-time">{hhmm(ep.createdAt)}</span>
              <span className="grow">{ep.summary ?? '(无摘要)'}</span>
              <span className={verifyClass(ep.verifyResult)}>{verifyLabel(ep.verifyResult)}</span>
              <span className="st-meta">{ep.diffStat?.trim() ? ep.diffStat : ep.commitSha ? ep.commitSha.slice(0, 7) : '—'}</span>
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
