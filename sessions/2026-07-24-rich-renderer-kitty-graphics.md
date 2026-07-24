# 2026-07-24 — Situation-aware rendering: the kitty-graphics pixel path

The book now renders as true RGBA pixels wherever the terminal can take them,
and falls back to block glyphs everywhere else. Frontend-only; engine untouched.

The brief was "optimise the default graphics for whatever situation there is",
which turned out to mean discarding the premise the previous session had locked
in.

## The premise that was wrong

`docs/rich-renderer.md` gated rich mode to **non-tmux**, on the belief that
kitty graphics cannot survive tmux. Two problems:

1. The app is designed to live in a tmux pane, so rich mode was unreachable in
   the environment it ships for — dead code by construction.
2. "tmux or not" is a boolean where the real world is a ladder: protocol
   support, passthrough, cell pixel size, font coverage.

kitty graphics *do* survive tmux. Measured, in kitty + tmux 3.5a:

```
sent:  _Ga=q (tmux-wrapped)   CSI 16 t (tmux-wrapped)   CSI c (bare)
back:  \x1b[?1;2;4c  \x1b_Gi=31;OK\x1b\\  \x1b[6;38;20t
```

Everything came back. Cell size 20x38 px, matching the estimate from
`font_size 16` on retina.

## Decisions locked

- **Rich works inside tmux.** DCS passthrough for transmission, Unicode
  placeholders for placement.
- **`allow-passthrough` is a pane option**, so the app sets it on *its own pane*
  and restores it on the way out. No `tmux.conf` edit, no user action, dies with
  the pane. The user's constraint was explicitly "don't make me change other
  software's config" — pane scope is what satisfies it.
- **Unicode placeholders in bare kitty too**, not just tmux. One code path, and
  placement becomes ordinary ratatui cells.
- **Full cell resolution, capped** — 0.42 MP moving / 1.2 MP settled. Crisp when
  parked, capped when spinning.
- **Art direction deferred.** The pixel path ships with today's shader; the
  stylized soft-shade "product render" look is a follow-up, so it can be judged
  against real covers on real pixels rather than guessed at.
- **Sixel deliberately not built.** kitty has no sixel; it would be untested
  dead code. `Caps` leaves room.

## Measurements that drove the design

Release build, book rect 50x26 cells at 20x38 px/cell:

| | before threading | after |
|---|---|---|
| Motion frame (0.42 MP) | 14.5 ms | **4.4 ms** |
| Settle frame (0.99 MP) | 20.0 ms | **7.4 ms** |
| Full pane settle (1.2 MP) | 20.7 ms | **6.6 ms** |

14.5 ms was too close to the 50 ms tick once zlib and the pty write are added.
Scoped threads (no new dependency) fixed it. **Judge this on release only** —
debug is ~30x slower and will libel the design.

## Technical gotchas

- **Both placeholder diacritics must always be emitted.** kitty lets a cell
  inherit row/col from the cell on its left, but ratatui's `Buffer::diff` emits
  only changed cells in possibly non-contiguous runs — "the cell to the left"
  may never have been written this frame. Confirmed by live capture: with a
  38-column header over the object, the placeholders resume mid-row carrying the
  correct column diacritic (U+0598 = index 40). The shorthand would have
  mis-placed that whole run.
- **A placeholder cell must measure width 1.** If `unicode-width` ever classed
  U+10EEEE as wide, `Buffer::diff` would skip every second cell and the object
  would tear. Asserted in a test, not assumed (`unicode-width` as a
  dev-dependency purely for this).
- **Replies arrive out of order.** tmux answers DA1 locally and immediately;
  kitty's replies make the longer trip and land *after* it. Stopping at the
  sentinel would miss both — hence the grace period. This was not theoretical;
  the first working probe found it.
- **tmux routes input to the active pane only.** Probing from a background pane
  sees DA1 but never kitty's `_G` reply, so it concludes "no graphics". A false
  negative, but safe: it degrades to glyphs, only bites during the first 200 ms
  of startup, and `--render rich` / `v` override it.
- **`script(1)` is useless for verifying terminal queries** — it swallows the
  replies. An early run appeared to prove kitty detection worked through tmux
  when in fact `raw reply` was a single EOT byte and the `true` came from the
  env heuristic. `tmux new-window` + `capture-pane` is the honest harness;
  printing the raw reply bytes in `--probe` is what exposed the mistake.
- **Atomics, not mutexes, for teardown state.** The panic hook restores the
  passthrough option and deletes the image; a mutex there could be poisoned or
  still held by the panicking thread.
- **`teardown()` must run before leaving the alternate screen** — the placement
  lives on that screen.
- **Three disjoint field borrows still work.** `presenter_for(mode, &mut
  app.scene, &mut app.rich)` alongside `&app.view` compiles for the same reason
  two did: all are *direct* field paths of `app`. Routing any through a
  `&mut App` method still breaks it.
- **Floor, don't round, when scaling to a pixel cap.** Rounding both axes up can
  carry the product just past the budget (814x516 = 420,024 against 420,000).

## Drive-by fix

`Scene::frame`'s cache key omitted `ss` and `glyphs`. Since `glyphs` fixes the
framebuffer height (`rows * cell_h`), toggling glyph families could serve one
frame at the wrong height. The key is now a named `FrameKey` struct with both
fields, guarded by `switching_glyph_family_invalidates_the_cached_frame`.

## Structure

- `render3d/caps.rs` — the probe, `Caps`, tmux passthrough management.
- `render3d/kitty.rs` — escapes, the generated 297-entry diacritic table,
  placeholder encoding, process-global teardown.
- `render3d/raster.rs` — RGBA raster with edge-only antialiasing.
- `render3d/present.rs` — `RichState` + a real `RichPresenter`.

`render` itself is **untouched**, which is what makes `--dump-frame` byte-
identical by construction rather than by promise. Verified against captured
baselines at 100x30, 50x26, 200x55 and 1x1 after every step.

## Verified

- `cargo test --workspace` — 172 passing, 4 ignored (timing/layout dumps).
- `cargo clippy --workspace --all-targets` — clean.
- `--dump-frame` byte-identical at four captured baselines (100x30, 50x26,
  200x55, 1x1) after every step. `render` is untouched, so this holds by
  construction rather than by luck.
- Live in kitty + tmux: 2302 placeholder cells across 47 rows, fg
  `176;120;50` = the expected image id; `v` toggles to 42 block glyphs and back;
  `allow-passthrough` `off` before and `off` after quit.
- **Not verified: how it feels.** Both performance rounds were judged by the
  user, not by our instruments — which is the whole lesson above.

## Housekeeping

`cargo fmt --all` reformats ~340 lines across the engine and CLI: the repo
carries **pre-existing rustfmt-version drift** and `make check`'s `fmt-check`
already fails on `main`. That churn was reverted rather than bundled in here.
Fix it as its own commit (`cargo fmt --all`) if wanted.

## The performance trap (found after first use)

First run in anger: "performance is atrocious and unusable". Worth recording in
full, because every instrument we had said the opposite.

- Trace: ~2–4 ms/frame. Encode: <1 ms. `write` + `flush`: 1.2 ms, **never
  blocks**. A 400-frame bench sustained **270 fps at 27 MB/s** with no
  backpressure whatsoever.
- The synthetic `frame_budget` test said 49 KB/frame. With a **real**
  photographic cover it is **340 KB** — the procedural plate compresses several
  times better, and using it as the fixture hid the whole problem.
- So the app was fine and the *terminal* was drowning: 0.42 MP at 20 fps is
  7.1 MB/s of continuous whole-texture replacement, ~140x what the glyph path
  sends, decompressed and re-uploaded per frame by kitty and re-parsed by tmux.
  Neither pushes back — they just buffer and then starve input handling.

**Lesson: a renderer that never blocks can still be far too expensive.** The
budget that matters is the one the *consumer* pays, and nothing on our side
reports it. Measure bytes/second, with representative data, in a real pane.

Fixed by two changes totalling ~9x:

1. `MOTION_MAX_PX` 420K → 120K.
2. A retransmit throttle: two pose quantizations, fine (1/512 rad) for "is it
   still moving", coarse (`MOTION_Q` = 1/32 rad ≈ 1.8°) as the transmit cache
   key while moving. Consecutive 20 fps ticks share a coarse bucket, so a
   spinning book re-sends at ~7 fps — and since the trace lives *inside* the
   transmit, a skipped frame costs no CPU either.

`--bench-rich N` and the `wire_rate` test exist so this is measurable next time
rather than re-derived.

### Round two: tuning was the wrong lever

The 9x reduction still wasn't enough — "performance is still not cutting it at
all, and this is really breaking the lightweight goal of the TUI". The
comparison that explains it:

| | bytes/s | what the terminal does with them |
|---|---|---|
| glyph spin | ~0.45 MB/s (full 50x26 repaint, upper bound) | writes cells into a grid it already has |
| pixel spin, tuned | ~0.78 MB/s | zlib-decompress, re-upload a whole texture, recomposite |

**Only ~1.7x the bytes, and still unusable — so byte rate is the wrong metric.**
A byte of image payload costs a terminal far more than a byte of text. Tuning
`MOTION_MAX_PX` or the throttle can therefore never converge on "as light as
glyphs": we'd have to drop to absurd resolution and it would *still* cost more.
The only way to be lightweight while animating is to **send no images while
animating**. Do not attempt a third round of tuning.

Consequence: **`--render auto` now picks glyph even where pixels work.** Rich is
opt-in (`--render rich`, or `v`) until the spin stops sending images. Shipping a
default the user has twice called unusable would have been wrong, and the flip
is one match arm.

## Next thread: the middle ground

Goal: as cheap as glyphs while moving, high-resolution when it isn't. Ranked in
`docs/rich-renderer.md` → "Middle ground". Summary:

1. **Glyph in motion, pixels at rest** (recommended). Route `Quality::Motion` to
   `GlyphPresenter` and `Quality::Settle` to `RichPresenter`. The spin costs
   exactly what it costs today; one image is transmitted when the book parks, at
   the full 1.2 MP budget. Cheap to build — both backends already sit behind
   `BookPresenter`, and motion-vs-rest is already detected (`FINE_Q`) with no
   event-loop plumbing. Watch: the glyph→pixel "pop", and dropping the image
   reliably when motion resumes (`space`, `r`, layout changes, screen changes).
2. **Progressive turntable.** Transmit each quantized yaw once under its own
   image id, animate by rewriting only the placeholder fg colour — zero bytes,
   full resolution, free after one revolution. Costs ~130 MB of terminal memory
   for a smooth 27 s turn, a first revolution as expensive as today, and needs
   the pitch nod dropped or made commensurate so the loop closes.
3. **Drop the idle spin in rich mode** — a simpler rule with the same effect.

Everything built this session is reusable as-is: the probe, the passthrough
handling, the encoder, the placeholder scheme, the RGBA raster and the teardown
are all orthogonal to *when* frames get sent.

## Deferred

- **Art direction** — the stylized soft-shade pass (smooth ramps, exaggerated
  fresnel rim, cover sheen, contact shadow). The contact shadow wants the miss
  path of the RGBA raster, which is another reason that raster is separate.
- **Progressive turntable** — if the spin should be both smooth *and* full
  resolution: transmit each quantized yaw once under its own image id, animate
  by rewriting only the placeholder fg colour (pure text, zero bytes), free
  after one revolution. Costs terminal memory (~540 poses for a buttery 27 s
  turn) and needs the pitch nod dropped or made commensurate so the loop closes.
- **Ghostty / WezTerm** — both implement virtual placements, neither tested
  here. Safe by construction: no `_G` reply means glyphs.
- **`Caps` not persisted** — the probe runs every launch. It costs ~200 ms only
  on a silent terminal; a real one answers in a few ms.
