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

.PHONY: help test test-import golden lint fmt fmt-check check clean bench bench-trend perf

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

bench: ## Compare renderers end-to-end (needs a REAL, ACTIVE pane)
	@echo "This takes over the terminal for ~40s (240 frames x 3 modes, paced to"
	@echo "the app's 20fps tick). It must run in a real pane,"
	@echo "and inside tmux that pane must be the ACTIVE one — tmux routes input"
	@echo "to the focused pane only, so a background pane gets no replies and"
	@echo "the terminal-latency columns come back empty."
	@echo "Bench a book that has a real cover: the procedural plate compresses"
	@echo "several times better and flatters every image number."
	@echo
	@mkdir -p $(PERF_DIR)
	@# The summary is printed only after the terminal is restored, so capturing
	@# stderr and replaying it afterwards loses nothing and keeps a copy.
	cargo run --release -p readingbuddy-tui -- \
	  --bench-render all --perf-log $(PERF_LOG) --perf-history $(PERF_HISTORY) \
	  2> $(PERF_SUMMARY); st=$$?; cat $(PERF_SUMMARY); exit $$st
	@echo
	@echo "per-frame log : $(PERF_LOG)"
	@echo "summary       : $(PERF_SUMMARY)"
	@echo "trend         : $(PERF_HISTORY)"

bench-trend: ## Show every recorded bench run, oldest first
	@test -f $(PERF_HISTORY) || { echo "no history yet — run 'make bench'"; exit 1; }
	@column -t -s "$$(printf '\t')" $(PERF_HISTORY)

perf: ## Cost + wire-rate sweeps, both renderers (release, ignored tests)
	@mkdir -p $(PERF_DIR)
	READINGBUDDY_PERF_LOG=$(PERF_DIR)/$(PERF_STAMP)-sweep.jsonl \
	  cargo test --release -p readingbuddy-tui --bins -- \
	  --ignored --nocapture --test-threads 1 \
	  glyph_cost raster_cost frame_budget glyph_wire_rate wire_rate

lint: ## Clippy across the workspace
	cargo clippy --workspace --all-targets

fmt: ## Format all crates
	cargo fmt --all

fmt-check: ## Verify formatting without writing
	cargo fmt --all --check

check: fmt-check lint test ## CI-style gate: fmt + lint + test

clean: ## cargo clean
	cargo clean
