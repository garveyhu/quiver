import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from '@/App';
import './styles.css';

// DEV-ONLY: a fake Tauri bridge for headless screenshot/verification, activated
// by opening the dev server with `?mock`. Installed BEFORE the app mounts so the
// hooks' first invoke/listen hit the mock. Never reached in the packaged app.
if (import.meta.env.DEV && location.search.includes('mock')) {
  const { installDevMock } = await import('@/devMock');
  installDevMock();
}

const root = document.getElementById('root');
if (!root) throw new Error('root element not found');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
