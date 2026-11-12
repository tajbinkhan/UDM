import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';

// One page per window kind (AGENTS.md section 7.4); the shell opens each by
// its file name (`crates/ui/src/windows.rs`).
const windows = ['main', 'add-url', 'save-as', 'download', 'done', 'remove', 'about', 'options', 'scheduler', 'grabber', 'batch', 'links', 'properties', 'drop-target',
  'login', 'countdown', 'refresh', 'password', 'pick-links', 'pick-videos', 'gallery',
  'zip-preview', 'exception'];

export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, fs: { allow: ['../assets', '.'] } },
  build: {
    target: ['es2022', 'safari16'],
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: Object.fromEntries(windows.map((w) => [w, resolve(import.meta.dirname, `${w}.html`)])),
    },
  },
  resolve: mode === 'test' ? { conditions: ['browser'] } : undefined,
  test: {
    environment: 'jsdom',
    include: ['test/**/*.test.ts'],
    setupFiles: ['test/setup.ts'],
  },
}));
