# crates/corpus

The fixture generator for the import harnesses. Not shipped; not a dependency of
anything the user runs.

**This crate must never depend on `readingbuddy`.** Reusing the engine's own
parsing or normalization to build its fixtures would bake any bug in those
straight into the goldens — the generator is an independent oracle, and that is
the whole of its value.

Two tiers, and the split is load-bearing:

- **tier 1, `gen-synthetic`** — small, committed, covers every *shape* (hostile
  input, the sibling-epub ISBN path). Runs offline in CI on every PR. Shape
  coverage must not sit behind a download, or the branch is untested on every
  machine that has not run the fetch script.
- **tier 2, `gen-corpus`** — derived from real Project Gutenberg epubs,
  gitignored, covers *scale and realism*. Nightly only. **The sandbox proxy
  blocks gutenberg.org, so this cannot run in a cloud session at all** — a
  hosted CI runner is the only machine that can build it.

The determinism rules and the tier-2 layout split are stated in full in the root
`CLAUDE.md` under **Engine standards** ("Fixtures are generated, not
hand-written" and "Corpus determinism"). Read them before changing a generator:
`corpus/generated` is gitignored, so on a runner every test in `tests/corpus.rs`
takes its `SKIPPED:` path and asserts nothing unless
`READINGBUDDY_REQUIRE_FIXTURES=1` is set — which is exactly how one bug survived
item 2 and was found weeks later by a hand-run `make corpus`.

Regenerate: `make synthetic` (tier 1 + goldens), `make goodreads`, `make corpus`
(tier 2).
