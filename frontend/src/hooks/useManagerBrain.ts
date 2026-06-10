import { useCallback, useEffect, useState } from 'react';

import { listRoles, updateRole } from '@/services/commands';

export interface ManagerBrainState {
  /** 当前经理大脑:'rule'(规则·免费) / 'claude'(真思考·烧钱)。未知时 null。 */
  brain: string | null;
  /** 切换经理大脑(复用人事部 update_role,下一拍即生效)。 */
  setBrain: (b: 'rule' | 'claude') => Promise<void>;
}

/**
 * 经理大脑的读/切 —— 系统设置页用,让"聪明的自治"开关(经理是否用 claude 思考)和运行模式
 * 在一处可配,不必再单独跑去人事部。本质是 role('manager').brain 的快捷面。
 */
export function useManagerBrain(): ManagerBrainState {
  const [brain, setBrainState] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void listRoles()
      .then(rs => {
        const m = rs.find(r => r.id === 'manager');
        if (alive && m) setBrainState(m.brain);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  const setBrain = useCallback(async (b: 'rule' | 'claude') => {
    const m = await updateRole('manager', { brain: b });
    setBrainState(m.brain);
  }, []);

  return { brain, setBrain };
}
