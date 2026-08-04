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

.PHONY: help test test-engine test-import golden corpus corpus-check synthetic goodreads lint build-check fmt fmt-check check ci clean dist bench bench-box bench-trend perf web-check web-fix shots e2e

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
	cd gui && pnpm exec playwright test --project=webkit --update-snapshots
	@echo "shots in gui/tests/shots/ — read the PNGs, do not trust a green run."
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

check: fmt-check lint build-check test web-check ## Local gate: fmt + lint + build + test + frontend

# What .github/workflows/ci.yml runs, in the same order — which now makes `ci`
# and `check` the same three steps, since the gate widened to the whole
# workspace on ubuntu. Kept as a separate target anyway: it is the name people
# reach for, and the two will diverge again the moment the workflow grows a step
# that has no local equivalent. Keep them in step — a change here belongs in the
# workflow too.
#
# CI's macOS leg runs `test-engine` rather than `test`; that asymmetry is
# explained in the workflow and is not worth reproducing locally.
#
# `check` and `ci` have now DIVERGED on purpose: `check` runs `web-check` and
# `ci` does not, because .github/workflows/ci.yml has no frontend job yet. When
# the GUI lands (spec item 25) the workflow grows that step and this comment
# comes out. Until then, adding web-check here would make `make ci` stop
# reproducing the gate, which is the one thing it is for.
ci: fmt-check lint build-check test ## Reproduce the CI gate locally

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
