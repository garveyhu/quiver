import { useCallback, useEffect, useState } from 'react';

import { getSettings, updateSettings } from '@/services/commands';
import type { Settings, SettingsPatch } from '@/services/wire';

export interface SettingsState {
  settings: Settings | null;
  /** 改一项设置(增量),成功后用后端返回的完整设置刷新。 */
  patch: (p: SettingsPatch) => Promise<void>;
}

/** 应用设置数据源:挂载时拉一次 get_settings,提供增量 patch(系统设置页用)。 */
export function useSettings(): SettingsState {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    let alive = true;
    void getSettings()
      .then(s => alive && setSettings(s))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  const patch = useCallback(async (p: SettingsPatch) => {
    const s = await updateSettings(p);
    setSettings(s);
  }, []);

  return { settings, patch };
}
