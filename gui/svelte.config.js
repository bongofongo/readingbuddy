import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
export default {
  preprocess: vitePreprocess(),
  kit: {
    // SPA, one fallback document. There is no server: the app is a webview over
    // an in-process engine, so SSR would be prerendering against a library that
    // does not exist at build time. `fallback` rather than `prerender` because a
    // book route is `/book/[id]` over a library nobody has built yet.
    adapter: adapter({ fallback: 'index.html' }),
    prerender: { entries: [] },
  },
};
