# The rich (pixel) book renderer

Status: **the default, as a hybrid.** `Rich` means *block glyphs while the book
turns, true pixels the moment it parks*, and `--render auto` now picks it
wherever the terminal can take pixels. `v` still toggles, and `--render glyph`
still forces the old behaviour everywhere.

## Why the spin is glyphs

Two rounds of tuning did not fix an animated pixel book. The numbers say why:

| | bytes/s | what the terminal does with them |
|---|---|---|
| glyph spin | 0.09 MB/s measured (50x26), 0.13 MB/s on a full 120x40 pane | writes cells into a grid it already maintains |
| pixel spin, after tuning | ~0.78 MB/s | zlib-decompress, re-upload a whole texture, recomposite |

**Byte rate is the wrong metric.** A byte of image payload costs a terminal far
more than a byte of text, so tuning the pixel budget cannot converge on "as
light as glyphs": we would have to drop to absurd resolution and it would
*still* cost more. The only way to be lightweight while animating is to send no
images while animating.

That is now the rule rather than a conclusion looking for an implementation.
`RichPresenter` hands every moving frame to `GlyphPresenter` and deletes the
image first; only a parked pose transmits, once, at the full 1.2 MP budget.
`a_moving_book_sends_no_image_bytes_at_all` guards it — a zero, not a budget.

**Not viable, and please do not retry:** further tuning of `MOTION_MAX_PX` or
the retransmit throttle. That road was walked twice.

## How motion is known

`RichPresenter` used to *infer* motion by comparing the pose to the previous
draw's, which was elegant — no plumbing from the event loop — and is wrong for
the hybrid in exactly the case that matters. **A parked book stops redrawing at
all** (`App::tick` returns false, `dirty` is never set), so there is no second
draw in which to notice it parked, and the crisp frame would never be sent.

So `RenderParams::moving` is now set by `App` from `App::animating()`, the same
condition `tick` guards on — one source of truth, deterministic transitions in
both directions. The pose inference survives as the fallback when `moving` is
`None`, which is what `--dump-frame` and the presenter's own unit tests use.

The one subtlety: hiding the image on a motion frame must **not** clear
`last_pose` or the placeholder table, or the settle detection goes blind and the
image never comes back. Hence `hide_image` (cheap, per-transition) beside
`drop_image` (full reset, for leaving rich mode). `a_park_after_motion_still_transmits`
is the guard.

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
and the compositor, not just us. It deliberately bypasses `present.rs`, so it
measures the raster in isolation; for what a *mode* costs, use `--bench-render`
below.

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

## Measuring it — and why the old instruments could not

The first rich renderer shipped at 7.1 MB/s and made terminals unusable while
**every number we had said it was fine**: trace and encode were a few ms,
`write`/`flush` never blocked, and `--bench-rich` sustained 270 fps at 27 MB/s
without complaint. Everything we measured was on *our* side of the pty. The cost
is paid inside the terminal.

Three instruments now close that gap.

### 1. Bytes, split by class (`perf::CountingWriter`)

Wraps both the ratatui backend (text) and the graphics writer (image), so the
two are counted apart — a single total would hide the only thing worth knowing.
Wrapping the backend is also the only way the **glyph** path can be measured at
all: its bytes are produced by ratatui's diff, which the app never sees. That is
where the 0.09 / 0.13 MB/s figures at the top of this note come from, replacing
a number that had never been measured.

### 2. Terminal round-trip under load (`caps::rtt_probe`)

A terminal that is drowning answers slowly. So ask it, while loading it:

- **wrapped `_Ga=q`** is answered by the outer terminal → that terminal's queue;
- **bare `CSI c`** is answered by tmux locally → tmux's queue.

Two layers, read apart, which turns "it feels laggy" into "kitty is 200 ms
behind and tmux is fine". It reuses `maybe_wrap` / `parse_replies` /
the poll loop that the startup probe already had.

**Bench-mode only**, and not negotiable: it consumes stdin, which a live session
hands to `EventStream`. Sampling it from `app::run` would race the event reader
for the reply and eat keystrokes.

### 3. `--bench-render <glyph|rich|rich-always|all>`

Drives the **real presenter through a real `Terminal::draw`**, so the retransmit
throttle, the placeholder cells and ratatui's diff are all inside the
measurement. Fixed script (spin 100 → park 20 → spin 60 → slide the divider →
spin 40 → park 20), identical for every mode, `--bench-reps` to repeat it.

**Paced to the app's 20 fps tick**, and that is load-bearing rather than
cosmetic. The first version free-ran, which looked like the more rigorous choice
and was not: the retransmit throttle keys off *pose*, so at 1100 fps consecutive
frames land in different `MOTION_Q` buckets far more often than at 20 fps, and
`rich-always` sent **~50x** more than a real session would. Every MB/s figure was
inflated, and the three modes ran at three different frame rates, so the columns
were not even comparable to each other. `--bench-free-run` keeps the old
behaviour for throughput questions, and says so in the output.
Reports bytes/s by class, sends, mean trace and draw, and RTT p50/p90/max per
layer. `make bench` wraps it; `make bench BENCH_REPS=3` repeats the script, and
`BENCH_ARGS="--book x"` passes anything else through.

Each mode draws 25 warm-up frames that are discarded, and reps loop *outside*
modes so they interleave. Both are there because the first real comparison put
glyph first, straight after a release build: its opening 20 frames ran 40% slower
than the rest of its own run and it came out worst on every timing metric, which
is not believable for a path that draws the identical frame `rich` does while
moving. The byte columns are deterministic and need one rep; the latency columns
are not, and a surprising one should be re-run with `BENCH_REPS=3` before it is
believed.

The column that matters is **moving-KB**: image bytes emitted while the book was
animating. For the hybrid it is 0.

`rich-always` exists precisely so this is calibrated: it is the known-bad
configuration, kept and reachable only by flag, because an instrument that has
never seen the failure it exists to catch is not evidence of anything.

**It has now seen it.** Measured in kitty + tmux, 3 reps, paced and warmed up:

| mode | kitty p50 (3 reps) | kitty p90 (3 reps) |
|---|---|---|
| glyph | 3.3 / 3.3 / 3.3 ms | 3.7 / 3.6 / 3.5 |
| rich | 3.3 / 3.4 / 3.5 | 3.5 / 3.7 / 4.3 |
| **rich-always** | **8.3 / 7.2 / 8.3** | **9.1 / 9.1 / 10.6** |

A reproducible 2.2-2.5x separation with no overlap, while `glyph` and `rich` stay
indistinguishable from each other — which is what they should be, since they draw
identical frames in motion. tmux stays at 0.0-0.1 ms throughout, so the load lands
on the terminal, not on tmux. That is the two-layer split paying for itself.

If a future run does not reproduce that separation, suspect the harness before
the renderer, and read the sorted samples out of the JSONL — see the measurement
traps below.

Three conditions the harness warns about rather than silently tolerating: a
**cover that is missing or tiny** (the bench picks the *widest* cover in the
library and warns under 400px — a 180px OpenLibrary `-M` thumbnail upscaled into
the render compresses nearly as well as the procedural plate, at 27 KB/send
against ~105 KB), a **background tmux pane** (tmux routes input to the focused
pane only, so no replies come back and every mode quietly falls back to glyphs),
and a **paced run that missed its pacing** by more than 10%, which measured a
different workload than the one it reports.

### Measurement traps, all learned the hard way

- **Warm up.** The first 20 frames of a mode ran 40% slower than the rest of its
  own run. 25 frames per mode are drawn and discarded.
- **Interleave.** Modes used to run in blocks, so whichever went first absorbed
  the release build's IO tail; `glyph` came out worst on every timing metric,
  which is not believable for a path drawing the same frames `rich` does. Reps
  loop outside modes.
- **Read the distribution, not the percentiles.** `rich-always`'s samples are
  bimodal — a tight quiet floor plus a distinct loaded cluster. A p50/p90 pair
  flattens that into two numbers and invites comparison against noise. When a
  latency result looks null, sort the raw samples from the JSONL first.
- **Never let the probe outlive a tick.** At a 250 ms deadline, a run where the
  terminal never answered stalled 30 times per mode and dragged a paced 20 fps
  bench to 13 fps — while the byte columns looked entirely ordinary. The deadline
  is 40 ms and sampling gives up after two silent probes.
- **Probe after the draw.** Sampling before it puts every reading in the idle gap
  where the terminal has already drained; all three modes then report the same
  ~3.6 ms, which is just the quiescent round trip.

### The sandbox (`make bench-box`)

`make bench` needs your terminal: a real tty, a terminal that answers kitty
graphics queries, and — inside tmux — the *active* pane. `scripts/bench-sandbox.sh`
builds a throwaway one instead:

```
kitty (own process, pinned font + size, minimised)
  └── tmux (own server, own socket, -f /dev/null)
        └── readingbuddy-tui --bench-render all --bench-env box
```

What each layer is for:

- **Own tmux server** (`-L rbbench -f /dev/null`). The probe sets
  `allow-passthrough` on its pane; here that pane belongs to a server that lives
  for the length of the run and is then killed, so your `tmux.conf` is never read
  and your panes are never written. It also makes the pane the active one *by
  construction*, which retires the most common way to get a meaningless run.
- **Copied data root.** `database/` is a couple of hundred KB, so the bench opens
  a throwaway SQLite file — with the real covers, because a procedural or
  thumbnail cover compresses several times better and flatters every image column.
- **Pinned window and font** (`initial_window_*`, `font_family`). Rect and cell
  pixel size stop drifting between runs; two existing history rows differ by
  70x37 vs 78x47, which moves every byte column on its own.
- **A load gate.** `sysctl -n vm.loadavg`, refusing above `LOAD_MAX=2.0`.
  Contention does not surface as an error, it surfaces as a plausible latency
  column — the same run has shown `trace_us` swinging 1158 → 3530 within itself.
- **`taskpolicy -c user-interactive`**, so the work stays on an Apple Silicon
  machine's P-cores instead of being scheduled onto the efficiency cores.

Three things that look optional and are not, each a verified failure:

1. **`env -u TMUX -u TMUX_PANE`.** Inherited, they make the app wrap its queries
   in tmux DCS passthrough, which the fresh kitty — not being tmux — swallows
   whole. The probe then reports no graphics at all and the entire run is a
   glyph-only fiction that looks perfectly normal.
2. **stdout must stay a tty.** `probe_verbose` self-skips when it is not one, and
   a skipped probe silently downgrades every mode to glyphs. The summary goes to
   stderr precisely so it can be captured without redirecting stdout.
3. **The sandbox is not a live pane.** Different font, different size, no
   neighbours. Rows are tagged `env=box` for that reason; trend box against box.

Because it needs no terminal of yours, this is also the form the bench takes when
something other than a person at a keyboard runs it.

**Calibrated, same as the live harness** — the acceptance test for the sandbox is
that the known-bad mode still separates inside it. Measured, 3 reps, 80x38 object
rect, cell 15x28:

| mode | kitty p50 (rep 1 / 2 / 3) | kitty p90 |
|---|---|---|
| glyph | 3.1 / 3.1 / 3.3 ms | 3.6 / 3.5 / 3.6 |
| rich | 3.1 / 3.2 / 3.3 | 3.4 / 3.7 / 3.6 |
| **rich-always** | **3.7 / 7.8 / 10.6** | **4.0 / 11.8 / 11.4** |

`moving_KB` stays 0 for `glyph` and `rich` and lands at ~1.7 MB for
`rich-always`, matching the live pane. The invariant survives the move.

**The first rep is the terminal's warm-up, and it is not optional.** Look at
rep 1: `rich-always` reads 3.7 ms, barely apart from the hybrid, and the
instrument appears to have failed. It has not — a freshly launched kitty has
never been sent an image, and its cost climbs as its image store fills. Reps 2
and 3 then separate 2.4-3.2x with no overlap, and the byte columns are identical
across all three, which is what identifies this as a terminal-state effect rather
than a measurement fault. The in-process 25-frame warm-up cannot reach it; it is
not our state. A live pane never shows it because that terminal has been running
for hours. Hence `bench-sandbox.sh` defaults to **3 reps** where `make bench`
defaults to one, and says in its own output to read from rep 2.

Reproduced with the window both minimised and visible, so it is not occlusion
throttling — which was the standing worry about running the bench in a window
nobody is looking at, and is now answered.

A sandbox in which `rich-always` does *not* separate from `glyph` **by rep 2** is
lying about latency, and its latency columns are void — the byte columns survive
either way.

### 4. What gets kept

Two shapes, because they answer different questions and would spoil each other
if merged.

**Per run** — `--perf-log PATH` (or `READINGBUDDY_PERF_LOG`) writes one JSON
object per frame: mode, quality, moving, rect, pixel dims, per-stage µs, the
whole draw, bytes by class, and any RTT sample. This is where a single bad frame
is findable. It is written fresh per run and describes one session, so
concatenating two of them means nothing. Off by default — the recording calls
cost one relaxed atomic load. The ignored `wire_rate` sweep emits the same shape,
so one set of tools reads both.

**Across runs** — `--perf-history PATH` appends one TSV row per benched mode:
epoch, mode, fps, text and image MB/s, `moving_KB`, sends, trace, draw, RTT
percentiles, rect, tmux, whether the cover was real. Deliberately not JSON — this
file exists to be sorted, `cut`, plotted, or pasted into a spreadsheet, and the
header is written exactly once.

`make bench` wires both up under a gitignored `perf/`:

```
perf/20260724-210018-bench.jsonl   per-frame detail for this run
perf/20260724-210018-bench.txt     the summary tables, as printed
perf/history.tsv                   one row per mode per run, appended forever
```

`make bench-trend` prints the history as an aligned table. Nothing is committed:
the numbers are specific to your machine, terminal and font, so a checked-in
figure would be misleading — the trend only means something against your own
baseline.

One thing to know if you write a log path by hand: `cargo test` runs a test
binary from the **package** root, so a relative `--perf-log` under `make perf`
would land in `crates/tui/`. The Makefile passes absolute paths for that reason,
and `Recorder::open` creates missing parent directories rather than silently
writing nothing.

### 5. The cost sweeps (`make perf`)

Five ignored, release-only tests that report rather than assert. Run serially
(`--test-threads 1`) so they are not timing each other's contention — measured
in parallel, the pixel traces come out 20-30% slower than they really are.

| test | reports |
|---|---|
| `glyph_cost` | glyph trace per rect / glyph set / supersample |
| `glyph_wire_rate` | glyph bytes on the wire per rect |
| `raster_cost` | pixel trace per rect / quality tier |
| `frame_budget` | pixel trace + encode + payload split |
| `wire_rate` | payload against pixel budget, **real cover** |

The glyph pair exists so the two renderers can be compared without writing
throwaway instrumentation, and because they are *not* comparable line for line:
the glyph raster is still single-threaded where `raster.rs` splits across scoped
threads. That gap is the headroom any future glyph quality work would spend.

### 6. Offline gates

Deterministic, terminal-free, run by `cargo test`. They are what let the
expensive instruments stay manual:

| test | what it pins |
|---|---|
| `a_moving_book_sends_no_image_bytes_at_all` | the rule, as a zero |
| `a_moving_book_draws_glyphs_instead` | declining pixels still draws something |
| `parking_transmits_exactly_one_image` | one send, then silence |
| `resuming_motion_deletes_the_image_before_the_glyphs_land` | no ghost under the glyphs |
| `a_park_after_motion_still_transmits` | `hide_image` did not blind the settle detection |
| `the_hybrid_costs_no_more_than_glyphs_while_moving` | byte-for-byte equality with the glyph path |
| `a_glyph_spin_stays_inside_its_byte_budget` | the glyph baseline itself |
| `a_spinning_book_retransmits_well_below_the_tick_rate` | `rich-always` stays bad-but-survivable, so the calibration holds |

## The wire format is a policy question, because of one decoder

`ImageWire` (`render3d/kitty.rs`) picks between `o=z` and an uncompressed RGBA
payload, and `Caps::image_wire` carries the choice beside everything the probe
actually measured. It is the one setting in `Caps` that is **identification
rather than capability**, and that is forced: a terminal that dies decoding a
compressed image does not answer "no" to anything. It answers `OK` to the
graphics query, accepts the settle frame, and disappears — which from this side
is indistinguishable from the user quitting.

Two hosts get `Raw`: **Ghostty** (`TERM_PROGRAM=ghostty`, `xterm-ghostty`,
`GHOSTTY_*`) and **herdr** (`HERDR_ENV` / `HERDR_PANE_ID` / `HERDR_SOCKET_PATH`).
herdr is a multiplexer rather than a terminal and is in the list anyway, because
it **embeds libghostty** — `ghostty_terminal_vt_write` sits directly under its
pty reader in the crash report — so it owns the decode and inherits the bug. The
outer terminal being kitty changes nothing.

`--kitty-compress on|off` overrides the guess in both directions, and `--probe`
prints which way it went. Identification by name is exactly the sort of thing
that goes stale, so `on` is also how you check whether a fixed libghostty has
landed: it is the configuration that crashes.

**Raw costs bytes, so it costs pixels.** `RAW_MOTION_MAX_PX` and
`RAW_SETTLE_MAX_PX` are the zlib budgets divided by six — a smooth render goes
under one base64 byte per pixel through zlib against 5.33 raw, so a sixth of the
pixels puts the same number of bytes on the wire. The budget was always bytes
wearing a pixel count as its unit; dropping the compression alone would have
fired a 6.4 MB settle frame at a decoder just established as fragile about large
images. The settle frame is correspondingly softer on those two hosts — visibly
sharper than glyphs, not the full 1.2 MP.

**Still open, and the way to get the resolution back:** `t=t` temp-file
transmission. The payload becomes a *path*, so the full 1.2 MP costs ~100 bytes
on the wire and never meets the decompressor either. It needs a local terminal
(no ssh), a temp-file lifecycle, and a way to notice that a terminal ignored it —
`q=2` suppresses the error that would say so.

## Known bugs

**Ghostty 1.3.1 crashes on park — worked around, not fixed.** Pressing the
spin-freeze key (`space`)
while the book is on screen parks it, which transmits the one real settle
frame (`Quality::Settle`, up to `SETTLE_MAX_PX`, zlib-compressed per `o=z`,
chunked in `kitty::transmit`). In Ghostty 1.3.1 (ReleaseFast, macOS 26.5.2)
this reliably kills the whole terminal process. Ghostty's own crash reporter
(`~/.local/state/ghostty/crash/*.ghosttycrash`, a Sentry envelope with an
embedded Breakpad minidump — not macOS's DiagnosticReports) captured it;
symbolizing the minidump in lldb (`target create --core <name>.dmp`, `bt all`)
puts the fault on Ghostty's own read thread, not ours:

```
EXC_BREAKPOINT (code=1)
Io.Writer.unreachableRebase
compress.flate.Decompress.streamInner
compress.flate.Decompress.readVec
Io.Reader.appendRemaining
terminal.kitty.graphics_exec.loadAndAddImage
termio.stream_handler.StreamHandler.apcEnd
terminal.stream.Stream(...).nextNonUtf8
termio.Exec.ReadThread.threadMainPosix
```

A Zig `unreachable` inside Ghostty's `std.compress.flate` decompressor, while
it decodes the zlib-compressed image payload we transmit. Motion never
triggers it — the hybrid rule sends no pixels while spinning — so the bug is
invisible until the book is parked. Not reproduced under tmux+Ghostty or
other terminals in this session.

**The same crash then arrived from a second direction**: herdr 0.7.5 (an agent
multiplexer that embeds libghostty) dies identically while running inside
*kitty*, which rules out Ghostty-the-terminal and puts it squarely in the
embedded VT engine. Its report is the same six frames, under herdr's own pty
reader:

```
Io.Writer.unreachableRebase ← compress.flate.Decompress.streamInner
  ← compress.flate.Decompress.readVec ← Io.Reader.appendRemaining
  ← terminal.stream_terminal.Handler.apcEnd ← ghostty_terminal_vt_write
  ← herdr::pane::terminal::PaneTerminal::process_pty_bytes
```

Our payload is protocol-correct — `px.len() == s*v*4`, chunks inside the
documented 4096-byte ceiling — so the only end of this we own is whether that
decompressor is handed anything at all. It now is not: both hosts are
identified from the environment and get `ImageWire::Raw` (see above). Still
their bug, still unreported upstream; `--kitty-compress on` is how a fixed
version would be confirmed.

Two things this workaround does **not** claim. The settle frame on those hosts
is a sixth of the pixels, because the bytes have to stay where they were. And
herdr only *forwards* kitty graphics to its own outer terminal when its
`[experimental] kitty_graphics` is on — off by default — so not crashing is not
the same as the book appearing there.

## Still open

**A progressive turntable**, if the spin ever needs to be smooth *and*
full-resolution: quantize yaw to N steps, transmit each pose once under its own
image id, animate by rewriting only the placeholder foreground colour — pure
text, zero bytes, free after one revolution. Costs terminal memory (a buttery
27 s revolution wants ~540 poses, ~130 MB at low res), a first revolution as
expensive as today, and the pitch nod must be dropped or made commensurate with
the yaw period so the loop closes. The RTT instrument is now the right way to
find out whether a terminal tolerates N resident images.

**Glyph-path polish**, deferred deliberately. `render()` in `mod.rs` is still
single-threaded while `raster.rs` uses scoped threads, so there is 4–8x of
headroom sitting there to spend on a higher supersample, the edge-refine trick,
a perceptual (luma-weighted) glyph fit, and richer shading. The spin is what the
eye sees most of the time now, so this is where the next visual win is.

**Art direction.** The pixel path still uses the existing shader. The intended
look is a stylized soft-shade "product render": smooth non-physical gradients,
exaggerated fresnel rim so the silhouette pops, a gentle sheen across the cover,
and a soft contact shadow grounding the book. Clean and high-res, not photoreal,
keeping the existing key/fill/cream palette and its luma-clamped accent
discipline. The hybrid is what unlocks this: the settle frame is now transmitted
exactly once, so it can afford shading an animated frame never could.

Sixel is deliberately not implemented (kitty has no sixel, so it would be
untested dead code). `Caps` leaves room for it.
