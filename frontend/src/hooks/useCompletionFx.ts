import { useEffect, useRef, useState } from 'react';

import { getStats } from '@/services/commands';
import { subscribe } from '@/services/ipc';

/** 一次交付完成的反馈类型:验收通过 / 打回。 */
export type CompletionFx = 'ok' | 'bad' | null;

/**
 * 交付时刻反馈:订阅 task-updated,比对 get_stats 的验收/失败计数 ——
 * 验收数 +1 → 'ok'(绿脉冲),失败数 +1 → 'bad'(红脉冲),瞬态约 1.2s 后清。
 * 移植原型派活终局的"审计通过→发货 / 打回"反馈。
 */
export function useCompletionFx(): CompletionFx {
  const [fx, setFx] = useState<CompletionFx>(null);
  const prev = useRef<{ verified: number; failed: number } | null>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;

    const flash = (t: CompletionFx) => {
      setFx(t);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => alive && setFx(null), 1200);
    };

    const check = async () => {
      try {
        const s = await getStats();
        if (!alive) return;
        const p = prev.current;
        if (p) {
          if (s.verified > p.verified) flash('ok');
          else if (s.failed > p.failed) flash('bad');
        }
        prev.current = { verified: s.verified, failed: s.failed };
      } catch {
        // 后端不可用时不反馈。
      }
    };

    void check(); // 建立基线,不闪。
    subscribe('task-updated', check).then(u => {
      if (alive) unlisten = u;
      else u();
    });

    return () => {
      alive = false;
      window.clearTimeout(timer.current);
      unlisten?.();
    };
  }, []);

  return fx;
}
