#!/usr/bin/env bash
#
# Run `--bench-render` inside a disposable terminal instead of the one you are
# working in.
#
# The bench needs three things a normal shell cannot give it: a real tty, a
# terminal that answers kitty graphics queries, and — inside tmux — the *active*
# pane. That is why it has always had to take over your session, and why it
# could not run unattended. This script builds a throwaway one:
#
#   kitty (own process, pinned font+size, minimised)
#     └── tmux (own server on its own socket, own empty config)
#           └── readingbuddy-tui --bench-render all
#
# What that buys, in the order the problems bite:
#
#   * Nothing of yours is touched. The tmux server is `-L rbbench -f /dev/null`,
#     so `allow-passthrough` is set on a pane in a server that exists for four
#     minutes and is then killed; your tmux.conf is never read and your panes
#     are never written. The data root is a *copy* of database/, so the bench
#     opens a throwaway SQLite file.
#   * The pane is the active one by construction. A background pane gets no
#     replies from tmux and the latency columns come back empty — the single
#     most common way to get a meaningless run.
#   * Window size and font are pinned, so cols/rows and the cell pixel size stop
#     drifting between runs. Two rows in history.tsv currently differ by 70x37 vs
#     78x47, which is enough to move every byte column on its own.
#
# It does NOT make the numbers comparable to a live-pane run — different font,
# different size, no neighbours. Rows are tagged `env=box` for exactly that
# reason; trend box against box.
#
# Usage:  scripts/bench-sandbox.sh [-- extra args for readingbuddy-tui]
# Knobs (environment):
#   BENCH_REPS=3      reps, passed through (modes interleave; rep 1 is the
#                     terminal's own warm-up — read the latency table from 2 on)
#   BENCH_MODE=all    glyph | rich | rich-always | all
#   LOAD_MAX=ncpu/2   refuse to start above this 1-minute load average
#   FORCE=1           run anyway
#   KEEP_BOX=1        do not delete the temporary data root (for debugging)
#   COLS=100 ROWS=40  sandbox window size in cells
#   START_AS=normal   show the sandbox window instead of minimising it
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

# Three reps, where `make bench` defaults to one. A sandbox terminal is *cold*:
# measured twice, `rich-always` came back at 3.7 ms p50 in rep 1 and 7.8-10.6 ms
# in reps 2 and 3 of the same run, with the byte columns identical throughout.
# The in-process warm-up cannot reach this — it is kitty's own image store
# filling up, and rep 1 is the only rep that sees it empty. A live pane never
# shows it because that terminal has been running for hours. So the first rep of
# a box run is warm-up for the *terminal*, and one rep would report a
# rich-always that looks nearly as cheap as the hybrid.
BENCH_REPS=${BENCH_REPS:-3}
BENCH_MODE=${BENCH_MODE:-all}
# Thresholds relative to the core count, not absolute: the bench is one drawing
# thread plus a terminal, so what matters is whether a couple of cores are free,
# and a fixed number would be either unusable on a laptop with a busy background
# (this machine idles around 2.5) or waved through on a 32-core box.
ncpu=$(sysctl -n hw.ncpu 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 8)
LOAD_MAX=${LOAD_MAX:-$(awk -v n="$ncpu" 'BEGIN { printf "%.2f", n / 2 }')}
LOAD_WARN=${LOAD_WARN:-$(awk -v n="$ncpu" 'BEGIN { printf "%.2f", n / 4 }')}
COLS=${COLS:-100}
ROWS=${ROWS:-40}
SOCKET=rbbench
extra=("$@")

for dep in kitty tmux; do
  command -v "$dep" >/dev/null || {
    echo "bench-sandbox: needs $dep on PATH" >&2
    exit 1
  }
done
[ -d database ] || {
  echo "bench-sandbox: no database/ to copy — the bench needs a library with a real cover" >&2
  exit 1
}

# The load gate. Contention does not show up as an error, it shows up as a
# plausible-looking latency column, which is worse — the trace timings in
# history.tsv already swing 3x within a single run.
load=$(sysctl -n vm.loadavg | tr -d '{}' | awk '{print $1}')
if awk -v l="$load" -v m="$LOAD_MAX" 'BEGIN { exit !(l > m) }'; then
  if [ "${FORCE:-0}" = 1 ]; then
    echo "bench-sandbox: load $load > $LOAD_MAX — running anyway (FORCE=1); latency columns are suspect"
  else
    echo "bench-sandbox: 1-minute load is $load, above LOAD_MAX=$LOAD_MAX ($ncpu cores)." >&2
    echo "  Wait for the machine to settle, or re-run with FORCE=1 and distrust the latency columns." >&2
    exit 1
  fi
elif awk -v l="$load" -v w="$LOAD_WARN" 'BEGIN { exit !(l > w) }'; then
  echo "bench-sandbox: load $load is above $LOAD_WARN — usable, but re-run a surprising latency column."
fi
# Recorded per row too, so a strange result stays diagnosable after the fact.

# Build before entering the sandbox: a build finishing inside the run is an IO
# and CPU tail that lands on whichever mode goes first.
echo "bench-sandbox: building release binary…"
cargo build --release -p readingbuddy-tui
bin="$root/target/release/readingbuddy-tui"

stamp=$(date +%Y%m%d-%H%M%S)
perf_dir="$root/perf"
mkdir -p "$perf_dir"
log="$perf_dir/$stamp-box.jsonl"
summary="$perf_dir/$stamp-box.txt"
history="$perf_dir/history.tsv"

box=$(mktemp -d "${TMPDIR:-/tmp}/rbbench.XXXXXX")
status_file="$box/status"
cleanup() {
  tmux -L "$SOCKET" kill-server 2>/dev/null || true
  if [ "${KEEP_BOX:-0}" = 1 ]; then
    echo "bench-sandbox: data root kept at $box"
  else
    rm -rf "$box"
  fi
}
trap cleanup EXIT

# A copy, not a symlink: the bench opens the DB read-write and runs migrations.
# database/ is a couple of hundred KB, so this costs nothing, and the covers
# come with it — a procedural or thumbnail cover compresses several times better
# and would flatter every image byte column.
cp -R database "$box/database"
tmux -L "$SOCKET" kill-server 2>/dev/null || true

# stdout must stay a tty all the way down: the capability probe self-skips when
# it is not one (caps.rs), and a skipped probe silently downgrades every mode to
# glyphs. The summary is written to stderr precisely so it can be captured
# without touching stdout — that is the only redirection here.
inner="cd '$root' && '$bin' \
  --data-dir '$box' \
  --bench-render '$BENCH_MODE' --bench-reps '$BENCH_REPS' --bench-env box \
  --perf-log '$log' --perf-history '$history'"
for arg in ${extra[@]+"${extra[@]}"}; do
  inner+=" '$arg'"
done
inner+=" 2> '$summary'; echo \$? > '$status_file'"

echo "bench-sandbox: ${BENCH_MODE} x${BENCH_REPS} in a disposable ${COLS}x${ROWS} kitty…"

# `env -u TMUX -u TMUX_PANE` is load-bearing. Inherited, they make the app wrap
# its queries in tmux DCS passthrough, which the fresh kitty — not being tmux —
# swallows whole: the probe then reports no graphics at all and the run is a
# glyph-only fiction. Verified failure, not a precaution.
#
# There is deliberately no `taskpolicy` here. Its `-c` is a QoS *clamp*: it can
# only lower the class, never raise it, so the one thing it could achieve is
# pushing the bench onto the efficiency cores — the opposite of what a P-core
# pin would be. macOS offers no unprivileged way up. Keeping the machine idle
# (the load gate above) is the whole of the scheduling story.
launch=(
  env -u TMUX -u TMUX_PANE
  kitty
  --single-instance=no
  "--start-as=${START_AS:-minimized}"
  --title readingbuddy-bench
  -o "initial_window_width=${COLS}c"
  -o "initial_window_height=${ROWS}c"
  -o remember_window_size=no
  -o font_family=Menlo
  -o font_size=12
  -o macos_quit_when_last_window_closed=yes
  -o confirm_os_window_close=0
  -o scrollback_lines=0
  tmux -L "$SOCKET" -f /dev/null new-session -x "$COLS" -y "$ROWS" "$inner"
)
# kitty's own stderr is kept, not discarded. Throwing it away once turned a
# failed launch into "the run died early" with nothing to go on — the sandbox
# must not be able to fail silently, since silence here is indistinguishable
# from a clean run that produced no numbers.
launch_err="$box/kitty.err"
"${launch[@]}" 2>"$launch_err" || true

status=$(cat "$status_file" 2>/dev/null || echo 1)
if [ -s "$summary" ]; then
  cat "$summary"
else
  echo "bench-sandbox: the sandbox produced no summary — the run died early." >&2
  if [ -s "$launch_err" ]; then
    echo "  the terminal said:" >&2
    sed 's/^/  | /' "$launch_err" >&2
  fi
  cp "$launch_err" "$perf_dir/$stamp-box.launch-err" 2>/dev/null || true
fi
echo
if [ "$BENCH_REPS" -gt 1 ]; then
  echo "Read the latency table from rep 2 on: rep 1 ran against a terminal that had"
  echo "never been sent an image, and reads optimistically for the pixel modes."
fi
echo "per-frame log : $log"
echo "summary       : $summary"
echo "trend         : $history   (rows tagged env=box)"
exit "$status"
