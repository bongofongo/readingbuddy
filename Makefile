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

.PHONY: help test test-engine test-import golden corpus corpus-check synthetic goodreads lint fmt fmt-check check ci clean bench bench-box bench-trend perf

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

fmt: ## Format all crates
	cargo fmt --all

fmt-check: ## Verify formatting without writing
	cargo fmt --all --check

check: fmt-check lint test ## Local gate: fmt + lint + whole-workspace test

# What .github/workflows/ci.yml runs, in the same order — which now makes `ci`
# and `check` the same three steps, since the gate widened to the whole
# workspace on ubuntu. Kept as a separate target anyway: it is the name people
# reach for, and the two will diverge again the moment the workflow grows a step
# that has no local equivalent. Keep them in step — a change here belongs in the
# workflow too.
#
# CI's macOS leg runs `test-engine` rather than `test`; that asymmetry is
# explained in the workflow and is not worth reproducing locally.
ci: fmt-check lint test ## Reproduce the CI gate locally

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

clean: ## cargo clean
	cargo clean
