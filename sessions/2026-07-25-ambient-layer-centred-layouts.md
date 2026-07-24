# 2026-07-25 — ambient layer + centred/shrink-wrapped layouts

Making the TUI feel alive at large terminal sizes. At 120x40+ the menu was a
~63x14 box marooned in ~2900 empty cells, the list screens stretched edge to
edge, and the book sat hard against the left edge with all surplus width dumped
into the info panel. Frontend-only; engine untouched.

## Decisions locked

- **Ambient background layer** on the non-book screens only (menu / library /
  search / settings). Never the book view — the object owns that pane, and it
  keeps the kitty hybrid's invariants entirely out of scope.
- **Two motifs, `Off` by default.** User chose off-by-default explicitly: a
  reading app should not start animating unasked. Cycled with `a` on the
  settings screen, persisted in `tui.toml`.
- **Book view centred as a *block*** — book pane + capped info pane centred
  together with equal gutters. User picked this over lopsided true-screen-centre
  (which would have put the book at the terminal midpoint with a left gutter ~3x
  the right). At 200 cols: 51 / 50 / 48 / 51.
- **Library + search shrink-wrap** to their contents and centre on both axes,
  rather than the earlier width-cap-only treatment. A four-book library is a
  small centred box, not a mostly-empty frame.
- **Search selection highlight** scoped to the title span, matching the library.
  Kept search's existing `theme::primary()` weight rather than adopting the
  library's bold `theme::title()` — the fix is the reverse scoping, not a
  restyle.

## Bugs found

- **`persist_accent` blanked every other setting** (pre-existing, latent).
  It built `TuiConfig { accent }` as a struct literal and `config::save`
  serializes the whole struct over the file. Harmless while accent was the only
  field; the moment `ambient` was added, *changing the accent would erase the
  motif*. Now one `persist_config` writing from live state, via a pure
  `config_snapshot()` so the invariant is testable without touching the real
  `~/.config/readingbuddy/tui.toml`.
- **Search screen still had the whole-row highlight** — the exact bug fixed on
  the library screen in `2026-07-24-tui-polish`, missed on search at the time.
  Same fix: drop `List::highlight_style`, pass `selected` into the row builder,
  patch `REVERSED` onto the title span alone.
- **Contours repainted the entire terminal every frame** — 0.174 MB/s at 120x40,
  more than the actual 3D book (0.131). See gotchas; found only because the byte
  instrument was written before the motif was tuned.
- **Contours rendered as speckle, not curves.** The isoline band was thresholded
  in *field* units, so its on-screen width scaled inversely with the local
  gradient: sub-subpixel dashes where steep, blobs where flat.
- **Noise x/y frequency ratio was inverted.** An octant subpixel is square and
  needs equal frequencies; the code had y at 2x x, and quadrant off by 4x.
  Correct scale is `fy = fx * (4 / cell_h)`.
- Self-inflicted, caught by the tests themselves: a test that wrote to the
  user's real config dir; `print_ambient` hanging forever on `Motif::Off`
  (whose clock never advances); a mote-coverage assertion that was
  arithmetically unreachable; a panel-cap assertion that over-claimed.

## Technical gotchas

- **`blit()` cannot be reused as a background layer.** It writes *every* cell of
  its rect unconditionally (so it clobbers), and `plan()` brute-forces all `2^n`
  masks on a **fully covered** cell — 256 iterations at octant density. Added
  `blit_sparse()` beside it (coverage-forced split, no search, skips uncovered
  cells so it composites). Second path, never a replacement — same rule as
  `raster.rs` beside `render`, which is what keeps the book byte-identical.
- **ratatui `Block` styles the cells it doesn't draw but never blanks them.**
  Ambient glyphs showed through the interior of the menu/settings boxes and
  between list rows. Every content box needs `render_widget(Clear, rect)` first.
- **ratatui's diff only skips *byte-identical* cells — this is the whole cost
  story.** Contours drew each band with a brightness ramp; `blit_sparse` averages
  a cell's lit subpixels, so as the field drifted a hair every lit cell's
  averaged colour moved a fraction and the diff repainted the screen. The
  *glyphs* were already stable (~0.04 subpixels of motion per frame) — only the
  colour was moving. Measured at 200x50: continuous ramp 0.419 MB/s → 3 quantized
  steps 0.231 → **one flat ink 0.044**, then 0.019 after lowering `FREQ`. Motes
  are 0.006. Flat ink works because the mean of *any* subset of identically-inked
  subpixels is that same ink, so cell colour is exactly stable.
- **`animating()` must never be widened.** It means "the book is turning" and
  feeds `params.moving`; folding the ambient layer in would make a parked book
  report moving forever and never transmit its crisp kitty frame — with nothing
  on screen looking wrong. Separate `ambient_visible()` (drawn) /
  `ambient_animating()` (drifting), and `tick()` splits into
  `tick_book` + `tick_ambient`. Guarded by
  `ambient_dirties_without_claiming_the_book_is_moving`.
- **Quantize the ambient clock, not the redraw.** `advance()` reports a change
  only when the 4fps frame index moves, so 4 of every 5 ticks cost one integer
  compare. Same trick as `present.rs`'s pose buckets. `draw()` renders from the
  quantized index, not the raw clock, or the field creeps between redraws and the
  quantization buys nothing.
- **Isoline width must be normalized by the field gradient** to be constant in
  *subpixels* — otherwise speckle (above). Costs 3 noise evals instead of 1, so a
  cheap `NEAR_LEVEL` pre-filter skips the ~85% of subpixels that cannot be near a
  level for any plausible gradient.
- **Ambient ink cannot fade to invisible.** `blit_sparse` writes absolute RGB
  against a background the app deliberately doesn't know (the whole `Color::Reset`
  / `DarkGray` / `REVERSED` discipline exists to avoid needing to). Fading toward
  mid-grey lands on a still-visible grey. So ink sits near mid-luminance (reads on
  light *and* dark) and motifs disappear by dropping **coverage** instead.
- **Compose layout caps *outside* `split_rects`, not inside it.** `content_block`
  caps and centres the rect, then `split_rects` divides that — so the function
  keeps its tiling contract and **every one of its existing tests stayed valid**.
  Teaching it to stop tiling would have churned them all. `BOOK_MAX_COLS +
  PANEL_MAX_COLS = 98` is chosen so `split_widths(98) == (50, 48)` exactly: the
  two caps agree rather than one silently overriding the other.
- **`List::highlight_style` patches the entire row** — restated because the search
  screen proves the note from `2026-07-24-tui-polish` wasn't enough to stop it
  recurring. Any new list needs `selected` threaded into its row builder.
- **Selection styling is invisible in a symbol dump**, which is exactly why the
  search bug survived. `print_lists` now prints a `#` mask of cells carrying
  `REVERSED` beside each row; the bug shows as a full-width bar, the fix as 14
  cells. Assert with `cell.modifier.contains(Modifier::REVERSED)` on a
  `TestBackend` buffer, or `span.style.add_modifier` on the built `Line`.
- `readingbuddy::RankedResult` has no `Default` impl — construct it field-wise in
  tests.
- Sizing a shrink-wrapped box: `Line::width()` gives a row's rendered width, and
  the block **title counts too** (it sits in the top border). Chrome is 6: two
  borders, the two-column gutter `List` reserves for `highlight_symbol`, and a
  column of padding each side (`Block::padding(Padding::horizontal(1))`) —
  without the padding, rows press flat against both frame edges.

## Verification

- Green: fmt, clippy `--all-targets` (0 warnings), and the full workspace suite —
  **211 passed, 0 failed, 9 ignored** (engine 44, koreader_import 5, TUI 162).
- **Byte instrument** (`ambient_bytes` / `the_ambient_layer_stays_inside_its_byte_budget`
  / ignored `ambient_wire_rate` sweep) — written *before* tuning, and it caught the
  0.174 MB/s repaint that nothing on screen revealed. Budget 0.05 MB/s at 120x40
  against measured 0.002 (motes) / 0.010 (contours). `Off` asserts a hard 0.
- `every_screen_draws_at_every_size` extended to run its whole matrix once per
  motif, down to 1x1.
- New invariant tests: ambient dirties without claiming the book moves; the book
  screen is byte-identical with the layer on and off; a modal freezes the layer;
  persisting carries every setting; both motifs stay sparse (<15%) and never go
  blank; `blit_sparse` composites instead of clobbering.
- Visual review via ignored aids: `print_ambient` (each motif, three frames),
  `print_lists` (both list screens + reverse mask), `print_layout` (gained a
  180x44 case — the only size where the centred book block is visible).

## Deferred

- **`--bench-ambient` harness + a deliberately-bad `Storm` calibration motif**
  (the ambient analogue of `RenderMode::RichAlways`, so the instrument has seen
  the failure it exists to catch). Proposed and consciously skipped: for a 4fps
  sparse text layer that defaults off, the byte-budget test plus the ignored
  sweep is proportionate. Worth revisiting if the layer ever animates faster.
- OSC 11 background-colour query in `caps.rs`, which would let ambient ink blend
  *from* the real terminal background and become genuinely faint instead of
  mid-toned. Risk is one more reply to parse before ratatui takes stdin.
- Search titles are `primary()` while library titles are `title()` (bold) — a
  pre-existing inconsistency, left alone rather than smuggled into a bug fix.
- Motif tuning (`FREQ`, `LEVELS`, `mote_count`) was judged by eye in a dump, not
  in a live terminal.
