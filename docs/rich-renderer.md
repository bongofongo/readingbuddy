# The rich (pixel) book renderer

Status: **implemented, and opt-in.** The pixel path works and looks good, but an
*animated* one is too expensive for a TUI whose whole point is being lightweight
in a tmux pane. So `--render auto` picks glyph; rich is reached with
`--render rich` or `v` in the book view.

## Why auto is glyph

Two rounds of tuning did not fix the spin. The numbers say why:

| | bytes/s | what the terminal does with them |
|---|---|---|
| glyph spin | ~0.45 MB/s (upper bound, full repaint) | writes cells into a grid it already maintains |
| pixel spin, after tuning | ~0.78 MB/s | zlib-decompress, re-upload a whole texture, recomposite |

Only ~1.7x the bytes — and still unusable. **Byte rate is the wrong metric.** A
byte of image payload costs a terminal far more than a byte of text, so tuning
the pixel budget cannot converge on "as light as glyphs": we would have to drop
to absurd resolution and it would *still* cost more. The only way to be
lightweight while animating is to send no images while animating.

That conclusion is what the next round should build on — see "Middle ground"
below. Everything else in this note describes machinery that works and should be
kept.

## Why this replaced the original design

The first version of this note gated rich mode to *non-tmux*, on the assumption
that kitty graphics cannot survive tmux. That assumption made rich mode dead
code in practice — the app is designed to live in a tmux pane — and it turned
out to be wrong.

kitty graphics survive tmux fine, given two things:

1. **DCS passthrough** for the escapes (`ESC P tmux; … ESC \`, inner ESCs
   doubled), gated behind tmux's `allow-passthrough`.
2. **Unicode placeholders** for placement, so tmux never has to understand that
   an image is involved.

Measured in kitty + tmux 3.5a, the full query/response round trip works:

```
sent:  _Ga=q (wrapped)   CSI 16 t (wrapped)   CSI c (bare)
back:  \x1b[?1;2;4c  \x1b_Gi=31;OK\x1b\\  \x1b[6;38;20t
```

So the real question was never "tmux or not" — it is "what can this terminal
do", which is a ladder, not a boolean.

## The capability ladder

`render3d/caps.rs` probes once at startup (raw mode on, alternate screen
entered, before ratatui reads any input) and produces a `Caps`. `--render`
overrides the verdict; `Auto` takes the best tier available.

| Tier | Requires | Result |
|---|---|---|
| `Rich` | kitty graphics answers, and bytes can reach the terminal | true RGBA pixels |
| `Glyph` (octant) | truecolor + a Unicode-16 font | today's block-glyph render |
| `Glyph` (quadrant) | anything | `--glyphs quadrant`, tofu-proof |

Three queries go out together. **DA1 (`CSI c`) is deliberately unwrapped** so
tmux answers it locally: it is a sentinel proving the terminal is replying at
all, which turns "no `_G` reply" into a *definitive* no rather than an
indistinguishable timeout. Only total silence (not a tty, or a terminal that
answers nothing) falls back to environment heuristics.

Two behaviours worth knowing, both learned by measurement:

- **Replies arrive out of order.** tmux answers DA1 immediately while kitty's
  replies make the longer trip, so stopping at the sentinel would miss them.
  Hence the grace period after DA1.
- **tmux routes input to the active pane only.** Probing from a background pane
  sees DA1 but never kitty's answer, and concludes "no graphics". A false
  negative, but a safe one — it degrades to glyphs, only bites if the pane is
  unfocused during the first 200 ms, and `--render rich` or `v` overrides it.

## tmux passthrough, without touching your config

`allow-passthrough` is a **pane** option. The app sets it on its own pane at
startup (`tmux set -p allow-passthrough on`) and puts it back on the way out —
restoring by *unsetting* if the pane had no value of its own, so inheritance
resumes. No `tmux.conf` edit, no user action, and it dies with the pane even if
the process is killed outright.

The debt is tracked in an `AtomicU8`, not a `Mutex`: the panic hook has to
restore it, and a mutex there could deadlock on a lock the panicking thread
already holds.

## Placement: Unicode placeholders, everywhere

Placement is **not** an escape sequence. It is ordinary buffer cells:
`U+10EEEE`, with row and column in combining diacritics and the image id in the
foreground colour. Used in bare kitty as well as tmux, so there is one code
path.

This is what makes everything else free. ratatui's diff engine, `split_rects`,
pane rotation (`t`), the divider (`[`/`]`), resize, and the title floating over
the object all work with no special handling: the header simply overwrites some
placeholder cells and the image does not draw there. Verified live — with a
38-column header, the placeholders resume mid-row carrying the correct column
diacritic.

Two details that are load-bearing rather than incidental:

- **Both diacritics are always emitted.** kitty allows a cell to inherit them
  from the cell on its left, but ratatui's `Buffer::diff` emits only changed
  cells in possibly non-contiguous runs, so "the cell to the left" may never
  have been written. Four extra bytes removes the whole class of bug.
- **The image id stays under 2^24**, so it fits the 24-bit foreground colour and
  the third ("most significant byte") diacritic is never needed.

The diacritic table is generated from the `rowcolumn-diacritics.txt` that ships
inside kitty.app, not transcribed — same discipline as `blit.rs`'s octant table,
guarded by `diacritic_table_is_intact`.

## The pixel raster

`render3d/raster.rs` is a second raster alongside `render`, not a replacement.
The glyph path's `hits * 2 >= samples` majority rule is right for glyphs (the
terminal background is unknown, and the glyph chooser recovers sub-cell detail
from the binary mask) but gives a hard stair-stepped silhouette in a pixel
image. Keeping them separate also means `render` is untouched, so `--dump-frame`
stays byte-identical *by construction*.

**Antialiasing is nearly free.** One ray per pixel, then re-trace only the
pixels whose 4-neighbours disagree about being on the book. A silhouette's
perimeter is O(√N) against the area's O(N), so full supersampling on the
boundary costs under 1% of the frame.

Both rasters go through `scene::camera_origin` → `fill_for(rows)`, which stays
the sole authority on the object's absolute size. The book must not grow just
because it now has more pixels; `the_book_keeps_its_absolute_size_across_rasters`
guards it.

### Resolution budget, and why it is small

Native cell resolution (`cols × cell_px`), scaled down uniformly until it fits
a cap. Uniform scaling keeps the aspect exactly, and since kitty stretches the
image into the same cell box, the pixel path has the *true* aspect rather than
the glyph path's `cols : rows*2` approximation.

| Tier | Cap | Trace |
|---|---|---|
| Motion (pose changing) | 0.12 MP | ~2 ms |
| Settle (pose unchanged since last draw) | 1.2 MP | ~8 ms |

Crisp when parked, capped when moving. The tier is chosen by comparing the pose
to the previous draw's, so it needs no plumbing from the event loop.

**The first version of this shipped at 0.42 MP and made the terminal unusable.**
The trap is that none of our own numbers show it: trace + encode is a few ms,
`write` + `flush` never blocks, and a bench pushes 270 fps at 27 MB/s without
complaint. The cost lands *inside the terminal*, which decompresses and
re-uploads the whole texture every frame while tmux re-parses it on the way.

Measured with a **real** cover (a photographic one compresses several times
worse than the procedural plate — never re-derive this from a synthetic
fixture), 50x26 cell rect:

| budget | render | KB/frame | MB/s @20fps |
|---|---|---|---|
| 420K (the bad version) | 651x644 | 347K | 7.1 |
| 200K | 449x444 | 170K | 3.5 |
| 120K (current) | 348x344 | 105K | 2.2 |

Two changes together brought 7.1 MB/s down to **~0.8 MB/s**:

1. The motion cap dropped to 0.12 MP.
2. **Retransmission is throttled below the tick rate.** `present.rs` quantizes
   the pose two ways: a *fine* quantum (1/512 rad) decides whether the book is
   still moving, and a *coarse* one (`MOTION_Q`, 1/32 rad ≈ 1.8°) goes in the
   transmit cache key while it is. Consecutive 20 fps ticks usually land in the
   same coarse bucket and reuse the frame already on screen, so a spinning book
   retransmits at ~7 fps. Because the trace happens inside the transmit, a
   skipped frame costs neither bytes nor CPU. The book turns 13°/s, so 1.8°
   steps still read as motion. `a_spinning_book_retransmits_well_below_the_tick_rate`
   guards it.

Tracing is split across scoped threads (no new dependency), which took a
0.42 MP frame from 14.5 ms to 4.4 ms.

`--bench-rich N` pushes N frames at the terminal and reports the trace / encode
/ write split and the achieved rate. Run it in a real pane — it measures the pty
and the compositor, not just us.

**If the spin ever needs to be both smooth and full-resolution**, the answer is
a progressive turntable: quantize yaw to N steps, transmit each pose once under
its own image id, and animate by rewriting only the placeholder foreground
colour — pure text, zero bytes, free after one revolution. It costs terminal
memory (a buttery 27 s revolution wants ~540 poses) and needs the pitch nod
dropped or made commensurate so the loop closes.

## Teardown

Images must be deleted or they ghost. `kitty::teardown()` runs as the **first**
step of `restore_terminal` — before leaving the alternate screen, since the
placement lives on that screen — and the panic hook inherits it. The live image
id is an `AtomicU32` for the same reason the passthrough debt is.

Deleting also happens when leaving rich mode via `v`: the placeholder cells stop
being written, but the image would otherwise stay composited under the glyphs.

## Where it plugs in

`RichPresenter` in `render3d/present.rs`. It takes `&mut Scene` *and*
`&mut RichState`, both direct fields of `App` — see the borrow note in
`ui/book.rs::present_book` before changing those signatures.

Escapes are written from inside `draw_book`, which is correct ordering rather
than a hack: `Terminal::draw` only mutates the `Buffer` and does not touch the
wire until it flushes afterwards, so the image always lands before the cells
that reference it. The writer is a `RichState` field so tests capture escapes
instead of spraying control bytes through the harness.

## Middle ground — the next round

The goal is a book that stays as cheap as glyphs while moving, and becomes
high-resolution when it isn't. Options, best first:

### 1. Glyph in motion, pixels at rest (recommended)

Route `Quality::Motion` to the **glyph** presenter and `Quality::Settle` to the
pixel one. The spin then costs exactly what it costs today — the path already
proven lightweight — and the pixel image is transmitted **once**, when the book
parks, at the full 1.2 MP budget.

Why this is the cheap one to build: both backends already sit behind
`BookPresenter`, and `RichPresenter` already distinguishes motion from rest with
no event-loop plumbing (`FINE_Q` pose comparison). It is mostly a routing change
plus deleting the image when motion resumes.

Open questions: the glyph→pixel transition is a visible "pop" (possibly good —
it reads as focusing); and `space`/`r`/layout changes must reliably drop the
image.

### 2. Progressive turntable

Quantize yaw to N steps, transmit each pose once under its own image id, animate
by rewriting only the placeholder foreground colour — pure text, zero bytes,
free after one revolution. Full resolution *and* smooth.

Costs: terminal memory (a buttery 27 s revolution wants ~540 poses; ~130 MB at
low res), a first revolution that is as expensive as today, and the pitch nod
must be dropped or made commensurate with the yaw period so the loop closes.

### 3. Give up the idle spin in rich mode

Rich mode renders one crisp still book; `space` starts a spin that falls back to
glyphs. Effectively option 1 with a simpler rule.

**Not viable:** further tuning of `MOTION_MAX_PX` or the retransmit throttle.
That road was walked twice; see "Why auto is glyph".

## Also still to do

**Art direction.** The pixel path currently uses the existing shader. The
intended look is a stylized soft-shade "product render": smooth non-physical
gradients, exaggerated fresnel rim so the silhouette pops, a gentle sheen across
the cover, and a soft contact shadow grounding the book. Clean and high-res, not
photoreal, and keeping the existing key/fill/cream palette and its luma-clamped
accent discipline. Worth doing *after* the middle ground lands, since a
still-frame renderer can afford much more shading than an animated one.

Sixel is deliberately not implemented (kitty has no sixel, so it would be
untested dead code). `Caps` leaves room for it.
