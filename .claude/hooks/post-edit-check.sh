#!/bin/bash
#
# PostToolUse hook — after an edit, run the cheapest check for THAT file's
# package and print anything it says.
#
# Why this exists: without it, a type error surfaces at wrap-up, several edits
# after the one that caused it, when the context that would explain it is gone.
# The point is not to gate anything — it is to put the error in the same turn as
# its cause.
#
# Three rules, all deliberate:
#
#   * It NEVER blocks. Always exits 0. A multi-file refactor passes through
#     legitimately-broken intermediate states, and a hook that rejected those
#     would make the refactor impossible rather than safe. `make check` and the
#     cargo-tester / web-checker agents are the gate; this is a smoke alarm.
#   * It is SCOPED to one package. `cargo check --workspace` after every edit
#     would cost more than it saves; `-p <the one crate>` is usually seconds
#     once warm.
#   * It is BOUNDED. A cold build compiles vendored Lua and libsqlite3, and a
#     hook that hangs for four minutes after a one-line edit is worse than no
#     hook. If it does not finish in time it says so and gets out of the way.

set -uo pipefail

BUDGET=45   # seconds; a check slower than this is not inner-loop feedback

payload=$(cat)

# Extract the edited path without requiring jq — it is not on every machine, and
# an absent parser must degrade to silence rather than to a stack trace.
file=$(printf '%s' "$payload" \
  | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
  | head -1)

[ -z "$file" ] && exit 0
[ -f "$file" ] || exit 0

root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
rel="${file#"$root"/}"

# `timeout` is coreutils; on macOS it is gtimeout, and on neither is it
# guaranteed. Without one, run bare rather than not at all.
if command -v timeout >/dev/null 2>&1;      then TO="timeout $BUDGET"
elif command -v gtimeout >/dev/null 2>&1;   then TO="gtimeout $BUDGET"
else                                             TO=""
fi

run() {  # run <label> <dir> <cmd...>
  local label="$1" dir="$2"; shift 2
  local out status
  out=$(cd "$dir" && $TO "$@" 2>&1); status=$?
  if [ $status -eq 124 ]; then
    echo "[$label] still building after ${BUDGET}s — skipped, not a failure."
  elif [ $status -ne 0 ]; then
    echo "[$label] FAILED — the edit that just landed may be the cause:"
    echo "$out" | tail -40
  fi
  return 0
}

case "$rel" in
  # ---- Rust ----------------------------------------------------------------
  crates/engine/*.rs|crates/engine/*/*.rs|crates/engine/**/*.rs)
      run "cargo check -p readingbuddy"       "$root" cargo check -p readingbuddy       --locked ;;
  crates/cli/*)     [ "${rel##*.}" = rs ] && run "cargo check -p readingbuddy-cli"  "$root" cargo check -p readingbuddy-cli  --locked ;;
  crates/tui/*)     [ "${rel##*.}" = rs ] && run "cargo check -p readingbuddy-tui"  "$root" cargo check -p readingbuddy-tui  --locked ;;
  crates/api/*)     [ "${rel##*.}" = rs ] && run "cargo check -p readingbuddy-api"  "$root" cargo check -p readingbuddy-api  --locked ;;
  crates/daemon/*)  [ "${rel##*.}" = rs ] && run "cargo check -p readingbuddyd"     "$root" cargo check -p readingbuddyd     --locked ;;
  crates/corpus/*)  [ "${rel##*.}" = rs ] && run "cargo check -p corpus"            "$root" cargo check -p corpus            --locked ;;

  # A migration is not checked by the compiler at all — say the one thing that
  # actually goes wrong here, since CI refuses it much later.
  crates/engine/migrations/*.sql)
      echo "[migrations] Reminder: never edit an APPLIED migration — CI's migrations job refuses a modified, deleted or renamed file."
      echo "[migrations] If this is new, confirm the number is unclaimed: 0011=item 20, 0012=item 21, 0013=item 23." ;;

  # ---- Frontend ------------------------------------------------------------
  gui/src-tauri/*)
      [ "${rel##*.}" = rs ] && run "cargo check -p readingbuddy-gui" "$root" cargo check -p readingbuddy-gui --locked ;;
  gui/*.svelte|gui/*.ts|gui/*/*.svelte|gui/*/*.ts|gui/**/*.svelte|gui/**/*.ts)
      if [ -d "$root/gui/node_modules" ]; then
        run "svelte-check" "$root/gui" pnpm exec svelte-check --threshold error --output human
      fi ;;
esac

exit 0
