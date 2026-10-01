import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    target: "es2022", commonjsOptions: { include: [/node_modules/, /generated\/.*\.cjs$/] } },
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: false
  }
});

