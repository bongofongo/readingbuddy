# 2026-07-24 — Render modes + customizable book-view layout

Single-book viewer work: a render-mode swap point (glyph vs future rich), a
3-state → then user-customizable responsive layout, plus an edit-note Esc bug
fix. Frontend-only; engine untouched.

## Decisions locked

- **Two render modes.** `RenderMode { Glyph, Rich }` (`render3d/mod.rs`).
  `$TMUX` auto-detect picks glyph in tmux, rich outside; `--render
  auto|glyph|rich` overrides. Rich is a **stub that falls back to glyph** — safe
  to ship, real impl deferred.
- **Rich renderer = design-only this round.** Scaffold the seam, defer the
  pixels. Design captured in `docs/rich-renderer.md` (kitty graphics + existing
  raytracer at high ss, gated non-tmux).
- **Layout became fully user-driven.** Started as fixed `{Stacked, Split,
  Compact}`; user then asked for customizability, so it's now
  `Split(PaneOrientation)` + `Compact`, with two live controls:
  - **Rotate panes — `t`** (clockwise). Four positions Book{Top,Right,Bottom,Left};
    start = aspect default (portrait→BookTop, landscape→BookLeft), `t` advances a
    quarter-turn, wraps at 4. Deliberately modeled as rotating the *single
    divider* so a future 3rd pane rotates with the same key.
  - **Slide divider — `[` / `]`** (`]` grows book's share). Stored as
    `divider_bias`, an **offset from the default**, not an absolute — so it never
    jumps and needs no knowledge of the frame size at keypress time.
- **Portrait split ratio**: user changed 1/3-2/3 → **1/2-1/2**
  (`stacked_object_height = height/2`), still capped by `BOOK_MAX_ROWS`.
- **Book max size +20%**: `BOOK_MAX_COLS 42→50`, `BOOK_MAX_ROWS 22→26`. These are
  the *default* divider position now, not a hard wall — `divider_bias` slides past.
- **Key bar trimmed**: dropped `↑↓ move` and `↵ open` (self-evident); added
  `t rotate`, `[ ] panes`.
- **Edit-note Esc**: for an existing (`NoteTarget::Edit`) note, Esc just closes
  the editor ("edit cancelled") — no discard prompt, no deletion. The
  discard-confirm is now new-notes-only; deletion stays exclusively on `d`.

## Technical gotchas

- **Disjoint field borrows for the presenter**: `presenter_for(mode, &mut
  app.scene)` returns `Box<dyn BookPresenter + '_>` borrowing `app.scene`, while
  the book comes from `&app.view`. Compiles only because both are *direct* field
  paths of the same `app` — don't route either through a `&mut App` method or the
  split breaks. (`ui/book.rs::present_book`.)
- **Trait-object method call needs no `use`**: calling `.draw_book()` on a `Box<dyn
  BookPresenter>` does NOT require the trait in scope — importing it warns unused.
  (Removed the `use crate::render3d::BookPresenter` and the re-export.)
- **`divider_bias` as an offset, not a fraction**: `App::handle` has no frame
  `Rect`, so an absolute target fraction couldn't be seeded without a jump.
  Storing an offset added to the size-derived default (`biased_span` clamps to
  `[MIN_PANE, axis-MIN_PANE]` and `[0.18,0.82]` frac) sidesteps needing the area.
- **`split_rects` returns the panel's border side**: the divider rule sits on the
  panel edge facing the book — `LEFT/RIGHT/TOP/BOTTOM` per orientation.
  `draw_panel` insets horizontally only for a vertical rule
  (`border.intersects(LEFT|RIGHT)`).
- **Portrait test uses physical cell aspect**: cells are ~1w:2h, so "portrait" is
  `height*2 > width`, not `height > width`.
- **clippy `enum_variant_names`** fires on the shared `Book` prefix
  (BookTop/…); the prefix is the point, so `#[allow(clippy::enum_variant_names)]`
  on `PaneOrientation`.
- **Repo-wide `cargo fmt --check` is dirty on this machine** — untouched files
  (engine, cli, theme.rs) show diffs, so this toolchain's rustfmt disagrees with
  the repo's pinned formatter. My new code is NOT in the diff set; left the
  pre-existing churn alone (do not blanket-`cargo fmt`).

## Verification

- `cargo test --workspace` + `cargo clippy --workspace --all-targets` (via
  cargo-tester) — green. TUI suite 80 tests.
- New tests: layout orientation/rotation/`split_rects`/`biased_span` clamps
  (`ui/mod.rs`); `rich_mode_draws_the_book_view_without_panicking`,
  `rotate_and_resize_adjust_layout_and_redraw`,
  `esc_on_an_existing_note_cancels_without_prompting` (`app.rs`).
- `every_screen_draws_at_every_size` (120x40→1x1) still green — layout panic
  would wreck a tmux pane.

## Deferred

- **Rich renderer** — kitty graphics path unbuilt (see `docs/rich-renderer.md`).
  This is the next thread's focus.
- **Layout prefs not persisted** — `LayoutPrefs { rotation, divider_bias }` is
  per-session only; could ride in `tui.toml` next to `accent`.
- **3rd+ pane** — rotation is built to generalize, but there's only book+panel today.
