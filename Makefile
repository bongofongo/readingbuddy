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

.PHONY: help test test-import golden lint fmt fmt-check check clean

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

lint: ## Clippy across the workspace
	cargo clippy --workspace --all-targets

fmt: ## Format all crates
	cargo fmt --all

fmt-check: ## Verify formatting without writing
	cargo fmt --all --check

check: fmt-check lint test ## CI-style gate: fmt + lint + test

clean: ## cargo clean
	cargo clean
