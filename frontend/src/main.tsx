import React from 'react';
import ReactDOM from 'react-dom/client';

import { App } from '@/App';

const root = document.getElementById('root');
if (!root) throw new Error('root 容器缺失');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
