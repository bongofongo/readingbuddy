---
title: GUI testing — four layers, and where tauri-driver actually belongs
date: 2026-08-04
source: Tauri v2 docs (WebDriver, Mocking) and the WebdriverIO Tauri service,
        checked 2026-08-04; see the links at the bottom
---

# GUI testing

## The fact that decides this

> "Driven directly, only Windows and Linux are supported on desktop, as macOS
> has no WKWebView driver tool available"
> — Tauri v2 docs, *WebDriver*

**`tauri-driver` does not run on your machine.** Apple ships no WebDriver for
WKWebView, so the tool that wraps `msedgedriver` (Windows) and
`WebKitWebDriver` (Linux) has nothing to wrap on macOS.

There is now a macOS path — `tauri-plugin-wdio-webdriver`, an *embedded*
WebDriver server compiled into the debug build, auto-detected by the WebdriverIO
Tauri service, no external driver and no subscription. It works. But it is worth
being clear that it is newer than the rest of this stack, and that it means a
WebDriver server inside your binary, which wants `#[cfg(debug_assertions)]`
around its registration and a look at what capabilities it grants.

Either way, the more important point is the second one:

**Even where `tauri-driver` runs perfectly, it is the wrong layer for a fast
loop.** It drives a *built application binary*. Every iteration pays a Rust
link, a window, a real webview and real IPC. That is a minute-scale loop being
asked to do a second-scale job, and no amount of configuration fixes the
category error.

## The shape that works, and you have already built it once

This is the same argument `CLAUDE.md` already makes about the TUI:

> The TUI suite is **fully headless** and needs no terminal: it renders through
> ratatui's `TestBackend`. Only `make bench`, `make bench-box` and `--probe`
> need a real, active pane.

The fast tests need no real terminal; the three that do are quarantined behind
make targets. Do exactly that again. A webview is a browser, and the Svelte that
renders in WKWebView renders in Playwright's WebKit too.

| | what | needs | speed | when |
|---|---|---|---|---|
| **L1** | component tests — Vitest + `@testing-library/svelte`, fake client injected | nothing | **milliseconds** | on save, `--watch` |
| **L2** | route + visual — Playwright against the **Vite dev server**, no Tauri | a browser | **seconds** | on save / pre-commit |
| **L3** | the Rust side — `cargo test -p readingbuddy-api` | nothing new | seconds | already exists |
| **L4** | E2E smoke — the real binary, real IPC, real SQLite | a driver | **minutes** | `make e2e`, pre-PR and CI |

L1 and L2 are where test-driven development happens. L4 is a seam check, not a
feature suite — see below.

## What makes L1 fast: the client seam, not `mockIPC`

Tauri ships real mocking utilities — `mockIPC`, `mockWindows`, `clearMocks`,
and event mocking since v2.7.0 — and they work:

```js
mockIPC((cmd, args) => {
  if (cmd === "add") return args.a + args.b;
});
```

Note what you are matching on: **a string**. That is the untyped drift the
generated-types recommendation exists to kill. A renamed command breaks the app
and every test that mocks it keeps passing.

So make `mockIPC` the *narrow* tool and the client trait the *broad* one. Item
25 already calls for the GUI to reach the backend through one typed client
rather than scattering `invoke` calls. Once that exists, a test injects a fake
client implementing the same generated interface, and:

- no Tauri is loaded at all, so L1 runs in plain `jsdom` at full speed;
- a drifted field or a renamed command is a **`tsc` error in the fake**, because
  the real and fake clients share a generated type;
- the fake is ordinary data — a seeded in-memory library — so a test can say
  "given a book with 400 pages and two readings" in one line.

Keep `mockIPC` for testing **the client itself**: does it invoke the right
command with the right arguments. That is exactly one small test file, and it is
the only place a command-name string should appear in a test.

## What L2 buys, and its one honest caveat

Playwright against the dev server catches most of what an E2E run would, because
your Svelte does not know it is in a webview. It is also where the screenshot
goldens live — `make shots`, committed, diffed on review, the direct descendant
of `make golden` and of `every_screen_draws_at_every_size`.

**The caveat: Chromium is not WKWebView.** A CSS or JS difference that only
shows in Safari's engine will pass L2 and fail in the real app. The fix is one
line of config — Playwright ships WebKit:

```
projects: [
  { name: 'webkit',   use: devices['Desktop Safari'] },   // closest to macOS
  { name: 'chromium', use: devices['Desktop Chrome'] },   // closest to WebView2
]
```

Run WebKit locally (it is what your users get), Chromium in the container.
**Chromium is preinstalled in cloud sessions** at
`PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`, so a cloud thread can run L2 with
no download — WebKit would need fetching, which the proxy may or may not allow;
find out once and write the answer down, the way the gutenberg.org note is
written down.

## Keeping L4 genuinely light

The discipline is what makes this cheap, not the tooling.

- **One file. Under ten tests. A stated time budget.** If it grows past that,
  the new test belongs in L1 or L2 and someone has misplaced it.
- **It tests the seam, not the features.** Does the app boot; does the window
  appear; does one real `invoke` round-trip to a real SQLite file and come back
  with the right book; does a cover load off `images_dir`; does a write persist
  across a restart. Every one of those is a thing L1 and L2 *mock away* and
  therefore cannot check. Nothing about the shelf's layout belongs here.
- **Against a copy of a seeded library**, never yours. `scripts/bench-sandbox.sh`
  already sets this precedent — it runs against a *copy* of `database/` so it
  touches neither your panes nor your library. Point `READINGBUDDY_DATA_DIR` at
  a temp copy of `make dev-db`'s output and E2E inherits the same guarantee.
- **`make e2e`, never the inner loop.** On CI it starts as its own job that does
  not gate the PR — the same call `scheduled.yml` already makes for the tier-2
  corpus. Promote it to a gate once it has been green long enough to be trusted.
- **Linux in CI is the cheap path.** `tauri-driver` + `WebKitWebDriver` + Xvfb
  on the ubuntu runner needs no plugin compiled into the binary and no
  third-party crate. If macOS E2E turns out to matter, add
  `tauri-plugin-wdio-webdriver` then — but the Linux job is the one that catches
  regressions, and it is the one that costs nothing to keep.

## The loop this produces

```
pnpm vitest --watch          ms       ← where TDD happens
pnpm playwright test --ui    seconds  ← where the shelf gets looked at
make ci                      minutes  ← cargo + web checks, the existing gate
make e2e                     minutes  ← before a PR; CI runs it on Linux
```

## The one thing to decide before writing any of it

Whether the fake client is **hand-written** or **generated from the same schema
as the real one**. Hand-written is faster on day one and is how fakes silently
stop resembling the thing they fake. Generated costs an afternoon and means a
DTO change breaks the fake at compile time — which, for a codebase where agents
write most of the tests, is the difference between a test suite that catches
drift and one that certifies it.

Given items 2 and 25 already commit to generated types, generate the fake too.

---

## Sources

- [WebDriver — Tauri v2](https://v2.tauri.app/develop/tests/webdriver/) — the Windows/Linux-only statement
- [Mock Tauri APIs — Tauri v2](https://v2.tauri.app/develop/tests/mocking/) — `mockIPC`, `mockWindows`, `clearMocks`, event mocking
- [Tests — Tauri v2](https://v2.tauri.app/develop/tests/)
- [Platform Support — WebdriverIO Tauri service](https://webdriver.io/docs/desktop-testing/tauri/platform-support/) — the macOS embedded-provider path
- [tauri-webdriver](https://github.com/danielraffel/tauri-webdriver) — third-party macOS driver; early, and itself points at [tauri-plugin-webdriver](https://github.com/Choochmeque/tauri-plugin-webdriver) as more mature
