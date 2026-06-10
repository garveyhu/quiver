import { useEffect, useState } from 'react';

import { subscribe } from '@/services/ipc';
import type { ManagerDecision, ManagerThinkingEvent } from '@/services/wire';

/** 思考流累积上限(字符):只留最近的,防长跑无限增长拖慢面板。 */
const MAX_CHARS = 4000;

/**
 * 经理思考流(可见性):订阅 `manager-thinking` —— 经理用 claude 决策时,claude 每吐一段思考
 * 就来一条增量片段,累积成实时滚动文本。同时订阅 `manager-decision`,每拍决策落定时插一条
 * 分隔(「— 决策:X —」),让 CEO 看清「想了一堆 → 拍了什么 → 又想」的节奏。
 *
 * 点开经理小人就能看到 ta 在想什么 —— 经理不再是黑箱。开关 `active` 关时不订阅(省开销)。
 */
export function useManagerThinking(active: boolean): string {
  const [thinking, setThinking] = useState('');

  useEffect(() => {
    if (!active) return;
    let alive = true;
    const unlisteners: Array<() => void> = [];

    const append = (chunk: string) =>
      setThinking(prev => {
        const next = prev + chunk;
        return next.length > MAX_CHARS ? next.slice(-MAX_CHARS) : next;
      });

    subscribe<ManagerThinkingEvent>('manager-thinking', ev => {
      if (alive) append(ev.text);
    }).then(u => (alive ? unlisteners.push(u) : u()));

    // 决策落定 → 插一条分隔,标出这拍想完拍了什么(noop 不刷屏)。
    subscribe<ManagerDecision>('manager-decision', d => {
      if (alive && d.action !== 'noop') append(`\n\n— 决策:${d.action}${d.reason ? `(${d.reason})` : ''} —\n\n`);
    }).then(u => (alive ? unlisteners.push(u) : u()));

    return () => {
      alive = false;
      unlisteners.forEach(u => u());
    };
  }, [active]);

  return thinking;
}
