import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The web-api binary listens on 127.0.0.1:3000 and has no CORS layer, so the dev
// server proxies `/api/*` to it instead of the browser calling it cross-origin.
// `/graph` seeds blocks over RPC before answering, which can take minutes —
// hence the generous proxy timeouts.
const BACKEND = process.env.LEDGERSCOPE_API ?? 'http://127.0.0.1:3000';
const TEN_MINUTES = 10 * 60 * 1000;

export default defineConfig({
  plugins: [react()],
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
