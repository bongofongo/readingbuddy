# Next thread — kickoff prompt: upgrade the book model's graphics

Copy the block below into a fresh thread to start work on the high-quality
(non-tmux) renderer.

---

We're upgrading the graphics of the 3D book model in the readingbuddy TUI
single-book viewer. The seam and the design already exist from prior sessions —
your job is to build the real thing behind it.

**Read first (in this order):**
- `docs/rich-renderer.md` — the design note for this exact work.
- `sessions/2026-07-24-render-modes-layout-customization.md` — how the render-mode
  seam and layout landed, plus the borrow-checker gotcha for the presenter.
- `sessions/2026-07-24-octant-render.md` — how the current block-glyph renderer works.
- `CLAUDE.md` → the `render3d/` bullets — the raytracer, `Scene` caching, blit,
  and the `RenderMode`/`BookPresenter` description.

**Where it plugs in:** `crates/tui/src/render3d/present.rs`. `RichPresenter`
currently delegates to the glyph path; that's the one function to make real.
Mode is already selected (`$TMUX` auto, `--render auto|glyph|rich`) and threaded
to `App.render_mode`, so nothing upstream should need to change.

**The goal / vision (from earlier discussions):** outside tmux, render the book
as true pixels rather than block glyphs — "Nintendo, but high-res": clean,
stylized, high-resolution art that still matches the design language. Reuse the
existing cuboid raytracer (`render3d::render`) at a higher supersample; the
constraint that forced block-glyphs (kitty-graphics/sixel dying in tmux) does
NOT apply here because rich mode is gated to non-tmux.

**Key facts / constraints:**
- Presentation is a deliberate swap point — `blit`/`Scene`/`scene` should not
  need changes to add a pixel backend; `Scene` already caches per-pose frames,
  so reuse the 20fps "only re-trace when the pose moved" gate.
- First protocol target: **kitty graphics protocol** (APC `_G` escapes), RGBA;
  sixel as a fallback tier later.
- Cell↔pixel mapping: translate the book `Rect` (cells) to a pixel box via the
  terminal's reported cell size.
- **Teardown matters**: emitted images must be deleted on redraw and on exit or
  they ghost — extend the panic/restore hook in `main.rs::restore_terminal`.
- The layout is renderer-agnostic: `split_rects` already hands the presenter a
  final inner `Rect`. Rotation/divider (`t`, `[`/`]`) must keep working in rich mode.
- Judge performance on a **release** build (debug is ~30x slower).

**Suggested first step:** propose a plan in plan mode — pick the crate for kitty
encoding (or hand-roll the escape), decide how to detect cell pixel size, and
sequence: static image first (no spin) → wire into the tick/pose-change gate →
teardown/ghosting → art-direction polish. Ask clarifying questions about the art
direction (shading ramps, rim light, pre-rendered turntable frames?) before
writing the plan.

**Verify** with a real terminal that supports the protocol (kitty/Ghostty/
WezTerm), plus keep `cargo test --workspace` green and
`every_screen_draws_at_every_size` passing (a layout panic wrecks a tmux pane).
`--dump-frame` output must stay byte-identical (glyph path untouched).
