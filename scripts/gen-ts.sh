#!/usr/bin/env bash
#
# Generate the GUI's TypeScript from the API crate's own types.
#
# `make ts` writes into the tree; `make ts-check` writes into a temp dir and
# diffs, so a DTO change that skipped the generator fails the gate instead of
# shipping. Both call this script — the generation must be one implementation,
# or the check is checking something other than what `make ts` produces.
set -euo pipefail

out="${1:?usage: gen-ts.sh <output-dir>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$out"

# Build BEFORE deleting anything. ts-rs APPENDS, so the old file has to go — but
# deleting it first means a compile error leaves no bindings at all, and `make ts`
# then destroys the committed copy on its way to failing. Which is exactly what
# happened the first time a DTO field was added and a test's struct literal did
# not compile.
cargo test --manifest-path "$root/crates/api/Cargo.toml" \
  --features ts export_bindings --no-run >/dev/null
rm -f "$out/bindings.ts"

# Plain `cargo test`, deliberately not nextest, even though the Makefile prefers
# nextest everywhere else. All 77 types append to one bindings.ts; cargo runs
# them as threads in a single process, where ts-rs is the only writer. nextest
# runs each test in its own process, and 77 processes appending to one file has
# no reason to produce a whole file. Not measured here (nextest was absent on the
# machine this was written on) — pinned because the failure mode is a silently
# short file, which is the shape of bug this seam exists to abolish. The length
# assertion at the bottom is the backstop either way.
TS_RS_EXPORT_DIR="$out" cargo test \
  --manifest-path "$root/crates/api/Cargo.toml" \
  --features ts export_bindings -- --quiet >/dev/null

# ts-rs maps i64/u64 to `bigint`. That is right for a transport that can carry
# one and wrong for this one. Tauri IPC is JSON: an i64 arrives from JSON.parse
# as a `number`, and `JSON.stringify(3n)` THROWS — so a `bigint` id on an
# outgoing request is a runtime failure that tsc calls correct, which is the
# worst of both. Every i64 crossing this seam is a row id, a page count, a unix
# second or a minute total, all far below 2^53, so `number` is exact as well as
# honest. perl rather than `sed -i`, whose in-place flag differs between BSD and
# GNU and would work on the dev machine and not in CI.
perl -i -pe 's/\bbigint\b/number/g' "$out/bindings.ts"

if grep -q 'bigint' "$out/bindings.ts"; then
  echo "gen-ts: a bigint survived the widening — see the comment in $0" >&2
  exit 1
fi

# The floor is derived, not hardcoded, so adding a DTO does not silently lower
# it: every exported type carries exactly one `ts(export` attribute.
want=$(grep -ho 'ts(export' "$root"/crates/api/src/*.rs | wc -l | tr -d ' ')
got=$(grep -c '^export type' "$out/bindings.ts" | tr -d ' ')
if [ "$got" -ne "$want" ]; then
  echo "gen-ts: exported $got types, the crate declares $want." >&2
  echo "        A truncated file looks exactly like this. Rerun; if it persists," >&2
  echo "        a type is annotated but unreachable from any exported root." >&2
  exit 1
fi

echo "gen-ts: $got types -> $out/bindings.ts"

# Expect four `failed to parse serde attribute: other` warnings on stderr, and do
# NOT silence them with ts-rs's `no-serde-warnings` feature. They name the one
# place serde and ts-rs disagree about this surface: `#[serde(other)]` on
# `ErrorCode::Internal` is what makes an unknown code from a newer daemon degrade
# rather than fail to parse (crates/api/CLAUDE.md), and ts-rs drops it. So the
# generated `ErrorCode` union is exhaustive over *today's* codes while the wire is
# not. A frontend must therefore keep a default arm on ErrorCode even though tsc
# will tell it the arm is unreachable.
