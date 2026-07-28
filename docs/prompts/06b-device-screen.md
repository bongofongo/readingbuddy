# Prompt — Item 6b: the device screen (TUI half)

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/tui-device-screen`). **Run this locally** — it needs a real terminal.

---

Read `docs/spec-engine-04-07.md` (item 6b) and `docs/decisions.md` before
starting. The **Architecture** section of `CLAUDE.md` describes this crate's
rules in detail and they are binding — especially everything about
`animating()`, the renderer and the size sweep.

**Depends on items 4 and 6a, both merged.** Rebase on `main` first rather than
resolving `app.rs` by hand. No migration.

## What you are building

The first place a stranger's data shows up in the library. It should read as the
device's shelf, not a file picker — a *place*, per the axiom.

Per book on the mounted device, one of four states, which
`Engine::scan_device` (item 6a) already computes: **New** (on the device, not in
the library), **Unchanged**, **Updated** (*N* new highlights, *M* refreshed),
**Unreadable**. Single-book pull first, then multi-select and sync-all.

Everything engine-side exists. This thread is presentation plus the event-loop
care point below.

## The file-by-file shape

1. `app.rs:36` — `Screen::Device`.
2. `app.rs:45-88` — `MenuItem::Device` plus a `MENU` row. **The array-length
   literal `8` at `:56` must be bumped**, and `activate_menu` (`:675-710`) needs
   its arm.
3. `App` fields: `device: Vec<DeviceBook>`, `device_state: ListState`,
   `device_marks: HashSet<usize>`, `device_root: Option<PathBuf>`; initialised
   in `App::new` (`:282-320`).
4. `step_device`, cloned from `step_library` (`app.rs:1387-1395`) — wraps with
   `rem_euclid`, no-ops on an empty vec.
5. New `crates/tui/src/ui/device.rs`, modelled on `library.rs:11-45`: build
   `Vec<Line>` rows, measure the widest, `super::list_box(area, widest, rows)`,
   `Clear`, then `List` with `highlight_symbol("› ")` and **no
   `highlight_style`** — the reverse goes on the title span only, which
   `search.rs:105-125` asserts and `print_lists` visualises.
6. `ui/mod.rs:3-10` (the `pub mod`) and `ui/mod.rs:373-379` (the draw arm).
7. `app.rs:2002-2008` (`draw_every_screen_once`) and `:2667`
   (`print_lists`' screen array).

## The one architectural care point — read this before writing the handler

**Every engine call in the TUI is awaited inline in the key handler**
(`app.rs:1421-1461`). There is no task channel and no `tokio::spawn` anywhere in
`crates/tui/src`. A scan, or a sync of forty books, would freeze the draw loop
and the 20 fps ticker solid.

Copy the one existing mitigation — the **deferred-work field**. `pending_verify`
is set by the key handler; the loop draws the "verifying…" frame; the work is
drained *after* the redraw (`app.rs:1454-1458`, `finish_verify` at
`:1155-1183`).

So:

- `pending_scan: Option<PathBuf>` — drained once, with a "scanning…" frame
  drawn first.
- `pending_pull: Option<VecDeque<PathBuf>>` — drained **one book per loop
  iteration, with a redraw between each**. That is what gives a sync-all real
  per-book progress and keeps `select!` reachable. A `for` loop inside the
  handler gives neither.

Diagnostics: the TUI has only `status: Option<String>` and no diagnostic buffer
— `run_search` (`:1300-1322`) and `import_ko` (`:1367-1384`) both flatten to one
line. This screen is the first surface that needs per-row `Diagnostic`s, so
store them on the row. `Diagnostic` is `Clone` deliberately
(`diagnostic.rs:31-36`).

## Keys

`Space` is `ToggleSpin` and `a` cycles the ambient motif, so:

| key | action |
|---|---|
| `Enter` | pull the selected row |
| `x` | mark / unmark |
| `s` | sync (marked rows, else every `New`/`Updated`) |
| `l` | link to an existing book — offer `DeviceState::New`'s candidates |
| `r` | rescan |

New `Action` variants go in `event.rs:7-49` with their `map_key` arms at
`:62-93`. The key bar stays visible; the screen must not be a dead end, and `m`
must still return to the menu.

`l` should reuse the candidate band the engine already returns — `link_sidecar`
records the choice as `Manual` via `set_device_link`, which *repoints*
deliberately, unlike the scan's `link_device_book`. Do not swap them.

## Two things not to break

- **Do not widen `animating()`** (`app.rs:477`). It means "the book is turning",
  it feeds `params.moving`, and widening it makes a parked book report moving
  forever and never transmit its crisp pixel frame — with nothing on screen
  looking wrong. The ambient layer has its own `ambient_visible()` /
  `ambient_animating()` for exactly this reason.
- `ambient_visible()` (`:493`) is `motif != Off && screen != Book`, so the new
  screen inherits the ambient background for free. That means its content box
  **must `Clear` first** — a `Block` styles the cells it does not draw but never
  blanks them.

## Tests

- `every_screen_draws_at_every_size` (`app.rs:1973-1995`) must pass with the new
  screen at every size down to 1×1. Guard with
  `if inner.width == 0 || inner.height == 0 { return; }` before rendering into a
  block's inner rect (see `book.rs:81-83`). **A layout panic wrecks the user's
  tmux pane** — this is the gate, not a formality.
- `test_app()` (`app.rs:1546`) seeds device rows from a fixture tree.
- A sync-all drains **one book per iteration**: assert the queue shrinks across
  ticks, not that it completes in one.
- `print_lists` (`app.rs:2637-2695`) shows the new screen's rows and its
  REVERSED mask — that mask exists because a whole-line highlight regression is
  invisible in a plain symbol dump.

## Constraints

- TUI only. If the engine is missing something, that is a bug in item 6a — say
  so rather than adding engine logic here.
- No `$EDITOR` / suspend path, ever. A broken vim killed the TUI once.
- Text uses `Color::Reset` / `DarkGray` and `REVERSED` selection so it reads on
  light and dark terminals; only accents are hard-coded RGB.
- If you add a persisted setting (a remembered mount path), it needs a
  `TuiConfig` field **and** a line in `config_snapshot` (`app.rs:1047-1052`) —
  `config::save` overwrites the whole file, and
  `persisting_carries_every_setting_not_just_the_changed_one` (`:2633`) guards
  it.

## Done when

`make ci` green; the size sweep passes; from the menu you can open Device, pull
one book, mark several and sync them with visible per-book progress, and link an
unmatched row to an existing book without leaving the screen. Run the
`cargo-tester` agent before committing.
