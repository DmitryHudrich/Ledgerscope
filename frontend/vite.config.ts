import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const BACKEND = process.env.LEDGERSCOPE_API ?? 'http://127.0.0.1:3000';
const TEN_MINUTES = 10 * 60 * 1000;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: BACKEND,
        changeOrigin: true,
        timeout: TEN_MINUTES,
        proxyTimeout: TEN_MINUTES,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
});
