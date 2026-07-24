# 2026-07-24 — TUI polish pass

Cosmetic/interaction polish so the TUI framework can be parked while focus moves
to the engine. Frontend-only; engine untouched.

## Decisions locked

- **Floating header** over a reserved top pane — title·author + progress text
  hover over the top 2 rows of the full-height book (user picked plain text, not
  a filled gauge; anchored top).
- **Spin** quicker: `SPIN_SPEED` 0.005 → 0.009 (~63s → ~35s per rev). User chose
  ~35s from {35/40/50}.
- **Accent color** configurable: 7-preset palette + free `#RRGGBB` entry,
  persisted. Presets cycle with ←/→, `/` opens the hex box.
- **List selection highlight** — first homogenized rows with `·` separators, then
  user reversed course: keep per-field **colors** on library rows but scope the
  REVERSED selection to the **title span only** (match the main menu). Book-view
  note/highlight/card lists were left on full-row reverse (not requested).

## Technical gotchas

- **Cursor parking to kill kitty trail**: ratatui leaves the hidden cursor on the
  last-written diff cell, which moves every animation frame → kitty `cursor_trail`
  streaks after the book. Fix: after every `terminal.draw`, pin the cursor to the
  bottom-right cell. Must use `Backend::set_cursor_position` + `Backend::flush`
  (trait methods) — NOT `crossterm::execute!`/`ExecutableCommand`, which needs a
  `Write` bound the `Backend` trait (and `TestBackend`) doesn't provide. No
  standard escape reliably toggles `cursor_trail` at runtime, so parking is the
  terminal-agnostic fix.
- **List widget can't scope highlight to one span**: `List::highlight_style`
  patches the *entire* rendered row. To highlight only the title (menu style),
  drop `highlight_style` entirely, pass the selected index into the row builder,
  and apply `theme::title().patch(theme::selected())` to just the title span.
  Keep `highlight_symbol` + `render_stateful_widget` for the `› ` marker/scroll.
- **Runtime accent without threading a color everywhere**: `theme::ACCENT` is now
  a process-global `AtomicU32` (packed `0xRRGGBB`); the zero-arg `accent()`/`key()`
  helpers read it live, so no draw signatures changed.
- **Config file separation**: TUI accent lives in its OWN
  `~/.config/readingbuddy/tui.toml` (`crates/tui/src/config.rs`), NOT the CLI's
  `config.toml`. Reason: different structs per crate + serde full-overwrite `save`
  → each crate's save would drop the other's fields (CLI holds the Google key).
- Floating header overlay works because ratatui buffer writes are last-wins and
  text spans carry `Color::Reset` bg — render object first, then the header on top
  of the same cells.

## Verification

- `cargo test --workspace` + `cargo clippy --workspace --all-targets` (via
  cargo-tester) — green.
- New unit tests: `theme::parse_hex` round-trip/reject, `set_accent` reflected by
  getters. `every_screen_draws_at_every_size` (120x40→1x1) still green.
- Eyeballed `--ignored print_layout`: header floats, book fills full height.
- No app-level accent-cycle test on purpose: `cycle_accent`/`persist_accent`
  write to real `~/.config`, which a test would pollute — theme tests cover the
  color logic instead.

## Deferred

- Book-view note/highlight/card lists still use full-row REVERSED (only library
  was requested for the title-scoped highlight).
- Persisting other TUI prefs (glyph set, spin) in `tui.toml` — schema is there,
  only `accent` is wired.
- Pixel companion still unbuilt.
