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
