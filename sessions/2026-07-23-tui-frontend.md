# Session log — 2026-07-23 — TUI frontend + ray-traced book object

Built `crates/tui` (bin `readingbuddy-tui`), the third crate: ratatui frontend
with menu → library → single-book view, whose centrepiece is a 3D book rendered
by a software ray tracer into terminal block glyphs. Engine untouched — the
`Engine` facade already exposed everything needed. 87 tests, clippy clean,
smoke-tested live through a pty.

## Decisions locked this session

- **Block glyphs, not a pixel protocol.** kitty-graphics/sixel don't survive
  tmux (`TERM_PROGRAM=tmux` here), and the target is a tmux pane. No
  `ratatui-image`, no protocol detection — one code path.
- **Ray-trace a cuboid, don't rasterize.** One box = slab intersection per
  subpixel: exact perspective, exact UVs, no z-buffer, no clipping, ~60 lines.
- **Separate crate with its own binary**, not a `readingbuddy tui` subcommand —
  keeps ratatui/crossterm/image out of the CLI crate.
- **Key bar is never fully hidden** in the book view; `o` only toggles
  collapsed (4 keys) ↔ expanded (8). A view with no visible way to the menu is
  a dead end. This replaced an earlier hidden-by-default options bar.
- **Spin about the book's own centre**, ~63s/turn. A pivot at the bottom of the
  spine ("like a top") was built and then reverted at the user's request — see
  Deferred.
- **Default pose tips the cover upward** (`yaw 0.44, pitch -0.40`). Pitch sign
  is the whole story: positive tips the top edge forward (cover faces *down*,
  the original), negative tips it back (cover faces *up*). Landed on -0.40 after
  trying -0.70 and -1.00; past ~-1.2 the cover foreshortens into nothing.
- Text uses `Color::Reset`/`DarkGray` and `REVERSED` selection so it reads on
  light and dark terminals; only accents are hard-coded RGB.

## Technical gotchas worth remembering

- **Half-blocks (1x2/cell) → quadrant blocks (2x2/cell) is the pixelation fix.**
  With `▀` alone, every silhouette edge stair-steps a *whole column* wide. All
  sixteen 2x2 block glyphs exist (U+2580..U+259F), so a cell can be a two-color
  quantization of four subpixels: try all sixteen splits, keep lowest squared
  error. Where a cell straddles the silhouette the split is *forced by coverage*
  — that's what buys quarter-cell edges. Doubles horizontal resolution for free.
- **Tie-breaking in the glyph chooser matters.** Seeding `best` with
  `f32::INFINITY` makes a flat cell pick mask 1 (`▘`) since it's the first with
  error 0. Seed with the solid block (mask 15) so flat cells and ties stay `█`.
- **Terminal subpixels are not square.** A cell is 1 wide x 2 tall, so with a
  2x2 sample grid the *sample* grid is square in count but the image's physical
  aspect is `cols / (rows*2)`. `primary_ray` must take the physical aspect —
  passing `w/h` of the framebuffer squashes the book.
- **Fit the camera by projecting the 8 corners, not a bounding sphere.** A book
  is flat, so a sphere fit leaves it tiny. Each corner imposes
  `dist >= q.z + |q.x|/(tx*fill)`. Evaluate over a full *yaw sweep*, not the
  current angle, or the book breathes in and out as it spins.
- **A fixed fill fraction is wrong.** It makes the object track the window, so a
  full terminal renders an enormous book. `scene::fill_for(rows)` slides 0.88 →
  0.60 as the pane grows, which caps *absolute* size. This was a real user
  complaint, not a theoretical one.
- **The back cover goes black without a fill light.** Ambient alone leaves any
  face turned away from the key unlit — very visible once the spin is a full
  360°. Now key + fill + ambient, plus an additive floor on the back board so a
  black jacket isn't a hole in the pane.
- **Cover paths in the DB are relative to the data root** (`./database/images/…`),
  so they don't resolve from an arbitrary cwd. `Scene::resolve_cover` falls back
  to `EngineConfig::images_dir` by filename. Symptom when missed: books silently
  render the procedural fallback cover.
- **Judge renderer performance on a release build.** 200x55 traces in ~1.7ms
  released, ~30x that in debug — the debug build sits at ~50% of a core and
  looks alarming for no reason.
- **`--dump-png` must render the quantized cells, not the raw framebuffer.**
  Dumping raw subpixels flatters the result and hides exactly the glyph
  quantization worth judging. It plans each cell as `blit` would and paints the
  2x2 quarters (1x2 px each, to keep proportions honest).
- Disjoint field borrows carry the draw path: `app.scene` (mut) alongside
  `app.view` (shared) is fine as long as the fields are named directly, not
  reached through a `&mut App` method.
- `Model` derives the front face's aspect from the cover image (covers are never
  stretched) and the thickness from `page_count` — a doorstop gets a visibly
  fatter spine than a novella.

## Verification

- `cargo test --workspace`: 87 tests (42 engine, 45 tui), all offline.
- `cargo clippy --workspace --all-targets`: clean.
- `app.rs::every_screen_draws_at_every_size` renders all three screens at sizes
  from 120x40 down to 1x1, options on and off — layout arithmetic is the
  likeliest panic site and a panic wrecks the user's tmux pane. Keep it passing.
- TUI tests use `sqlite::memory:` like the engine's, so nothing touches the real
  library.
- Live: driven through a pty (menu → library → book → rotate → options → back →
  quit), confirming alt-screen enter/exit, block glyphs on screen, continuous
  redraws while spinning, clean exit, no panic.
- Renderer inspected visually throughout via `--dump-frame … --dump-png`; that
  is how FOV was picked (0.70 keystoned badly, 0.30 read flat, 0.45 kept).

## Deferred

- **Pivot spin** — rotating about the bottom of the spine like a top was built
  (`to_world`/`to_local`, `Model::pivot`, a `camera_at` that centred on the
  swept volume) and reverted. It works, but the swept volume is ~2x the book's
  width, so the book renders much smaller and is never centred in its pane.
  Reinstating it means re-raising `fill_for` (0.95/0.72 was the compensation).
- Contact shadow under the book; title text rasterized onto the procedural
  cover; the animated pixel companion.
- Note/progress editing and search from inside the TUI — still CLI-only.
- `/` in the library list is reserved but unwired.
