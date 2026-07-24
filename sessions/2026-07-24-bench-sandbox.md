# 2026-07-24 — running the bench in a sandbox

Question that opened it: is there a better way to sandbox the program for perf
testing? Three pains named: measurement noise, state isolation, and "an agent
can never run `make bench` itself". Terminal hijack was explicitly *not* one —
handing over the pane was never the complaint.

## Decisions locked

- **The sandbox is a second entry point, not a replacement.** `make bench` in a
  real pane stays the instrument for a verdict you act on, because that is the
  terminal the answer is about; `make bench-box` is the routine, unattended
  regression check. The `env` column enforces that they never trend together.
- **Load thresholds are relative to core count** (`ncpu/2` abort, `ncpu/4` warn),
  not the absolute 2.0 first proposed. This machine idles around 2.6, so a fixed
  2.0 made the target unrunnable on the machine it was written for.
- **`bench-sandbox.sh` defaults to 3 reps** where `make bench` defaults to 1, and
  `make bench-box` reads `BENCH_BOX_REPS`, *not* `BENCH_REPS` — otherwise the
  existing `BENCH_REPS ?= 1` would have silently reinstated the cold-terminal
  reading described below.
- **No `taskpolicy`.** See gotchas; the comment saying why lives in the script so
  it is not re-added.
- Nothing about the live harness changed. The load gate and the new `load1`
  column are sandbox-independent and would work on `make bench` unchanged.

## The finding: a fresh terminal is a different instrument

Calibrating the box against `rich-always` (the deliberately-kept known-bad mode)
did **not** simply reproduce the live result. 3 reps, twice:

| mode | kitty p50 (rep 1 / 2 / 3) |
|---|---|
| glyph | 3.1 / 3.1 / 3.3 ms |
| rich | 3.1 / 3.2 / 3.3 |
| **rich-always** | **3.7 / 7.8 / 10.6** |

Rep 1 shows no separation at all — the instrument looks broken. It is not: a
freshly launched kitty has **never been sent an image**, and its cost climbs as
its image store fills. Reps 2 and 3 then separate 2.4-3.2x with no overlap.

What identifies it as terminal state rather than a measurement fault: the byte
columns are *identical* across all three reps. Only latency moves.

- Reproduced with the window both minimised and visible ⇒ **not** occlusion
  throttling, which had been the standing worry about benching in a window nobody
  is looking at. That worry is now answered.
- The in-process 25-frame warm-up cannot reach this. It is not our state.
- A live pane never shows it because that kitty has been running for hours —
  which is also the honest argument that the live pane is the *more* faithful
  instrument, not the less.

Acceptance criterion for the box is therefore "`rich-always` separates from
`glyph` **by rep 2**". A box where it doesn't has void latency columns; the byte
columns survive either way.

## Technical gotchas

- **`env -u TMUX -u TMUX_PANE` is load-bearing.** Inherited into a fresh kitty,
  they make the app DCS-wrap its capability queries; kitty — not being tmux —
  swallows them whole, the probe reports no graphics, and every mode falls back
  to glyphs. The run then looks entirely normal and is a fiction. Verified
  failure, hit on the first attempt.
- **stdout must stay a tty all the way down.** `probe_verbose` self-skips when it
  isn't one (`caps.rs`), and a skipped probe silently downgrades every mode. The
  bench summary goes to *stderr* precisely so it can be captured without
  redirecting stdout. `script -q FILE cmd` is the escape hatch when stdout itself
  must be captured — it relays queries and replies to the real terminal.
- **`taskpolicy -c` is a QoS *clamp* — it can only lower.** `user-interactive` is
  not a valid value (exits 64, "Could not parse as a QoS clamp"); `utility` is
  valid and would push the work *onto* the efficiency cores, the exact opposite
  of a P-core pin. macOS offers no unprivileged way up. Keeping the machine idle
  is the whole of the scheduling story.
- **Discarding the launcher's stderr turned a failed launch into a mystery.** The
  first version had `"${launch[@]}" 2>/dev/null || true`; the taskpolicy failure
  surfaced only as "the run died early", with no perf files and nothing to go on.
  kitty's stderr is now captured, printed on failure, and saved beside the perf
  log. Silence must not be able to look like a clean run.
- A private tmux server needs **both** `-L <socket>` and `-f /dev/null` — the
  socket keeps it off the user's server, the empty config keeps it from reading
  their `tmux.conf`. The probe writes `allow-passthrough` to a pane either way,
  so this is what keeps it off theirs.
- `database/` is ~250 KB, so state isolation is a `cp -R`, not a plan. A *copy*
  and not a symlink: the bench opens the DB read-write and runs migrations. The
  real covers must come along — a procedural or thumbnail cover compresses
  several times better and flatters every image column.

## Changes

- `scripts/bench-sandbox.sh` (new) + `make bench-box`: kitty (pinned font/size,
  minimised by default, `START_AS=normal` to watch it) → own tmux server →
  `--bench-render` against a copied data root. Knobs: `BENCH_REPS`, `BENCH_MODE`,
  `LOAD_MAX`/`LOAD_WARN`, `FORCE`, `KEEP_BOX`, `COLS`/`ROWS`, `START_AS`.
- `crates/tui/src/main.rs`: `--bench-env NAME` (default `live`) and two new
  history columns, `env` and `load1` (1-minute load via `libc::getloadavg`,
  sampled after the run). The existing header-rotation logic handled the schema
  bump on its own — old rows kept at `history.tsv.old`.
- `docs/rich-renderer.md`: new "The sandbox" section under the measurement traps,
  carrying the calibration table and the cold-terminal finding.
- `CLAUDE.md`: `make bench-box` entry with the load-bearing details.
- `crates/engine/src/koreader.rs`: pre-existing `clippy::unnecessary_sort_by` on
  the determinism sort, rewritten to `sort_by_key`. Untouched since
  `70da359`, unrelated to this session, cleared so the wrap-up gate is honest.
  The golden import snapshots are what verify the ordering is unchanged.

## Verification

- `cargo test --workspace`: 187 passed, 0 failed, 6 ignored. Clippy clean,
  `cargo fmt --check` clean.
- `the_history_file_gets_a_header_once_and_a_row_per_run` extended to assert both
  `env=live` and `env=box` land in their rows.
- Sandbox exercised end to end four times: probe-only (bare kitty, then kitty +
  private tmux — the latter reporting `passthrough: EnabledByUs`, `kitty
  graphics: true`, cell 20x38), then two full 3-rep `all` runs, minimised and
  visible. `moving_KB` stayed 0 for `glyph` and `rich` and ~1.7 MB for
  `rich-always` throughout, matching the live pane: the invariant survives the
  move.
- Load gate exercised in both directions — refused at load 3.4 under the first
  absolute threshold, warned-and-ran at 2.6-3.1 under the relative one.

## Deferred / unproven

- **The noise-reduction claim is not demonstrated.** Box `glyph` p90 came back
  3.6/3.5/3.6 against live 3.7/3.6/3.5 — indistinguishable. The box run threw
  19-34 ms maxima, but ran at load 3.07, so that is confounded rather than
  evidence either way. A matched box-vs-live variance comparison on a quiet
  machine is still owed.
- Live-pane runs still default to 1 rep and do not warn about elevated load; the
  gate lives in the sandbox script only.
- macOS + kitty assumptions throughout (`sysctl vm.loadavg`, `--start-as`,
  `macos_quit_when_last_window_closed`). No Linux path attempted.
