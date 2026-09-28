import { defineConfig } from 'vite';

export default defineConfig({
  // Relative asset paths so the built app works from any directory or host.
  base: './',
  worker: { format: 'es' },
  build: { target: 'es2022' },
  server: { port: 5173 },
});
