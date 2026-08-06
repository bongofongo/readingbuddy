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
  //
  // `fs.allow` names one file outside this root: `crates/corpus/edge-cases.json`,
  // the declaration item 38 checks both fixtures against. It is scoped to that
  // directory rather than opened to the repo, because the reason to reach out of
  // `gui/` is exactly one file and a wider grant would stop saying so.
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: ['..', '../crates/corpus'] },
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
