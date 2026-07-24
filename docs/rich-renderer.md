# The rich (non-tmux) book renderer — design note

Status: **design only, not implemented.** The seam exists
(`crates/tui/src/render3d/present.rs`); `RichPresenter` currently delegates to
the glyph path so `--render rich` is always safe.

## Why a second renderer

Block glyphs are the *only* presentation path today, on purpose: kitty-graphics
and sixel do not survive tmux (see `render3d/blit.rs` and CLAUDE.md). Outside
tmux that constraint is lifted, so the single-book view can use a true pixel
protocol for a higher-quality object. Mode is chosen once at startup:

- `$TMUX` set → glyph. `$TMUX` unset → rich. Overridable with
  `--render auto|glyph|rich` (`main.rs`, `RenderArg::resolve`).
- Rich is gated to non-tmux precisely so the pixel protocol is viable.

## Plan

1. **Protocol:** kitty graphics protocol (APC `_G` escapes) first — best
   coverage (kitty, Ghostty, WezTerm) and true RGBA. Sixel as a fallback tier.
2. **Pixels, not glyphs:** reuse the existing cuboid raytracer
   (`render3d::render`) at a higher supersample to produce an `RgbBuf`, convert
   to RGBA, and place it at the book cell rect. `Scene` already caches per-pose
   frames, so the 20fps spin loop reuses today's invalidation gate (re-trace
   only when the pose moved).
3. **Cell↔pixel mapping:** translate the book `Rect` (cells) to a pixel box via
   the terminal's reported cell size (kitty reports it; else query/assume).
4. **Teardown:** emitted images must be deleted on redraw and on exit to avoid
   ghosting — the panic/restore hook (`main.rs::restore_terminal`) gains a
   "delete images" step.
5. **Art direction ("Nintendo, hi-res", later, larger effort):** stylized
   shading ramps / rim light in the shader; optionally pre-rendered turntable
   frames for buttery spin. All behind the same `RichPresenter` seam.

## Where it plugs in

`RichPresenter::draw_book` in `render3d/present.rs`. Nothing else in the book
view changes: the 3-state responsive layout (Stacked/Split/Compact) is
renderer-agnostic and already hands the presenter a final inner `Rect`.
