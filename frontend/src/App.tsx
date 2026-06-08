import { QuiverShell } from '@/shell';
import { SettingsProvider } from '@/shell/SettingsContext';

/**
 * App 外壳 —— 全像素 React UI(QuiverShell)。
 * SettingsProvider 让设置成为全应用单一数据源(看板预算 Banner / 默认模式
 * 与设置页实时同步,不再因各自 useSettings 实例分叉)。
 */
export function App() {
  return (
    <SettingsProvider>
      <QuiverShell />
    </SettingsProvider>
  );
}
