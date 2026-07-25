# 2026-07-25 — centring the object, tab toggles the pane, and a resize bug in the pixel path

Continuation of the same day's layout work. Yesterday's block-centring left the
*book* parked left of centre on a wide window; this session centres the object
itself, changes what `t`/tab do, and — chasing a report of a torn book — finds a
pre-existing bug in the kitty transmit cache. Frontend-only; engine untouched.

## Decisions locked

- **The object is centred on the window, not the block.** Supersedes
  `2026-07-25-ambient-layer-centred-layouts.md`'s "centred as a *block*". The
  user's words: "the pane separation shouldn't be centered in the window but the
  actual center of the book 3d object should be centered." `book_rects` composes
  the old pieces — `content_block` + `split_rects` *size* the object, then it is
  repositioned; both keep their contracts and every existing test stayed valid.
- **Continuous degradation, no reflow cliff.** The slide toward centre stops as
  soon as it would take the panel under `PANEL_MIN`, so cramped panes return to
  the old tiling smoothly. Rejected the design pass's alternative (a hard
  86-column gate), which jumped the panel 36 → 18 columns at the threshold.
- **Landscape default is panel-on-the-left** (`BookRight`) — the user's call
  after seeing all four.
- **`t` still rotates; tab shows/hides the section pane.** First cut removed
  rotation entirely; user corrected mid-session. `Action::NextTab` is gone (tab
  was its only binding), `PrevTab`/BackTab still steps the menu back.
  `ensure_panel` restores a hidden pane for ↑↓/Enter/→, which would otherwise be
  dead keys.
- **The divider rule is painted only when a section is open** — against the bare
  tab menu it is a line through empty space. Its cell is still reserved, so
  opening a section doesn't shift the panel sideways.
- **Header centred over the object; the tab menu centred as a *block*** in its
  pane (`menu_box`), not per-line — per-line centring would shuffle the `› `
  caret column every time the selection moved. **Section content stays
  left-aligned** (Info facts, notes/highlights/cards) — explicit user call.
- **Stacked layouts keep half-and-half spacing.** A centred band was built and
  then reverted at the user's request mid-session: "switch back the half half
  pane spacing that existed before when tab menu is toggled on." The book still
  reaches the window's middle there by dismissing the pane with tab.
- **No perf/bench work**, waived explicitly by the user for this change.

## Bugs found

- **Parked pixel book torn, half-drawn or missing after a resize — stacked
  layout only** (pre-existing, dates to the rich path). `RichKey` is keyed on the
  *object rect*, and the stacked layout caps **both** of that rect's axes
  (`STACK_MAX_COLS=84`, `BOOK_MAX_ROWS=26`), so resizing the window routinely
  leaves the rect identical: the key still matched, nothing was re-transmitted,
  and the terminal — tmux in front of kitty especially — had already dropped or
  misplaced the image. The side-by-side layouts hid it because their object takes
  the **full pane height**, so any vertical resize changes `rows` and forces a
  fresh transmit.
  Fix, three parts: `RichState::resized()` called from `app::redraw` (the one
  funnel that sees every draw *and* the terminal size — new `App::term_size`);
  the rect's `x`/`y` added to `RichKey` so a frame that merely *moved* re-sends;
  and a **delete before transmitting** when the span changes, since a stale
  placement composites as a torn image — the same symptom as a lost one.

## Technical gotchas

- **`params.moving` is only set in `app::redraw`.** A test that calls
  `terminal.draw(|f| ui::draw(f, app))` directly inherits a stale flag, so a
  "spinning" frame silently renders through the *parked* path. Cost an hour of a
  wrong-looking diagnostic; set it explicitly in any hand-rolled draw loop.
- **ratatui `Paragraph` does not blank its area** — only the cells it writes.
  The floating header therefore costs the image ~55 placeholder cells out of
  2016, not two full rows; useful when reasoning about what overdraws an image.
- **Overlap assertions must use the divider's own axis.** Stacked panes share
  their x range entirely; an x-only "panes don't overlap" sweep fails on
  `BookTop` at every width.
- `ratatui::layout::Rect::union` exists in 0.29 — used for the key-bar hull, no
  helper needed.
- **Rich escapes go to real stdout** unless `RichState::with_writer` is used.
  A scratch diagnostic sprayed ~100 KB of base64 through the test output before
  that registered.
- Dropping rotation makes `BookLeft`/`BookBottom` unconstructible and trips
  `dead_code` on the enum variants — worth knowing if the rotation key is ever
  removed again (restoring `t` made it moot here).

## Verification

- `cargo test --workspace` 217 passed / 0 failed / 9 ignored; clippy clean; fmt
  clean.
- Layout measured with the `print_layout` dev aid (extended this session with an
  86x30 breakpoint case and a panel-hidden 180x44 frame): book ink centre 90 vs
  window centre 90 at 180x44; 55/55 at 110x32; 43/43 at 86x30; stacked 44x26 band
  rows 0–12 with the menu centred in the lower half.
- The resize bug is guarded by `a_resize_retransmits_the_parked_image` (portrait
  120x70 → 130x70, where the rect stays 84x26 with both caps binding). Confirmed
  it **fails** with the `resized()` call commented out and passes with it.
- Ruled out for that bug, so it need not be re-investigated: placeholder coverage
  (identical across all four orientations — 1961/2016 for both stacked), escape
  shape, wide-short raster clipping (84x24 → 756x456 px, book well inside), and
  the spin→park diff (every placeholder cell differs from the glyph frame, so
  ratatui sends them all).

## Deferred

- On a 180-column window the tab menu sits ~30 columns from the book, centred in
  its own pane. Right-aligning the menu block in its pane is a one-liner if it
  reads as disconnected.
- If tearing survives the resize fix in a real tmux pane, the next suspect is
  whether the placement survives a tmux pane redraw at all — a different remedy
  (re-transmit on focus/redraw events).
