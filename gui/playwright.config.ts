import { defineConfig, devices } from '@playwright/test';

/**
 * Layer 2 — routes and pixels, against the Vite dev server, no Tauri.
 *
 * `docs/gui/testing.md`'s argument, and the reason this is not `tauri-driver`:
 * Apple ships no WebDriver for WKWebView, so `tauri-driver` does not run on this
 * machine at all — and even where it runs it drives a *built binary*, paying a
 * Rust link and a real window per iteration. A webview is a browser, and the
 * Svelte that renders in WKWebView renders here.
 *
 * **WebKit, not Chromium.** WKWebView is what the app actually ships inside on
 * macOS, so WebKit is the engine whose bugs are ours. Chromium would be a faster
 * download and the wrong browser.
 *
 * Three viewports, because a layout bug is a size bug — the direct descendant of
 * the TUI's `every_screen_draws_at_every_size`, which renders from 120x40 down to
 * 1x1 for exactly this reason.
 */
export default defineConfig({
  testDir: 'tests',
  // Screenshots live beside the specs and are read by the `screenshot-reviewer`
  // agent, which is the only check in this repo that can see.
  snapshotPathTemplate: 'tests/shots/{arg}-{projectName}{ext}',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? 'list' : [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://localhost:5173',
    // A trace on the first retry is what turns "it failed in CI" into something
    // readable without reproducing it.
    trace: 'on-first-retry',
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Safari'], viewport: { width: 1180, height: 820 } } },
    { name: 'narrow', use: { ...devices['Desktop Safari'], viewport: { width: 720, height: 900 } } },
    { name: 'phone', use: { ...devices['iPhone 14'] } },
  ],
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
