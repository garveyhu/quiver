import { StatusTag, CostTag, PALETTE, cssVar } from '@/assets';
import { TAG_OF, shortTime } from './util';

/** 状态 → 卡片左侧色条(一眼辨状态)。 */
const STATUS_BAR: Record<string, string> = {
  queued: cssVar('dim2'),
  running: cssVar('amber'),
  verifying: cssVar('blue'),
  verified: cssVar('green'),
  done: cssVar('green'),
  failed: cssVar('red'),
  verify_failed: cssVar('red'),
  needs_rebase: cssVar('amber'),
};

export interface TaskCardData {
  id: string;
  prompt: string;
  mode: string;
  status: string;
  costUsd?: number | null;
  createdAt: number;
}

export interface TaskCardProps {
  task: TaskCardData;
  selected?: boolean;
  /** 键盘光标所在(amber 加粗左条) */
  cursored?: boolean;
  /** 已完成的历史项:压暗显示 */
  dimmed?: boolean;
  onClick?: () => void;
  /** 仅排队中可撤回 */
  onCancel?: () => void;
  /** 拖拽排序(仅排队中)。draggable + 拖放回调由父级接线。 */
  draggable?: boolean;
  onDragStart?: (e: React.DragEvent) => void;
  onDragOver?: (e: React.DragEvent) => void;
  onDrop?: (e: React.DragEvent) => void;
  onDragEnd?: (e: React.DragEvent) => void;
  /** 拖到此卡上方时的落点提示(顶部 amber 线) */
  dropHint?: boolean;
}

/**
 * 委托卡片:两行布局(委托文案 + 状态标签 / 模式·费用·时间·撤回),左侧状态色条。
 * 取代原先又挤又"低级"的单行,信息分层、留白、可读。
 */
export function TaskCard({ task, selected, cursored, dimmed, onClick, onCancel, draggable, onDragStart, onDragOver, onDrop, onDragEnd, dropHint }: TaskCardProps) {
  const bar = cursored ? cssVar('amber') : STATUS_BAR[task.status] ?? cssVar('dim');
  return (
    <div
      onClick={onClick}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
      style={{
        position: 'relative',
        cursor: draggable ? 'grab' : 'pointer',
        padding: '7px 9px 7px 12px',
        background: selected ? PALETTE.wall : 'transparent',
        boxShadow: `inset ${cursored ? 4 : 3}px 0 0 ${bar}${dropHint ? ', inset 0 3px 0 ' + cssVar('amber') : ''}`,
        opacity: dimmed ? 0.5 : 1,
        borderBottom: '1px solid rgba(255,255,255,0.06)',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span style={{ flex: 1, fontSize: 11, color: cssVar('cream'), overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {task.prompt}
        </span>
        <StatusTag kind={TAG_OF[task.status] ?? 'paused'} />
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 4, fontSize: 9, color: PALETTE.dim }}>
        <span style={{ color: task.mode === 'real' ? cssVar('red') : cssVar('green') }}>
          {task.mode === 'real' ? '● 真实' : '● 模拟'}
        </span>
        {task.costUsd != null && <CostTag usd={task.costUsd} />}
        <span>{shortTime(task.createdAt)}</span>
        <span style={{ flex: 1 }} />
        {onCancel && (
          <span
            title={task.status === 'queued' ? '撤回' : '停止运行'}
            onClick={(e) => { e.stopPropagation(); onCancel(); }}
            style={{ cursor: 'pointer', color: task.status === 'queued' ? PALETTE.dim : cssVar('red') }}
          >
            {task.status === 'queued' ? '撤回 ✕' : '停止 ◼'}
          </span>
        )}
      </div>
    </div>
  );
}
