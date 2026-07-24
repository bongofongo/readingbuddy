# 2026-07-24 — Perf framework, then the hybrid renderer

The pixel renderer stops animating, and starts being measurable. `--render auto`
picks pixels again — the first time it has, since the previous session pinned it
to glyphs. Frontend-only; engine untouched.

The question that opened the session was "perf testing framework, reimagine rich
rendering, both, or start from ground zero?" Answer: both, in that order, and
definitely not ground zero.

## Why not ground zero

The bottleneck is not our code and not our protocol choice. The terminal
re-uploads a whole texture per frame; sixel is worse, `ratatui-image` is the same
kitty path, and a real GPU window abandons the tmux-pane premise. Every
"completely different" renderer hits the same wall. The existing machinery (caps
probe, DCS passthrough, Unicode placeholders, threaded raster, near-free AA,
teardown) is measured and sound.

## The trap that shaped everything

`docs/rich-renderer.md` already recorded it and it is worth restating: **none of
our own numbers showed the 7.1 MB/s disaster.** Trace + encode was a few ms,
`write`/`flush` never blocked, `--bench-rich` sustained 270 fps at 27 MB/s. Every
instrument sat on our side of the pty; the cost is paid inside the terminal.

So a perf framework built out of criterion benches on our own functions would
have re-walked exactly the road that failed twice. The framework had to measure
things nothing had measured: bytes *by class*, and the terminal's own backlog.

## Decisions locked

- **Both, ordered: instrument first, then the hybrid.** The instrument is what
  makes the hybrid a claim that can be checked rather than another tuning round.
- **Hybrid = glyphs in motion, pixels at rest** (the design note's option 1).
  Not the progressive turntable — deferred, and the RTT instrument is now the
  right way to find out whether terminals tolerate N resident images.
- **`auto` flipped to pixels** wherever `Caps::supports_pixels()`. The only
  argument against it was the animated cost, and the spin no longer sends images.
- **`RichAlways` kept deliberately as known-bad code**, reachable only by
  `--render rich-always`. An instrument that has never seen the failure it exists
  to catch is not evidence of anything, so the calibration arm stays.
- **Motion is told, not inferred** — `RenderParams::moving`, set by `App`. See
  gotchas.
- **Perf log is JSONL, opt-in flag** (user's call), plus an accumulating TSV for
  trends. No live HUD or debug overlay — user explicitly did not want one.
- **Glyph-path visual upgrades deferred** (user's call): threading `render()`,
  higher supersample, edge refine, perceptual glyph fit, richer shading. The
  headroom is measured and sitting there.
- **Art direction still deferred**, but the hybrid is what unlocks it: the settle
  frame is transmitted exactly once, so it can afford shading an animated frame
  never could.

## Technical gotchas

- **A parked book stops redrawing, so pose-comparison motion inference cannot
  see it park.** `App::tick` returns false when not spinning → `dirty` never set
  → no next draw → the crisp frame is never transmitted. This is *the* bug the
  hybrid would have shipped with. Fix: `RenderParams::moving: Option<bool>`, set
  from `App::animating()` (the same guard `tick` uses). `None` keeps the old
  inference for `--dump-frame`, the bench and unit tests.
- **Hiding the image must not clear `last_pose` or the placeholder table.** The
  hybrid hides on *every* motion frame; the original `drop_image` also reset
  `last_pose`, which would blind the settle detection permanently, and clearing
  placeholders re-allocates a `String` per cell per transition. Split into
  `hide_image` (cheap, per-transition) and `drop_image` (full reset, for leaving
  rich mode). `a_park_after_motion_still_transmits` guards it.
- **`cargo test` runs a test binary from the *package* root, not the workspace
  root.** `READINGBUDDY_PERF_LOG=perf/x.jsonl make perf` silently wrote nothing:
  the path resolved under `crates/tui/`, the dir did not exist, and `wire_rate`
  swallowed the error with `.ok()`. Fixed three ways — Makefile passes absolute
  paths, `Recorder::open` creates missing parents, and the sweep warns instead of
  skipping quietly.
- **Ignored timing tests must run with `--test-threads 1`.** Run in parallel they
  time each other's CPU contention: pixel motion trace read 2.05 ms parallel vs
  **0.87 ms** serial, settle 9.30 vs **6.02 ms** — 20-30% pessimistic. Byte
  figures are unaffected. `make perf` now forces serial.
- **Byte counting has to happen at the *writer*, not the call sites.** The glyph
  path's bytes are produced by ratatui's diff, which the app never sees — which
  is why "glyphs are cheap" had never actually been measured. Wrapping the
  `Stdout` handed to `CrosstermBackend` is the only way in.
- **`Terminal::new` queries the OS for a window size**, which fails in a test
  process. Use `Terminal::with_options` + `Viewport::Fixed` to measure real
  `CrosstermBackend` output offline.
- **`--bench-render` must reject a non-tty up front**, or raw mode fails with an
  opaque `Device not configured (os error 6)`.
- **Stage timings go through a thread-local scratchpad, not a parameter.**
  `ui::book::present_book` depends on disjoint *direct field* borrows of `App`;
  threading another `&mut` through `presenter_for` is a borrow-checker fight for
  nothing. Drawing is single-threaded, so a `Cell` suffices.
- **A procedural cover compresses ~6x better than a photograph.** Same 348x344
  frame: 17 KB procedural (`frame_budget`) vs 105 KB real (`wire_rate`). The
  bench warns loudly rather than quietly producing flattering numbers.
- `clap`'s `env` feature was already on, so `--perf-log` got
  `READINGBUDDY_PERF_LOG` for free.

## Numbers (release, M3, serial)

Glyph wire cost, 60 spin ticks, counted at the backend writer:

| terminal | B/frame | MB/s |
|---|---|---|
| 30x16 | 975 | 0.020 |
| 50x26 | 4,411 | 0.088 |
| 80x30 | 6,769 | 0.135 |
| 120x40 | 6,527 | 0.131 |

**Plateaus at ~0.13 MB/s** — past ~80 cols the object hits `BOOK_MAX_COLS`/
`BOOK_MAX_ROWS`, so a bigger pane grows the panel, not the book. The design
note's "~0.45 MB/s" was an unmeasured full-repaint upper bound; the real figure
is 3-5x lower because the diff only sends what moved. Note the rect column is the
**terminal**, not the object.

Glyph trace: 1.57 ms (50x26 octant), 0.78 ms (quadrant), 2.77 ms (octant ss4),
4.76 ms (120x40 octant). ~3% of the 50 ms budget at the book rect.

Pixel: motion 348x344 → 0.87 ms trace; settle 1000x988 → 6.02 ms trace,
8.12 ms including encode, 100-105 KB payload. **Paid once**, on park.

Hybrid vs glyph while spinning: image bytes **0**, text bytes **byte-for-byte
identical** (asserted, not estimated). Headline: **7.1 MB/s → 0 MB/s while
animating.**

## What shipped

- `crates/tui/src/perf.rs` — `Meter`/`CountingWriter` (bytes split Text vs
  Image), thread-local stage scratchpad, `FrameNote`/`FrameRecord`, JSONL sink,
  `percentile`. Inert unless a log path is set (one relaxed atomic load).
- `caps::rtt_probe` — times the terminal's reply to a wrapped `_Ga=q` (the
  terminal's own queue) and a bare `CSI c` (tmux's, answered locally) under load.
  **Bench-only**: it consumes stdin, which `EventStream` owns in a live session.
- `--bench-render <glyph|rich|rich-always|all>` — drives the real presenter
  through a real `Terminal::draw` over a fixed script (spin 100 → park 20 → spin
  60 → slide divider → spin 40 → park 20), so the throttle, placeholder cells and
  ratatui diff all count. `--bench-reps`, `--perf-log`, `--perf-history`.
- Hybrid routing in `present.rs`; `RenderMode::RichAlways`; `auto` → pixels.
- `make bench` / `make bench-trend` / `make perf`; gitignored `perf/` holding
  timestamped per-run JSONL + summary, plus an appended `history.tsv`.

## Verification

- `cargo test --workspace` green — 186 passed, 0 failed, 6 ignored (engine lib
  44, koreader_import 5, tui 137).
- `cargo clippy --workspace --all-targets` clean for the tui crate; one
  pre-existing `unnecessary_sort_by` in `engine/src/koreader.rs:129` untouched.
- **`--dump-frame 100x30` byte-identical** to pre-session output (42,906 bytes,
  `cmp` clean) — the glyph raster is untouched by construction.
- `wire_rate` reproduces the design note's table exactly (105 KB at the 120K
  budget, 347 KB / 7.1 MB/s at 420K), which is the check that a real cover is
  being picked up.
- `make perf` verified end to end, log lands in `perf/`.
- 8 new offline gates, the load-bearing ones being
  `a_moving_book_sends_no_image_bytes_at_all` (a zero, not a budget) and
  `the_hybrid_costs_no_more_than_glyphs_while_moving` (byte equality with the
  glyph path).

## Deferred

- **`make bench` has not been run against a real terminal.** It needs a real,
  *active* pane. The check that matters: `rich-always` must show a clearly worse
  kitty RTT p90 than `glyph` and `rich`. If all three look the same, the
  instrument is broken and the safety argument above is unverified.
- **Startup UX consequence, undecided.** The book spins by default, so a user
  sees glyphs and never the pixel render until they press `space`. The key bar
  advertises it, but the headline feature is invisible by default. Options: leave
  it, or auto-park after N seconds idle.
- Glyph-path visual work (see decisions).
- Art direction for the settle frame.
- Progressive turntable.

---

# Addendum — the bench meets a real terminal

The "Deferred" item above is now closed: `make bench` ran in a real active
kitty+tmux pane. **The calibration passes.** It took four runs to get there, and
each failed run found a real bug in the harness rather than in the renderer —
which is the useful part of this addendum.

## The result

Three reps, paced, warmed up, interleaved (`perf/20260724-215528`):

| mode | kitty p50 (3 reps) | kitty p90 (3 reps) |
|---|---|---|
| glyph | 3.3 / 3.3 / 3.3 ms | 3.7 / 3.6 / 3.5 |
| rich | 3.3 / 3.4 / 3.5 | 3.5 / 3.7 / 4.3 |
| **rich-always** | **8.3 / 7.2 / 8.3** | **9.1 / 9.1 / 10.6** |

Reproducible **2.2-2.5x separation, no overlap**, three times. `glyph` and `rich`
are indistinguishable from each other — correct, they draw identical frames while
moving. tmux stays at 0.0-0.1 ms throughout, so the load lands on kitty, not tmux:
the two-layer split earned its keep.

Byte columns, all three reps: `rich` moving-KB **0**, 2 sends. `rich-always`
1270-1374 KB, 80-81 sends. Deterministic, as predicted.

**So the hybrid's justification no longer rests on the byte invariant alone** —
two independent measurements agree, and the instrument has demonstrably seen the
failure it exists to catch.

## Harness bugs found by running it (all fixed)

1. **The bench free-ran instead of pacing to the tick** (223/305/1122 fps against
   the app's 20). The retransmit throttle keys off *pose*, so sends-per-frame is
   rate-invariant but sends-per-second is not: `rich-always` sent ~410/s where a
   real session sends ~7.4/s, a **~55x** overload. Every MB/s figure was inflated
   and the three modes ran at three different rates, so the columns were not even
   comparable to each other. Now paced with `app::TICK` and the same
   `MissedTickBehavior` as `app::run`; `--bench-free-run` keeps throughput mode
   and labels itself.
2. **The RTT probe fired in the idle gap.** It sat after `ticker.tick().await`
   and before `redraw`, so every sample landed when the terminal had already
   drained: all three modes reported an identical ~3.6 ms, which is just the
   quiescent round trip through tmux passthrough. Free-running had accidentally
   hidden this by leaving no gap to land in. Probe now fires immediately *after*
   the draw; its sample attaches to the next frame's record.
3. **The bench's own cover check disagreed with the renderer.** Paths are stored
   relative to the *data root* (`./database/images/x.jpg`), so
   `images_dir.join(stored)` yields `database/images/./database/images/x.jpg` and
   never exists — the bench warned "no cover image on disk" for books
   `Scene::resolve_cover` was loading fine. `resolve_cover` is now a free function
   and the single authority. **The earlier report's "procedural plate, ~4x
   optimistic" claim was unfounded.**
4. **`history.tsv` was appended to under a stale header.** `real_cover` (a bool)
   became `cover_px` (a width) and three runs' rows kept landing under the old
   column name — silent corruption of the one artefact whose purpose is
   comparison over time. A header mismatch now rotates the file to `.tsv.old`.
5. **A silent probe became the workload.** One run came back at **13.2 fps
   against a 20 fps tick** with every mode on glyphs: the pane had lost focus, no
   graphics replies arrived, and each probe waited out its full 250 ms deadline —
   30 probes per mode is 7.5 s of stall on a 12 s run. The byte columns looked
   perfectly ordinary while this happened. Deadline is now **40 ms** (inside the
   frame budget) and sampling gives up after two consecutive silent probes.
6. **`make bench --bench-reps 3` cannot work** — make eats its own argv, and the
   recipe hardcoded one rep. Now a make variable: `make bench BENCH_REPS=3`, plus
   `BENCH_ARGS` for anything else.

## Measurement gotchas

- **Warm-up is not optional.** The first 20 frames of a mode ran **40% slower**
  than the rest of its own run. 25 warm-up frames per mode, discarded.
- **Run order is a confound.** Modes used to run in blocks, so whichever went
  first absorbed the release build's IO tail — `glyph` came out worst on every
  timing metric, which is not believable for a path that draws the same frames
  `rich` does. Reps now loop *outside* modes, so `BENCH_REPS=3` interleaves.
- **Percentiles hid the actual signal.** `rich-always`'s samples are cleanly
  **bimodal** — 18 at ~3.6 ms (quiet) and 12 at 8-10 ms (retransmitting). A
  p50/p90 pair reports that as "3.6 / 9.7" and invites comparison against noise.
  When a latency result looks null, read the sorted samples from the JSONL before
  believing it.
- **Byte columns are deterministic at one rep; latency columns are not.** Re-run
  a surprising latency result at 3 reps before drawing any conclusion from it.
- A 180px OpenLibrary `-M` thumbnail upscaled into a ~358px render compresses
  nearly as well as a procedural plate (27 KB/send vs ~105 KB). The bench now
  picks the *widest* cover in the library and warns below 400px — "has a cover"
  was never the right question.

## Claims corrected

- "RTT can't resolve 0.19 MB/s, the hybrid rests on the byte invariant alone" —
  **wrong**. It resolves it cleanly once warm-up and ordering noise are removed.
- "glyph regressed to 9942 µs draw" — **noise**. The clean run has glyph at
  4814-5169 µs and rich at 4080-5416 µs, indistinguishable.
- "the bench measured a procedural cover, ~4x optimistic" — **wrong**, see bug 3.

## Verification

- `cargo test --workspace` green; `cargo clippy --workspace --all-targets` clean
  apart from the pre-existing `unnecessary_sort_by` in `engine/src/koreader.rs:129`.
- Two new gates: `a_history_file_with_stale_columns_is_rotated_not_appended_to`
  and the existing header/ragged-row check.
- `make bench BENCH_REPS=3` verified live in a real active pane — the table above.

## Deferred (updated)

- ~~`make bench` unverified against a real terminal~~ — **done, calibration passes.**
- `perf/history.tsv` still contains the 9 rows from the degenerate 13 fps run.
  The tells are `fps 13.0` and `rtt_kitty_p50 0`; worth deleting to keep the trend
  honest.
- Startup UX: the book spins by default, so the pixel render is invisible until
  `space`. Still undecided — leave it, or auto-park after N seconds idle.
- Glyph-path visual work; art direction for the settle frame; progressive
  turntable.
