import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

// Tauri expects a fixed dev-server port (matched by tauri.conf.json's
// build.devUrl) and serves the web view from there in `cargo tauri dev`.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },
  // Tauri serves a fixed port; fail instead of silently picking another.
  clearScreen: false,
  server: {
    host: host || false,
    port: 1420,
    strictPort: true,
  },
  // Build into a dist/ that tauri.conf.json's frontendDist points at.
  build: {
    target: 'es2022',
    outDir: 'dist',
  },
});
