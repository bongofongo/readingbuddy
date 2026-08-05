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

## The dev library — `gen-devdb`, and how it keeps the rule

Beside the two tiers there is now a third output that is not an *import fixture*
at all: `gen-devdb` builds a **populated database** for the GUI to render, because
until it existed every GUI thread started against an empty shelf or against a
personal library that cannot be committed.

**It emits data, never a schema**, and that is how the no-`readingbuddy` rule
survives contact with SQLite. The engine owns the schema and `sqlx::migrate!` owns
the `_sqlx_migrations` ledger it is recorded in (checksummed sha384 over each
migration's raw bytes — measured, not assumed). A generator that created tables
would be a second copy of the schema free to drift; one that wrote the ledger
would be a reimplementation of sqlx's bookkeeping whose failure mode is a
`VersionMismatch` at startup rather than an obvious error here. So `make dev-db`
has the real `rb` binary create and migrate an empty database, and `seed.sql`
fills it. Every `INSERT` lists its columns, which is what keeps a renamed column
loud.

Two details worth knowing:

- **`notes_fts` is written by the seed.** There are no triggers on that table —
  the engine populates it in application code — so a seeded note without its FTS
  row is a note `SearchNotes` cannot find, which is the one thing item 27's search
  box exists to do.
- **`reading_events` is not seeded.** `make dev-db` runs `rb activity --refill`
  instead, so the log comes from the engine's own fillers. A generator writing
  that table directly would be asserting item 21's arithmetic rather than
  exercising it.

The twenty deliberate edge cases at the front of the library are the point, not
the two hundred ordinary books behind them: `page_count` of zero (item 17b's false
denominator), a 1,408-page doorstop beside a 48-page pamphlet (item 19's
thickness scale), no cover, a 220-character title, RTL, CJK, a mononym author, a
book with no author, an abandoned reading, a reread. `manifest.json` names what
each one is for, so a screenshot reviewer knows what to look at. Note that
`gui/src/lib/api/fake.ts` mirrors these shapes for the frontend's own layers —
two fixtures with one purpose, and the drift between them is a known cost.

Regenerate: `make synthetic` (tier 1 + goldens), `make goodreads`, `make corpus`
(tier 2), `make dev-db` (the GUI's library).
