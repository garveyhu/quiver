import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from '@/App';
// 全局像素主题由 shell.css + assets/pixel.css 提供(随 App 模块图加载)。

// DEV-ONLY: a fake Tauri bridge for headless screenshot/verification, activated
// by opening the dev server with `?mock`. Installed BEFORE the app mounts so the
// hooks' first invoke/listen hit the mock. Never reached in the packaged app.
if (import.meta.env.DEV && location.search.includes('mock')) {
  const { installDevMock } = await import('@/devMock');
  installDevMock();
}

const root = document.getElementById('root');
if (!root) throw new Error('root element not found');

// DEV-ONLY: 像素美术组件预览画廊,用 `?preview=assets` 打开(不影响正式应用)。
let tree = <App />;
if (import.meta.env.DEV && location.search.includes('preview=assets')) {
  const { AssetsPreview } = await import('@/assets/preview/AssetsPreview');
  tree = <AssetsPreview />;
}

ReactDOM.createRoot(root).render(<React.StrictMode>{tree}</React.StrictMode>);
