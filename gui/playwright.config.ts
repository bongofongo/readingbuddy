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
  /*
   * The three widths are chosen to be **one per layout**, not one per device.
   *
   * The book page is three columns and folds twice: at ≤1180 the right rail —
   * the inspector — unsticks and goes under the centre, and at ≤860 the left
   * rail stops being a column too. So a suite that only ever rendered 1180 and
   * 720 would have screenshotted the *folded* layout twice and the three-column
   * desk the whole redesign is about **never**. That is what these were before,
   * and it is the failure this file exists to avoid one layer down.
   *
   * 1440 is the width the layout is argued at; 1100 is the one fold; the phone
   * is the other. A layout bug is a size bug, and each of these is a different
   * size in the sense that matters.
   */
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Safari'], viewport: { width: 1440, height: 900 } } },
    { name: 'narrow', use: { ...devices['Desktop Safari'], viewport: { width: 1100, height: 900 } } },
    { name: 'phone', use: { ...devices['iPhone 14'] } },
  ],
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
