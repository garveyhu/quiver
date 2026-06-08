import { createContext, useContext } from 'react';
import type { ReactNode } from 'react';
import { useSettings } from '@/hooks/useSettings';

/** 整个 useSettings 返回值(settings / saveStatus / patch 等)的形状。 */
type SettingsValue = ReturnType<typeof useSettings>;

const SettingsContext = createContext<SettingsValue | null>(null);

/**
 * 全应用唯一的 settings 实例。放在根部,QuiverShell 与 SettingsView 共用同一份
 * 状态——这样在设置里改预算上限,看板的预算 Banner / 默认模式能立刻同步,不再
 * 因为各自 `useSettings()` 实例而分叉。
 */
export function SettingsProvider({ children }: { children: ReactNode }) {
  const value = useSettings();
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettingsContext(): SettingsValue {
  const ctx = useContext(SettingsContext);
  if (!ctx) throw new Error('useSettingsContext 必须在 SettingsProvider 内使用');
  return ctx;
}
