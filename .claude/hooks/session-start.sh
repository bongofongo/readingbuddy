#!/bin/bash
#
# SessionStart hook — warm a fresh cloud container so the first `cargo` command
# is not also a toolchain download and a from-scratch build of two C libraries.
#
# Without this, the first thing any session does is pay for:
#   * a full rustup sync of the pinned toolchain (rust-toolchain.toml, 1.97.1)
#     plus rustfmt and clippy, because the runners' rustup installs it lazily on
#     the first cargo call;
#   * compiling vendored lua54 (mlua) and libsqlite3-sys from source.
#
# The container image is cached once this hook finishes, so everything it builds
# is paid for once rather than every session.
#
# Local machines are left alone — they already have all of this.

set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

echo "readingbuddy: warming the toolchain…"

# Forces rustup to install what rust-toolchain.toml pins, with its components,
# before anything needs them. `rustup show` is enough to trigger the sync.
rustup show active-toolchain

# Prefetch the registry against the committed lockfile. --locked so this can
# never be the thing that quietly updates a dependency.
echo "readingbuddy: fetching dependencies…"
cargo fetch --locked

# Build the test binaries too. This is the expensive half (vendored Lua,
# libsqlite3) and the half that makes the difference: after it, `cargo test` in
# the session is a link rather than a build.
#
# Non-fatal on purpose. A broken build is a thing the session should discover and
# report, not a thing that stops the session from starting.
echo "readingbuddy: building workspace + tests (this is the slow one)…"
if ! cargo build --workspace --tests --locked; then
  echo "readingbuddy: warm-up build failed — the session will still start." >&2
fi

# Deliberately NOT installed: cargo-nextest.
#
# Its prebuilt binary lives behind get.nexte.st, which this sandbox's proxy
# refuses (CONNECT 403), and `cargo install cargo-nextest` compiles it from
# source for a runner that will use it a handful of times. The Makefile already
# degrades every target to plain `cargo test` when nextest is absent, precisely
# so nothing hard-requires it. CI installs it via taiki-e/install-action, where
# the download works.

echo "readingbuddy: ready. \`make ci\` reproduces the gate; \`make test-engine\` is the fast loop."
