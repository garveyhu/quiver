import { useEffect, useState } from 'react';

import { getEpisodes } from '@/services/commands';
import type { EpisodeRecord } from '@/services/wire';

/** 时间线数据源:面板打开时拉一次近期 episode(只读)。 */
export function useEpisodes(open: boolean): EpisodeRecord[] {
  const [episodes, setEpisodes] = useState<EpisodeRecord[]>([]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void getEpisodes(12)
      .then(e => {
        if (alive) setEpisodes(e);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open]);

  return episodes;
}
