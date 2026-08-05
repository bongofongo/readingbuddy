import { sveltekit } from '@sveltejs/kit/vite';
// `vitest/config`, not `vite` — plain vite's `defineConfig` has no `test` key and
// rejects it, which svelte-check reports as a type error rather than vitest
// ignoring it.
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [sveltekit()],
  // Fixed port, and `strictPort` so a stale dev server on 5173 is an error
  // rather than a silent move to 5174 that `tauri.conf.json`'s `devUrl` and
  // Playwright's `baseURL` both then point at the wrong thing.
  server: { port: 5173, strictPort: true },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
