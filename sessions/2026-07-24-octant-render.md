# 2026-07-24 — octant-resolution book + 20% shrink

Followup to the TUI 3D book: user said the book was still ~20% too big on a full
terminal and its resolution too low. Task: shrink it, and raise render quality
within the block-glyph framework if possible.

## Decisions locked
- **Shrink 20%** via `scene::fill_for` endpoints ×0.8 (`0.88→0.70`, `0.60→0.48`).
  Absolute-size cap, not window-tracking — same mechanism as before.
- **Octants over a framework change.** Block-glyph resolution ceiling is the
  glyph family: quadrants = 2×2 subpixels/cell, octants (Unicode 16, 2×4) = 2×
  vertical detail, still tmux-safe. Rejected kitty/sixel (don't survive tmux —
  the whole reason for block glyphs). User chose octants + supersampling.
- **`ss` 2→3** (9 rays/subpixel) so the finer octant edges don't alias; cover
  texture density `cols*3 → cols*4`.
- **`--glyphs quadrant|octant` flag** (default octant) on TUI + `--dump-frame`,
  because octants need a Unicode-16 font and degrade to tofu otherwise. Quadrant
  path kept first-class as the fallback.

## Technical gotchas
- **Octant mask→char table is NOT a linear range.** 256 patterns, but only 230
  have dedicated code points at `U+1CD00..=U+1CDE5`; the other 26 reuse existing
  glyphs scattered across Block Elements / Symbols for Legacy Computing:
  - 16 quadrant/half/full patterns → `U+2580..U+259F` + space.
  - corner singles → `U+1CEA0/1CEA3/1CEA7/1CEAB` (half-width quarter blocks).
  - `U+1FB82` upper-¼, `U+2582` lower-¼, `U+1FB85` upper-¾, `U+2586` lower-¾
    (full-width row blocks), `U+1FBE6/1FBE7` middle-left/right quarter blocks.
  Do **not** hand-write this. Generated from `UnicodeData.txt` (16.0): parse
  `BLOCK OCTANT-<positions>` names → mask, fill the 26 gaps from the list above.
  Bit order = reading order (bit0 TL, then L→R, top→bottom = octant positions
  1..8). Guarded by `octant_masks_round_trip` (mask→glyph→mask for all 256) and
  `octant_table_uses_every_dedicated_code_point_once` (no dup glyphs).
- **Physical aspect is independent of subpixel count.** `primary_ray` aspect
  stays `cols/(rows*2)` for both families — it's the cell grid's physical shape,
  not the sample grid's. Only the framebuffer *height* changes: `rows*cell_h()`
  (2 or 4). Octant subpixels come out ~square as a bonus.
- **`blit.rs` generalized** from hardcoded `[_;4]` to slices + `GlyphSet`.
  `plan()` now uses `full = (1<<n)-1` and searches `1..full` splits (254 for
  octants vs 14). `RgbBuf::cell` returns `([Option<Vec3>;8], n)`. `to_png` draws
  every cell 2×4 image px regardless (sub_h = `4/cell_h`), so quadrant subpixels
  are 1×2 and octant 1×1 — honest proportions.
- clippy: `field_reassign_with_default` on the dump path — use struct-update
  `RenderParams { glyphs, ..default() }`, not `let mut` + reassign.

## Verification
- `cargo test --workspace` — 90 passed / 1 ignored. New: octant round-trip +
  dedup guards; rewrote 2×2-assuming blit tests for both families; fixed
  `renders_a_silhouette` dims (120×80 → 120×160, centre probe 60,80).
- `every_screen_draws_at_every_size` still green (layout panic = wrecked pane).
- clippy `--workspace --all-targets` clean.
- Release `--dump-frame 200x55 --dump-png` for octant vs `--glyphs quadrant`:
  same object, visibly smaller than before, both quantize without artifacts.
- Perf: full dump run ~20ms incl. process startup + DB open — trace itself well
  under the 20fps budget; frames cache between ticks anyway.

## Deferred / unverified
- **Font support in the user's real terminal** — the PNG proves the quantization
  logic, but whether octant glyphs actually render (vs tofu) depends on the tmux
  pane's font. User to eyeball `cargo run --release -p readingbuddy-tui`; falls
  back with `--glyphs quadrant`.
- Table generator lives only in the scratchpad (Python over `UnicodeData.txt`).
  If octants ever need regenerating, re-derive from the 16.0 UCD, not memory.
