import { useCallback, useEffect, useState } from 'react';

import { addAuthoritativeFact, getMemoryFacts, retireFact } from '@/services/commands';
import type { FactRecord } from '@/services/wire';

export interface MemoryRoomState {
  facts: FactRecord[];
  /** CEO 录入一条权威事实(可带主题),刷新列表。 */
  addFact: (text: string, topic?: string) => Promise<void>;
  /** 作废一条记错/过时的事实,刷新列表。 */
  retire: (factId: number) => Promise<void>;
}

/** 记忆库数据源:打开时拉所有当前事实,提供录入/作废(都即时刷新)。 */
export function useMemoryRoom(open: boolean): MemoryRoomState {
  const [facts, setFacts] = useState<FactRecord[]>([]);

  const refresh = useCallback(() => {
    void getMemoryFacts()
      .then(setFacts)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  const addFact = useCallback(
    async (text: string, topic?: string) => {
      await addAuthoritativeFact(text, topic);
      refresh();
    },
    [refresh],
  );

  const retire = useCallback(
    async (factId: number) => {
      await retireFact(factId);
      refresh();
    },
    [refresh],
  );

  return { facts, addFact, retire };
}
