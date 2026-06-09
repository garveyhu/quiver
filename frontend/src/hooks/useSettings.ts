import { useEffect, useState } from 'react';

import { getSettings } from '@/services/commands';
import type { Settings } from '@/services/wire';

/** 应用设置数据源:挂载时拉一次 get_settings(设置低频变化,不订阅)。 */
export function useSettings(): Settings | null {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    let alive = true;
    void getSettings()
      .then(s => {
        if (alive) setSettings(s);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  return settings;
}
