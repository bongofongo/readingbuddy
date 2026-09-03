---
name: run-readingbuddy
description: Build, launch, drive and screenshot readingbuddy's four binaries — the Tauri GUI, the ratatui TUI, the CLI and the daemon — against a seeded library. Use when asked to run, start, launch, open, screenshot or poke the app, or to confirm a change works in the real app rather than in tests.
---

# Running readingbuddy

Four binaries, one library. **Everything goes through the driver** —
`.claude/skills/run-readingbuddy/driver.sh` — because three of the four
cannot be checked by reading them: the TUI needs a tty, the GUI needs a
Wayland surface plus a webview, and the daemon needs a socket peer.

All paths below are relative to the repo root. The driver takes the library
from `RB_DATA` (default `dev-data/`) and drops artifacts in `RB_OUT`
(default `/tmp/rb-run/`).

**This is not the check.** `make ci` / the `cargo-tester` agent is the gate.
The driver is for looking at the running app.

## Prerequisites

The library the driver runs against:

```bash
ls dev-data/database/app.db          # if absent: make dev-db
```

For the GUI only, the frontend deps. `pnpm` was **not** installed on this
machine — only a leftover `~/.local/share/pnpm/store` — and `package.json`
pins the version:

```bash
npm i -g pnpm@10.14.0                # lands in ~/.npm-global/bin
cd gui && ~/.npm-global/bin/pnpm install --frozen-lockfile
```

That is the only thing pnpm is needed for. The driver launches the dev
server through `gui/node_modules/.bin/vite` directly, so a non-interactive
shell without `~/.npm-global/bin` on `PATH` still works.

Wayland screenshots need `grim` (present). Driving the TUI needs `tmux`
(present).

## Run (agent path)

```bash
D=.claude/skills/run-readingbuddy/driver.sh

$D build all                         # or: gui | tui | cli | api
$D cli list --limit 3                # CLI against $RB_DATA
$D api list_books '{"limit":1}'      # starts the daemon if needed, one JSON line
$D tui j Enter                       # TUI in a private tmux; pane -> stdout
$D tui-frame 80x24                   # the 3D book, no tty: ANSI + PNG in $RB_OUT
$D gui                               # launch the Tauri GUI, screenshot it
$D shot /tmp/rb-run/after.png        # screenshot the running GUI window
$D stop                              # stop everything the driver started
```

Subcommands build their binary on demand, so `build` is optional.

**`gui`** starts vite (or reuses one already on :5173), launches the binary
with the four environment settings that make WebKitGTK survive on this
machine, waits for the window to map, and screenshots it to
`/tmp/rb-run/gui.png`. **Look at the PNG.** A blank frame means the webview
loaded nothing — check `/tmp/rb-run/gui.log` and `/tmp/rb-run/vite.log`.

**`tui`** takes tmux key names, one per argument, with a pause between each
(`j`, `Enter`, `C-d`, `q`). It runs on its own tmux server (`-L rbrun
-f /dev/null`) at a fixed 120x40, so it never touches your panes and two
runs are comparable. It quits the app and kills that server on the way out.

**`api`** speaks the daemon's wire protocol: one JSON object per line over
`<data-dir>/readingbuddyd.sock`, each reply carrying the id it answers. This
is the surface the GUI is built on, so it is the right place to check
whether the API can serve a screen before building one.

## Run (human path)

```bash
cd gui && READINGBUDDY_DATA_DIR=$PWD/../dev-data ~/.npm-global/bin/pnpm tauri dev
```

This is what the root `CLAUDE.md` documents, and on this machine it **fails
at the last step**: the binary builds and runs, then dies instantly with
`Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display`,
because `tauri dev` does not set the WebKit environment below. Use the
driver, or export those variables first.

The CLI needs nothing special, and the TUI needs a real terminal:

```bash
cargo run -p readingbuddy-cli -- --data-dir $PWD/dev-data list --limit 5
cargo run -p readingbuddy-tui -- --data-dir $PWD/dev-data --dump-frame 40x12
```

Run the TUI itself (no `--dump-frame`) from an actual pane. Started without a
tty it exits immediately with `Error: No such device or address (os error 6)`
— which is why `driver.sh tui` goes through tmux.

## Gotchas

- **The GUI dies on launch without four environment settings.** NVIDIA +
  WebKitGTK + Hyprland gives `Gdk-Message: Error 71 (Protocol error)`
  before a window ever maps. What works:
  `unset DISPLAY`, `WEBKIT_DISABLE_DMABUF_RENDERER=1`,
  `WEBKIT_DISABLE_COMPOSITING_MODE=1`, `GDK_BACKEND=wayland`. Falling back
  to X11 is **not** an option here — `DISPLAY=:0` is set but stale, and
  `xdpyinfo -display :0` cannot reach it, so there is no Xwayland to land on.
- **Nothing can click the GUI — on Linux.** No `wtype`, no `ydotool`, and this
  is Hyprland 0.56.2 whose `hyprctl dispatch` takes Lua
  (`hl.dsp.window.close()`), with no `sendshortcut` dispatcher in that
  namespace. There the GUI can be launched, screenshotted and read; it cannot
  be driven. To exercise behaviour rather than appearance, drive `api` (the same
  surface the GUI calls) or the frontend's vitest suite. **Do not probe the Lua
  dispatcher interactively** — `hl.dsp.window.close()` is live and acts on the
  focused window.

  **On macOS it can be driven, and the whole driver above does not apply.**
  There is no Wayland, no `grim` and no WebKitGTK; Tauri renders through
  WKWebView and none of the four environment settings are needed. What works,
  and what this file was missing:

  ```bash
  # geometry, from the accessibility API — not a guess and not a window picker
  osascript -e 'tell application "System Events" to tell process "readingbuddy-gui" \
    to get {position, size} of window 1'          # -> 6, 38, 1500, 940
  screencapture -x -o -R"6,38,1500,940" shot.png  # exactly that window
  osascript -e 'tell application "System Events" to click at {588, 159}'
  osascript -e 'tell application "System Events" to key code 121'   # page down
  ```

  This is **deterministic where `grim` is a coin flip**: the region comes from
  the window itself, so it cannot capture whatever happened to be on top. Set
  the size first (`set size of window 1 to {1500, 940}`) and two runs are
  comparable.

  Three things to know before relying on it:

  - **Screen coordinates from a screenshot** are
    `screen = displayed × 0.75 + origin` for a 1500-wide window captured at 2×
    and shown at 2000px. Get the scale wrong and clicks land on the wrong
    control with no error.
  - **`:focus-visible` fires on a synthetic click** in WebKit, so a `.choice`
    renders with a focus ring after a driven click. That is an artifact of
    driving, not a defect — do not "fix" it.
  - **macOS may prompt for screen recording** ("tmux is requesting to bypass
    the system private window picker"). Screenshots work **without** granting
    it; dismiss it. Granting a persistent bypass is the user's decision, not an
    agent's.

  Clicking reaches anything with a control on screen. For a route with **no
  link into it from where you are**, the fallback is a temporary redirect in
  `gui/src/routes/+layout.svelte`, which vite's HMR applies without a restart:

  ```svelte
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  $effect(() => {
    if (page.url.pathname !== '/reading') goto('/reading?book=3');
  });
  ```

  **Revert it before committing** — `git checkout gui/src/routes/+layout.svelte`.
  It compares `pathname`, so a target carrying only a query string (`?note=1` on
  the page you are already on) loops for ever; seed the page's own state instead.
  A tab or a panel is component state and has no URL at all, so neither trick
  reaches one — click it.
- **Port 5173 is not configurable.** It is `devUrl` in
  `gui/src-tauri/tauri.conf.json`, so `pnpm tauri dev` hard-fails with
  `Port 5173 is already in use` when a stale vite is around — and a vite
  orphaned from an earlier session can sit there for days. The driver reuses
  whatever is listening instead of failing; `ss -ltnp | grep 5173` names the
  pid if you want it gone.
- **`RB_DATA` must be absolute.** `cover_path` is stored as
  `images_dir.join(name)`, so a relative root yields a relative path and the
  webview has no working directory to resolve it against — covers come up
  blank. The driver absolutises it.
- **`nc -U` hangs on the daemon.** It does not close the connection at stdin
  EOF, so it waits on a reply that already arrived; it only appeared to work
  in the docs because `| head -c 600` SIGPIPEd it. The driver uses a python
  socket that reads one line and closes.
- **`$(printf ...)` strips the frame delimiter.** The daemon reads one JSON
  object *per line*; command substitution eats the trailing newline, and the
  daemon then holds the connection waiting for the rest of the line. The
  driver re-adds it at the socket.
- **A background service must be `setsid -f`'d or it eats the driver's output.**
  `driver.sh gui | tail -4` appeared to hang for ever — no output, no exit —
  while the window was up and the screenshot already on disk. vite had
  inherited the caller's stdout pipe, `tail` prints only at EOF, and bash sat
  in `do_wait` on a child that never exits. `spawn()` forks the service out to
  init; do not "simplify" it back to `cmd &`. Plain `setsid` (no `-f`) does
  **not** fix it — it changes session, not parentage.
- **Never `pkill -f` a pattern that appears in your own command line.**
  `pkill -f "vite/bin/vite.js"` matches the shell running it and kills the
  shell — twice here, exit 144, no other output. Use the bracket trick:
  `pgrep -f 'vite/bin/[v]ite.js'`, `pgrep -f 'debug/readingbuddy-[g]ui'`.
- **Two GUI instances look identical to `shot`.** Both map a window of class
  `readingbuddy-gui` against the same SQLite file, and the screenshot picks
  the first. `gui` reuses an already-mapped window rather than starting a
  second.
- **The GUI's data dir is `READINGBUDDY_DATA_DIR`, not `--data-dir`.** The
  other three take the flag.

## Troubleshooting

| symptom | fix |
|---|---|
| `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display` | the four env settings above; the driver sets them |
| `Port 5173 is already in use` | a stale vite — `ss -ltnp \| grep 5173`, kill the pid, or let the driver reuse it |
| `pnpm: command not found` | `npm i -g pnpm@10.14.0`; to *run*, use `gui/node_modules/.bin/vite` |
| `driver.sh api` hangs | you are using `nc -U` somewhere, or lost the trailing newline |
| `no library at … — run: make dev-db` | the seeded library is missing or `RB_DATA` points elsewhere |
| `no GUI window appeared` | read `/tmp/rb-run/gui.log`; the process usually died in the first second |
| GUI screenshot is blank | webview loaded nothing — `/tmp/rb-run/vite.log` |
| `driver.sh gui \| tail` prints nothing and never returns | a service is holding the pipe — it must go through `spawn()` |
| `driver.sh stop` finds nothing, but things are still running | `$RB_OUT` was wiped, so the pid files are gone: `kill $(pgrep -f 'debug/readingbuddy-[g]ui')` |

## Not verified this session

`make dev-db` — `dev-data/` already existed and rebuilding it would have
discarded the working library. Everything else in this file was run here.

## Which machine this file is about

Everything above except the macOS block was written on, and verified on, the
Linux box (Hyprland + WebKitGTK + NVIDIA). The macOS block was written and
verified on the dev laptop on 2026-09-02 and **the `driver.sh` script does not
run there at all** — it reaches for `grim`, `hyprctl` and the four WebKitGTK
environment settings, none of which exist. On macOS, drive the GUI with the
`osascript` + `screencapture` recipe directly; a driver for it is unwritten.
