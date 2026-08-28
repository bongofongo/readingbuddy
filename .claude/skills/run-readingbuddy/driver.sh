#!/usr/bin/env bash
# readingbuddy run-driver — launch and drive any of the four binaries.
#
# This is agent tooling, not product surface. It exists because three of the
# four frontends cannot be checked by reading them: the TUI needs a tty, the
# GUI needs a Wayland surface and a webview, and the daemon needs a socket
# peer. Each subcommand below leaves something on stdout or on disk that can
# be looked at.
#
#   ./driver.sh build [gui|tui|cli|api|all]
#   ./driver.sh cli <args...>              # CLI against $RB_DATA
#   ./driver.sh api <method> [params-json] # one JSON line to the daemon
#   ./driver.sh tui [tmux-keys...]         # TUI in a private tmux, pane -> stdout
#   ./driver.sh tui-frame [WxH] [out.png]  # headless 3D frame, no tty needed
#   ./driver.sh gui                        # launch the Tauri GUI, screenshot it
#   ./driver.sh shot [out.png]             # screenshot the running GUI window
#   ./driver.sh stop                       # stop everything the driver started
#
# Env:
#   RB_DATA  library to run against   (default $REPO/dev-data)
#   RB_OUT   where artifacts land     (default /tmp/rb-run)
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RB_DATA="${RB_DATA:-$REPO/dev-data}"
RB_OUT="${RB_OUT:-/tmp/rb-run}"
TMUX_SRV=rbrun                       # our own tmux server; never touches the user's
VITE_PORT=5173                       # tauri.conf.json devUrl — not configurable here
mkdir -p "$RB_OUT"

say()  { printf '\033[36m>>\033[0m %s\n' "$*" >&2; }

# spawn <logfile> <cmd...> — start a long-lived service with NO tie to this
# script. `setsid -f` forks, so the service is reparented to init: bash has no
# child left to wait on, and — the part that cost an afternoon — nothing is
# holding the caller's stdout pipe open. Without it `driver.sh gui | tail`
# hangs for ever with the window already up and the screenshot already taken,
# because `tail` prints only at EOF and vite kept the pipe alive.
spawn() {
  local log="$1"; shift
  setsid -f "$@" </dev/null > "$log" 2>&1
}
die()  { printf '\033[31m!!\033[0m %s\n' "$*" >&2; exit 1; }

need_data() {
  [ -f "$RB_DATA/database/app.db" ] || die "no library at $RB_DATA — run: make -C $REPO dev-db"
  # cover_path is stored as images_dir.join(name), so a relative root yields a
  # relative path that a webview cannot resolve. Absolutise before handing it on.
  RB_DATA="$(cd "$RB_DATA" && pwd)"
}

# --- build ------------------------------------------------------------------

cmd_build() {
  case "${1:-all}" in
    gui) (cd "$REPO" && cargo build -p readingbuddy-gui) ;;
    tui) (cd "$REPO" && cargo build -p readingbuddy-tui) ;;
    cli) (cd "$REPO" && cargo build -p readingbuddy-cli) ;;
    api) (cd "$REPO" && cargo build -p readingbuddyd) ;;
    all) (cd "$REPO" && cargo build -p readingbuddy-gui -p readingbuddy-tui \
                                    -p readingbuddy-cli -p readingbuddyd) ;;
    *)   die "build: unknown target '$1'" ;;
  esac
}

bin() {  # bin <name> <package> — build on demand, print the path
  local b="$REPO/target/debug/$1"
  [ -x "$b" ] || { say "building $2"; (cd "$REPO" && cargo build -p "$2") || die "build failed"; }
  echo "$b"
}

# --- cli --------------------------------------------------------------------

cmd_cli() {
  need_data
  "$(bin readingbuddy readingbuddy-cli)" --data-dir "$RB_DATA" "$@"
}

# --- daemon / api -----------------------------------------------------------

rb_send() {  # rb_send <sock> <line> — write one line, read one line, close
  python3 - "$1" "$2" <<'PY'
import socket, sys
sock, line = sys.argv[1], sys.argv[2]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(30)
s.connect(sock)
# The newline is the frame delimiter and $(...) strips trailing ones, so it is
# re-added here rather than trusted from the caller — without it the daemon
# holds the connection open waiting for the rest of the line.
s.sendall((line.rstrip("\n") + "\n").encode())
buf = b""
while not buf.endswith(b"\n"):          # one JSON object per line, id-matched
    chunk = s.recv(65536)
    if not chunk:
        break
    buf += chunk
s.close()
sys.stdout.write(buf.decode())
PY
}

daemon_up() {
  local sock="$RB_DATA/readingbuddyd.sock"
  # A live daemon owns the socket; a SIGKILLed one leaves a corpse that the
  # daemon's own bind() removes. Connecting is how we tell them apart.
  if [ -S "$sock" ] && python3 -c '
import socket,sys
s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); s.settimeout(2)
try: s.connect(sys.argv[1])
except OSError: sys.exit(1)
' "$sock" 2>/dev/null; then return 0; fi
  local d; d="$(bin readingbuddyd readingbuddyd)"
  say "starting daemon on $sock"
  spawn "$RB_OUT/daemon.log" "$d" --data-dir "$RB_DATA"
  for _ in $(seq 1 60); do
    [ -S "$sock" ] && { pgrep -f 'debug/[r]eadingbuddyd' > "$RB_OUT/daemon.pid"; return 0; }
    sleep 0.25
  done
  die "daemon never bound its socket — see $RB_OUT/daemon.log"
}

cmd_api() {
  need_data
  local method="${1:?usage: driver.sh api <method> [params-json]}"
  local params="${2:-{\}}"
  daemon_up
  local line; line="$(printf '{"id":1,"request":{"method":"%s","params":%s}}\n' "$method" "$params")"
  # nc -U is the wrong tool here: it does not close the connection on stdin EOF,
  # so it sits on the reply forever unless something downstream SIGPIPEs it.
  rb_send "$RB_DATA/readingbuddyd.sock" "$line" \
    | python3 -c 'import json,sys; [print(json.dumps(json.loads(l), indent=2)) for l in sys.stdin if l.strip()]'
}

# --- tui --------------------------------------------------------------------

cmd_tui() {
  need_data
  local t; t="$(bin readingbuddy-tui readingbuddy-tui)"
  tmux -L "$TMUX_SRV" kill-server 2>/dev/null
  # -f /dev/null: the user's ~/.tmux.conf is not ours to inherit. Fixed 120x40
  # so a captured pane is comparable between runs.
  tmux -L "$TMUX_SRV" -f /dev/null new-session -d -s rb -x 120 -y 40 \
    "TERM=xterm-256color '$t' --data-dir '$RB_DATA'" || die "tmux failed to start the TUI"
  sleep 3
  for k in "$@"; do tmux -L "$TMUX_SRV" send-keys -t rb "$k"; sleep 1.2; done
  sleep 1
  tmux -L "$TMUX_SRV" capture-pane -p -t rb
  tmux -L "$TMUX_SRV" send-keys -t rb q 2>/dev/null
  sleep 0.5
  tmux -L "$TMUX_SRV" kill-server 2>/dev/null
  return 0
}

cmd_tui_frame() {
  need_data
  local size="${1:-100x30}" png="${2:-$RB_OUT/tui-frame.png}"
  "$(bin readingbuddy-tui readingbuddy-tui)" --data-dir "$RB_DATA" \
      --dump-frame "$size" --dump-png "$png" > "$RB_OUT/tui-frame.ansi" \
    || die "dump-frame failed"
  say "ansi: $RB_OUT/tui-frame.ansi   png: $png"
}

# --- gui --------------------------------------------------------------------

vite_up() {
  if ss -ltn 2>/dev/null | grep -q ":$VITE_PORT\b"; then
    say "reusing the dev server already on :$VITE_PORT"
    return 0
  fi
  [ -d "$REPO/gui/node_modules" ] || die "no gui/node_modules — run: cd $REPO/gui && pnpm install"
  say "starting vite on :$VITE_PORT"
  # node_modules/.bin/vite directly: pnpm is only needed to *install*, and it is
  # not always on a non-interactive PATH even when it is installed.
  (cd "$REPO/gui" && spawn "$RB_OUT/vite.log" ./node_modules/.bin/vite dev)
  for _ in $(seq 1 80); do
    ss -ltn 2>/dev/null | grep -q ":$VITE_PORT\b" && {
      pgrep -f 'vite/bin/[v]ite.js' > "$RB_OUT/vite.pid"; return 0; }
    sleep 0.25
  done
  die "vite never came up — see $RB_OUT/vite.log"
}

gui_window() {  # prints "X,Y WxH" for the GUI window, empty if there is none
  command -v hyprctl >/dev/null || return 1
  hyprctl clients -j 2>/dev/null | python3 -c '
import json,sys
for c in json.load(sys.stdin):
    if c["class"] == "readingbuddy-gui":
        print("%d,%d %dx%d" % (c["at"][0], c["at"][1], c["size"][0], c["size"][1])); break
'
}

cmd_gui() {
  need_data
  # A second instance is a second window against the same SQLite file, and the
  # two are indistinguishable to `shot`. Reuse whatever is already mapped.
  if [ -n "$(gui_window)" ]; then
    say "a readingbuddy-gui window is already mapped — screenshotting that one"
    cmd_shot; return 0
  fi
  local g; g="$(bin readingbuddy-gui readingbuddy-gui)"
  vite_up
  say "launching the GUI"
  # Three env vars and one unset, all load-bearing on this machine — see the
  # Gotchas section of SKILL.md. Without them the process dies at once with
  # "Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display."
  ( unset DISPLAY
    export WEBKIT_DISABLE_DMABUF_RENDERER=1 \
           WEBKIT_DISABLE_COMPOSITING_MODE=1 \
           GDK_BACKEND=wayland \
           READINGBUDDY_DATA_DIR="$RB_DATA"
    spawn "$RB_OUT/gui.log" "$g" )
  for _ in $(seq 1 120); do
    [ -n "$(gui_window)" ] && {
      pgrep -f 'debug/readingbuddy-[g]ui' > "$RB_OUT/gui.pid"
      sleep 1.5; cmd_shot; return 0; }
    sleep 0.5
  done
  die "no GUI window appeared — see $RB_OUT/gui.log"
}

cmd_shot() {
  local out="${1:-$RB_OUT/gui.png}" geo
  command -v grim >/dev/null || die "grim is not installed — no way to screenshot a Wayland surface"
  geo="$(gui_window)"
  [ -n "$geo" ] || die "no readingbuddy-gui window is mapped — run: driver.sh gui"
  grim -g "$geo" "$out" || die "grim failed"
  say "screenshot: $out  ($geo)"
}

# --- stop -------------------------------------------------------------------

cmd_stop() {
  for p in gui vite daemon; do
    [ -f "$RB_OUT/$p.pid" ] || continue
    # One pid per line, and a pid file can hold more than one: pgrep matches
    # every instance, including a daemon left over from an earlier session.
    # `kill "$(cat …)"` with the quotes on passes both lines as one argument
    # and silently fails, which is how a stray daemon survived a `stop`.
    while read -r pid; do
      [ -n "$pid" ] && kill "$pid" 2>/dev/null && say "stopped $p ($pid)"
    done < "$RB_OUT/$p.pid"
    rm -f "$RB_OUT/$p.pid"
  done
  tmux -L "$TMUX_SRV" kill-server 2>/dev/null && say "stopped tmux server $TMUX_SRV"
  rm -f "$RB_DATA/readingbuddyd.sock"
  return 0
}

# ----------------------------------------------------------------------------

case "${1:-}" in
  build)     shift; cmd_build "$@" ;;
  cli)       shift; cmd_cli "$@" ;;
  api)       shift; cmd_api "$@" ;;
  tui)       shift; cmd_tui "$@" ;;
  tui-frame) shift; cmd_tui_frame "$@" ;;
  gui)       shift; cmd_gui "$@" ;;
  shot)      shift; cmd_shot "$@" ;;
  stop)      shift; cmd_stop "$@" ;;
  *) sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
