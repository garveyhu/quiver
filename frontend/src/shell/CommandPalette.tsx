import { useEffect, useMemo, useRef, useState } from 'react';
import { Panel, cssVar, PALETTE } from '@/assets';
import type { RunMode } from '@/types/run.types';

export interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  /** 额外可搜索关键词 */
  keywords?: string;
  run: () => void;
}

export interface CommandPaletteProps {
  onClose: () => void;
  /** 主操作:用输入文本派活(父级决定模拟/真实 + 二次确认,并负责关闭面板)。 */
  onEnqueue: (text: string) => void;
  mode: RunMode;
  onToggleMode: () => void;
  /** 导航 / 项目 等静态命令 */
  commands: PaletteCommand[];
}

/**
 * ⌘K 命令面板:键盘优先的统一入口。输入文本即可派活(列表第一项),或搜索/方向键
 * 选择跳转命令。⇧⏎ 切模拟/真实,↵ 执行,Esc 关。是底坞 Dock 的快捷投影。
 */
export function CommandPalette({ onClose, onEnqueue, mode, onToggleMode, commands }: CommandPaletteProps) {
  const [q, setQ] = useState('');
  const [idx, setIdx] = useState(0);

  const items = useMemo<PaletteCommand[]>(() => {
    const query = q.trim();
    const lower = query.toLowerCase();
    const filtered = commands.filter(
      (c) => !lower || `${c.label} ${c.keywords ?? ''}`.toLowerCase().includes(lower),
    );
    const primary: PaletteCommand[] = query
      ? [
          {
            id: '__enqueue',
            label: `派活(${mode === 'real' ? '真实' : '模拟'}):${query}`,
            hint: '↵ 钉上去 · ⇧↵ 切模式',
            run: () => onEnqueue(query),
          },
        ]
      : [];
    return [...primary, ...filtered];
  }, [q, commands, mode, onEnqueue]);

  const active = Math.min(idx, Math.max(0, items.length - 1));
  const listRef = useRef<HTMLDivElement>(null);

  // 方向键选中项滚入视野(列表长时)。
  useEffect(() => {
    const row = listRef.current?.children[active] as HTMLElement | undefined;
    row?.scrollIntoView({ block: 'nearest' });
  }, [active]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setIdx((i) => Math.min(i + 1, items.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) {
        onToggleMode();
        return;
      }
      items[active]?.run();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div
      className="qv-backdrop"
      onClick={onClose}
      style={{ position: 'fixed', inset: 0, display: 'flex', alignItems: 'flex-start', justifyContent: 'center', paddingTop: '14vh', zIndex: 80 }}
    >
      <div onClick={(e) => e.stopPropagation()}>
        <Panel title="命令 ⌘K" width={460} bodyPadding={0}>
          <div style={{ padding: 8 }}>
            <input
              className="qv-input"
              autoFocus
              value={q}
              placeholder="输入文字派活,或搜索命令…  (⇧↵ 切模式)"
              onChange={(e) => {
                setQ(e.target.value);
                setIdx(0);
              }}
              onKeyDown={onKeyDown}
              style={{ width: '100%' }}
            />
          </div>
          <div ref={listRef} className="qv-scroll" style={{ maxHeight: '46vh', overflowY: 'auto' }}>
            {items.length === 0 && (
              <div style={{ padding: 14, color: PALETTE.dim, fontSize: 11 }}>没有匹配的命令</div>
            )}
            {items.map((it, i) => (
              <div
                key={it.id}
                className="qv-eventrow"
                style={{ cursor: 'pointer', background: i === active ? PALETTE.wall : undefined }}
                onMouseEnter={() => setIdx(i)}
                onClick={() => it.run()}
              >
                <span style={{ flex: 1, color: i === active ? cssVar('amber') : cssVar('cream'), overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {it.label}
                </span>
                {it.hint && <span style={{ fontSize: 9, color: PALETTE.dim2, flexShrink: 0 }}>{it.hint}</span>}
              </div>
            ))}
          </div>
        </Panel>
      </div>
    </div>
  );
}
