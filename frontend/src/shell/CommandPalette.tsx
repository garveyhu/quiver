import { useEffect, useRef, useState, type KeyboardEvent } from 'react';

import { COMMANDS, type QuiverCommand } from '@/shell/commandRegistry';

interface CommandPaletteProps {
  open: boolean;
  onRun: (cmd: QuiverCommand) => void;
}

/** 命令栏(⌘K):模糊筛选 + 方向键导航 + Enter/点选执行。开合由 App 控制(常驻渲染，靠 .on 显隐)。 */
export function CommandPalette({ open, onRun }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const q = query.trim().toLowerCase();
  const filtered = q ? COMMANDS.filter(c => (c.label + c.hint).toLowerCase().includes(q)) : COMMANDS;

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setSel(0);
    const t = setTimeout(() => inputRef.current?.focus(), 30);
    return () => clearTimeout(t);
  }, [open]);

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (!filtered.length) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSel(s => (s + 1) % filtered.length);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSel(s => (s - 1 + filtered.length) % filtered.length);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const cmd = filtered[sel];
      if (cmd) onRun(cmd);
    }
  };

  return (
    <div className={`cmdk${open ? ' on' : ''}`}>
      <input
        ref={inputRef}
        value={query}
        onChange={e => {
          setQuery(e.target.value);
          setSel(0);
        }}
        onKeyDown={onKeyDown}
        placeholder="输入命令…  例如 新任务 / 验收 / 急停 / 设预算"
      />
      <div id="cmdk-list">
        {filtered.map((c, i) => (
          <div
            key={c.id}
            className={`row${i === sel ? ' sel' : ''}`}
            onMouseEnter={() => setSel(i)}
            onClick={() => onRun(c)}
          >
            <span>
              {c.label} <span className="cmdk-hint">· {c.hint}</span>
            </span>
            {c.kbd && <kbd>{c.kbd}</kbd>}
          </div>
        ))}
      </div>
    </div>
  );
}
