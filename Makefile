# readingbuddy — repeatable test/lint entry points.
#
# Wraps cargo + cargo-nextest. nextest is auto-detected; if it is not
# installed every target degrades to plain `cargo test` so nothing hard-requires
# it. Install for faster parallel runs: `cargo install cargo-nextest`.

# Detect nextest once.
NEXTEST := $(shell command -v cargo-nextest 2>/dev/null)
ifdef NEXTEST
  RUN = cargo nextest run
  RUN_FILTER = cargo nextest run -E
else
  RUN = cargo test
  RUN_FILTER = cargo test --test
endif

# Detect the frontend the same way nextest is detected, and for the same reason:
# every web target must degrade to a stated skip rather than an error, so `make
# check` keeps working in a tree where the GUI has not landed yet (spec item 25)
# and on a machine with no node installed.
GUI_PKG := $(wildcard gui/package.json)
GUI_DEPS := $(wildcard gui/node_modules)

.PHONY: help test test-engine test-import golden corpus corpus-check synthetic goodreads kostats lint build-check fmt fmt-check check ci clean dist bench bench-box bench-trend perf ts ts-check dev-db web-check web-fix shots routes e2e

# Perf output, kept so runs can be compared over time.
#
# Two shapes, on purpose. Each run gets its own timestamped JSONL + summary
# (they describe one session and would be meaningless concatenated), while
# `history.tsv` accumulates one row per mode per run — that is the trend line,
# and it is what "did this change help" is actually read off.
# Absolute: cargo runs a test binary from the package root, so a relative path
# would put `make perf`'s log under crates/tui/ instead of here.
PERF_DIR ?= $(CURDIR)/perf
PERF_STAMP := $(shell date +%Y%m%d-%H%M%S)
PERF_LOG ?= $(PERF_DIR)/$(PERF_STAMP)-bench.jsonl
PERF_SUMMARY ?= $(PERF_DIR)/$(PERF_STAMP)-bench.txt
PERF_HISTORY ?= $(PERF_DIR)/history.tsv

# Repeats of the bench script, e.g. `make bench BENCH_REPS=3`. Modes interleave
# across reps, so a transient cannot land entirely on one of them. Pass anything
# else through with BENCH_ARGS, e.g. BENCH_ARGS="--book pachinko".
BENCH_REPS ?= 1
# Seed for the tier-2 corpus. Output is a pure function of (seed, generator
# version, epub bytes), so changing this changes every generated sidecar.
CORPUS_SEED ?= 42
BENCH_ARGS ?=

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
	  awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

test: ## Run the whole workspace test suite
	$(RUN) --workspace

test-import: ## Run only the KOReader import harness
ifdef NEXTEST
	cargo nextest run -p readingbuddy -E 'test(/koreader_import/)'
else
	cargo test -p readingbuddy --test koreader_import
endif

golden: ## Regenerate the golden import snapshots
	UPDATE_GOLDEN=1 cargo test -p readingbuddy --test koreader_import import_matches_golden

bench: ## Compare renderers end-to-end (REAL, ACTIVE pane; BENCH_REPS=3 to average)
	@echo "~$$(( ($(BENCH_REPS) * 3 * 265) / 20 ))s, taking over the terminal."
	@echo "It must run in a real pane, and inside tmux that pane must be the"
	@echo "ACTIVE one — tmux routes input to the focused pane only, so a"
	@echo "background pane gets no replies and the latency columns come back empty."
	@echo
	@echo "One rep is enough for the byte columns, which are deterministic."
	@echo "The latency columns are not: they pick up whatever else the machine is"
	@echo "doing, so use BENCH_REPS=3 before believing a surprising one."
	@echo
	@mkdir -p $(PERF_DIR)
	@# The summary is printed only after the terminal is restored, so capturing
	@# stderr and replaying it afterwards loses nothing and keeps a copy.
	cargo run --release -p readingbuddy-tui -- \
	  --bench-render all --bench-reps $(BENCH_REPS) $(BENCH_ARGS) \
	  --perf-log $(PERF_LOG) --perf-history $(PERF_HISTORY) \
	  2> $(PERF_SUMMARY); st=$$?; cat $(PERF_SUMMARY); exit $$st
	@echo
	@echo "per-frame log : $(PERF_LOG)"
	@echo "summary       : $(PERF_SUMMARY)"
	@echo "trend         : $(PERF_HISTORY)"

bench-box: ## Same comparison, but in a disposable terminal (no pane hijack)
	@# A throwaway kitty + its own tmux server, so the run needs no terminal of
	@# yours, cannot touch your panes or your library, and comes back with a
	@# pinned window size and font. Rows land in the same history.tsv tagged
	@# env=box — trend those against each other, never against env=live.
	@# Its own reps default (3, because rep 1 warms the fresh terminal) is left
	@# alone unless BENCH_BOX_REPS is set — passing BENCH_REPS's 1 through here
	@# would silently reinstate the cold-terminal reading.
	$(if $(BENCH_BOX_REPS),BENCH_REPS=$(BENCH_BOX_REPS) ,)scripts/bench-sandbox.sh $(BENCH_ARGS)

bench-trend: ## Show every recorded bench run, oldest first
	@test -f $(PERF_HISTORY) || { echo "no history yet — run 'make bench'"; exit 1; }
	@column -t -s "$$(printf '\t')" $(PERF_HISTORY)

perf: ## Cost + wire-rate sweeps, both renderers (release, ignored tests)
	@mkdir -p $(PERF_DIR)
	READINGBUDDY_PERF_LOG=$(PERF_DIR)/$(PERF_STAMP)-sweep.jsonl \
	  cargo test --release -p readingbuddy-tui --bins -- \
	  --ignored --nocapture --test-threads 1 \
	  glyph_cost raster_cost frame_budget glyph_wire_rate wire_rate

lint: ## Clippy across the workspace (warnings are errors, same as CI)
	cargo clippy --workspace --all-targets -- -D warnings

# NOT subsumed by `lint`, and the difference is load-bearing. `--all-targets`
# resolves dev-dependencies, and `crates/tui`'s dev-dependencies switch on the
# engine's `internals` feature — so under clippy the `Engine::storage()` escape
# hatch exists for every target in the graph, including the shipped binaries.
# This is the build in which it does not, which makes it the only thing standing
# between item 14's closed seam and a frontend quietly reopening it.
build-check: ## Build the shipped targets only — the `internals` feature must not be needed
	cargo check --workspace --locked

fmt: ## Format all crates
	cargo fmt --all

fmt-check: ## Verify formatting without writing
	cargo fmt --all --check


# ---------------------------------------------------------------------------
# Frontend. `pnpm`, never npm or yarn — a second lockfile is a silent
# divergence. Each target states its skip rather than failing, because an absent
# GUI is a fact about this tree and not a broken build.
# ---------------------------------------------------------------------------

# The dev library. `dev-data/` is gitignored and reproducible from a seed, so it
# is disposable by design — `make dev-db` always rebuilds it from scratch rather
# than migrating what is there, because a half-seeded database is harder to
# diagnose than a missing one.
DEV_DB_DIR ?= $(CURDIR)/dev-data
DEV_DB_SEED ?= 42
DEV_DB_SRC := corpus/generated/devdb

dev-db: ## Build a seeded library at $(DEV_DB_DIR) — ~200 books, covers, a vault
	@command -v sqlite3 >/dev/null || { \
	  echo "dev-db needs the sqlite3 CLI (present on macOS and ubuntu runners)."; exit 1; }
	@# --data-dir, and not only --out: `cover_path` is a whole path in this
	@# schema, so the seed has to name where the library is being built.
	cargo run -q -p corpus -- gen-devdb --seed $(DEV_DB_SEED) --data-dir "$(DEV_DB_DIR)"
	rm -rf "$(DEV_DB_DIR)"
	mkdir -p "$(DEV_DB_DIR)/database/images" "$(DEV_DB_DIR)/vault"
	@# The engine creates and migrates the database, so corpus never owns a second
	@# copy of the schema or of sqlx's `_sqlx_migrations` ledger. Any read-only
	@# command does it; `list` is the cheapest and prints the empty-library line.
	cargo run -q -p readingbuddy-cli -- --data-dir "$(DEV_DB_DIR)" list >/dev/null
	cp $(DEV_DB_SRC)/covers/*.png "$(DEV_DB_DIR)/database/images/"
	cp $(DEV_DB_SRC)/vault/*.md "$(DEV_DB_DIR)/vault/"
	sqlite3 "$(DEV_DB_DIR)/database/app.db" < $(DEV_DB_SRC)/seed.sql
	@# The seed states `cover_path` and cannot state `cover_width` — SQLite cannot
	@# decode a PNG, which is why item 20's back-fill is a command. Without this
	@# every book here has a cover and a NULL `cover_aspect`, and a shelf reading
	@# that column concludes the column does not work.
	cargo run -q -p readingbuddy-cli -- --data-dir "$(DEV_DB_DIR)" covers
	@# reading_events comes from the engine's own fillers rather than from invented
	@# rows — the fixture states highlights and readings, and the real derivation
	@# turns them into a log. A generator writing that table directly would be
	@# asserting item 21's arithmetic instead of exercising it.
	cargo run -q -p readingbuddy-cli -- --data-dir "$(DEV_DB_DIR)" activity --refill
	@echo ""
	@echo "dev library at $(DEV_DB_DIR) — 220 books, 20 of them deliberate edge cases."
	@echo "What each edge case is for: $(DEV_DB_SRC)/manifest.json"

ts: ## Regenerate gui/src/lib/api/bindings.ts from the API crate's own types
	scripts/gen-ts.sh "$(CURDIR)/gui/src/lib/api"

# The gate. Generation output is COMMITTED so a thread that cannot run cargo
# still reads current types; this is what stops that copy going stale silently.
# A DTO change without a regeneration fails here rather than becoming a blank
# panel in a webview with a console error nobody is reading.
ts-check: ## Fail if the committed bindings are not what the DTOs generate
	@tmp=$$(mktemp -d); \
	scripts/gen-ts.sh "$$tmp" >/dev/null; \
	if diff -u gui/src/lib/api/bindings.ts "$$tmp/bindings.ts" > "$$tmp/diff"; then \
	  rm -rf "$$tmp"; echo "ts-check: bindings match the DTOs"; \
	else \
	  cat "$$tmp/diff"; rm -rf "$$tmp"; \
	  echo ""; echo "ts-check: bindings are stale. Run 'make ts' and commit."; exit 1; \
	fi

web-check: ## Frontend gate: svelte-check + tsc + eslint + vitest + build
ifeq ($(GUI_PKG),)
	@echo "SKIPPED: no gui/package.json — the GUI scaffold is spec item 25."
else ifeq ($(GUI_DEPS),)
	@echo "SKIPPED: gui/node_modules absent — run 'pnpm install' in gui/ first."
else
	cd gui && pnpm exec svelte-check --threshold error
	cd gui && pnpm exec tsc --noEmit
	cd gui && pnpm exec eslint .
	cd gui && pnpm vitest run
	cd gui && pnpm build
endif

web-fix: ## Format + autofix the frontend (writes files, like `fmt`)
ifeq ($(GUI_DEPS),)
	@echo "SKIPPED: no gui/node_modules."
else
	cd gui && pnpm exec prettier --write .
	cd gui && pnpm exec eslint . --fix
endif

shots: ## Render every route to gui/tests/shots/ for the screenshot-reviewer agent
ifeq ($(GUI_DEPS),)
	@echo "SKIPPED: no gui/node_modules."
else
	@# Three projects, all WebKit — desktop, narrow and phone. WebKit because
	@# WKWebView is what the app ships inside on macOS, so its bugs are ours;
	@# Chromium would be a smaller download and the wrong browser. `--update-
	@# snapshots` because this target's job is to PRODUCE the images for a human
	@# or the screenshot-reviewer agent to look at. `make routes` is the target
	@# that fails on a diff.
	cd gui && pnpm exec playwright test --update-snapshots
	@echo ""
	@echo "shots in gui/tests/shots/ — read the PNGs, do not trust a green run."
endif

routes: ## Assert every route still renders and matches its committed shot
ifeq ($(GUI_DEPS),)
	@echo "SKIPPED: no gui/node_modules."
else
	cd gui && pnpm exec playwright test
endif

# NOT part of `check` or `ci`, deliberately. It builds the app binary and drives
# a real webview, which is minutes, and `tauri-driver` does not run on macOS at
# all (no WKWebView driver exists). This is the seam check — does it boot, does
# one real invoke reach SQLite — never a feature suite. See docs/gui/testing.md.
e2e: ## E2E smoke against the built app (slow; Linux, or the wdio plugin on macOS)
ifeq ($(GUI_DEPS),)
	@echo "SKIPPED: no gui/node_modules."
else
	cd gui && pnpm exec wdio run wdio.conf.ts
endif

check: fmt-check lint build-check ts-check test web-check routes ## Local gate: everything CI runs

# What .github/workflows/ci.yml runs, in the same order. Kept as a separate
# target from `check` even when the two agree: it is the name people reach for,
# and they diverge the moment the workflow grows a step with no local equivalent.
# Keep them in step — a change here belongs in the workflow too.
#
# CI's macOS leg runs `test-engine` rather than `test`; that asymmetry is
# explained in the workflow and is not worth reproducing locally.
#
# The GUI landed (spec item 25), so `ci` now carries the frontend. Four things
# went in with it and each has a distinct job:
#   ts-check   — the committed bindings.ts still matches the DTOs. In CI's `check`
#                job, since it needs cargo and no node.
#   web-check  — svelte-check + tsc + eslint + vitest + build.
#   routes     — layer 2: every route renders in WebKit at three sizes and matches
#                its committed shot. `tauri-driver` cannot run on macOS, so this
#                browser suite is the visual gate, not a stand-in for one.
# `e2e` stays out of both: it builds the app binary and drives a real webview,
# which is minutes, and is a seam check rather than a feature suite.
ci: fmt-check lint build-check ts-check test web-check routes ## Reproduce the CI gate locally

test-engine: ## Engine tests only — CI's macOS leg, and the fast inner loop
	$(RUN) -p readingbuddy

corpus: ## Fetch Gutenberg epubs, generate tier-2 sidecars, run the corpus tests
	scripts/fetch-corpus.sh
	cargo run -p corpus -- gen-corpus --seed $(CORPUS_SEED)
	cargo test -p readingbuddy --test corpus -- --nocapture

corpus-check: ## Corpus tests only (offline; skips loudly if not generated)
	cargo test -p readingbuddy --test corpus -- --nocapture

synthetic: ## Regenerate the committed tier-1 hostile fixtures, then the goldens
	cargo run -p corpus -- gen-synthetic
	$(MAKE) golden

goodreads: ## Regenerate the committed Goodreads export fixture
	cargo run -p corpus -- gen-goodreads --seed $(CORPUS_SEED)

kostats: ## Regenerate the committed KOReader statistics fixture (SQL + expected totals)
	cargo run -p corpus -- gen-kostats --seed $(CORPUS_SEED)

dist: ## Build this machine's release archive into dist/, exactly as CI would
	@set -eu; \
	target=$$(rustc -vV | sed -n 's|^host: ||p'); \
	version=v$$(cargo pkgid -p readingbuddy-tui | sed 's/.*[#@]//'); \
	name="readingbuddy-$${version}-$${target}"; \
	cargo build --profile dist --locked --target "$$target" -p readingbuddy-tui -p readingbuddy-cli; \
	rm -rf "dist/$${name}"; mkdir -p "dist/$${name}"; \
	cp "target/$${target}/dist/readingbuddy-tui" "target/$${target}/dist/readingbuddy" \
	   LICENSE README.md TUTORIAL.md "dist/$${name}/"; \
	tar -C dist -czf "dist/$${name}.tar.gz" "$${name}"; \
	rm -rf "dist/$${name}"; \
	ls -lh "dist/$${name}.tar.gz"

clean: ## cargo clean
	cargo clean
