import { useEffect, useState } from 'react';

import { getMetrics } from '@/services/commands';
import type { MetricsDto } from '@/services/wire';

/** 指标数据源:面板打开时拉一次 get_metrics(只读聚合)。 */
export function useMetrics(open: boolean): MetricsDto | null {
  const [metrics, setMetrics] = useState<MetricsDto | null>(null);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void getMetrics()
      .then(m => {
        if (alive) setMetrics(m);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open]);

  return metrics;
}
