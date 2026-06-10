import { useCallback, useEffect, useState } from 'react';

import { listRoles, updateRole } from '@/services/commands';
import type { AgentRole, RolePatch } from '@/services/wire';

export interface RolesState {
  roles: AgentRole[];
  /** 增量改一个角色;成功后用后端返回的完整行原位刷新。 */
  patchRole: (id: string, patch: RolePatch) => Promise<void>;
}

/** 人事部数据源:面板打开时拉全部角色配置;patch 后原位更新(version 由后端 +1)。 */
export function useRoles(open: boolean): RolesState {
  const [roles, setRoles] = useState<AgentRole[]>([]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void listRoles()
      .then(r => alive && setRoles(r))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open]);

  const patchRole = useCallback(async (id: string, patch: RolePatch) => {
    const updated = await updateRole(id, patch);
    setRoles(prev => prev.map(r => (r.id === updated.id ? updated : r)));
  }, []);

  return { roles, patchRole };
}
