# Prompt — Item 30: looking a book up again

Paste into a fresh session at the repo root, on branch `feat/engine-enrich`,
**branched from main after item 29 has merged.**

---

Read `docs/spec-engine-29-32.md` (item 30), `docs/decisions.md` (the **Data
ownership** and **Out of scope for now** sections — the second one constrains
this item directly) and `crates/engine/src/providers/CLAUDE.md`. `CLAUDE.md`'s
**Engine standards** section is binding.

**Engine + CLI. No migration.** Item 29's `field_provenance` is the thing this
writes through; if it is not on main yet, stop and say so rather than building
around it.

## The gap

`Storage::enrich_book` exists and **only calibre calls it**. Every book created
without an ISBN — `import_book_from_sidecar` (offline by design, correctly),
`import_file` matched on a filename stem — has no cover, no description and no
page count, permanently. On a real library that is most of the shelf.

## `Engine::enrich_book_from_providers(id) -> Result<EnrichReport>`

- **ISBN → `lookup_isbn`.** No ISBN → a `SearchRequest { title, author }` scored
  through the **existing** `matching.rs` bands (`koreader::scores_for` +
  `band`, the same path a sidecar, a Goodreads row and a calibre row take).
  **Do not invent a second matcher** — `docs/decisions.md` names that rule under
  **Files** and it holds wherever books are matched.
- **Below `AUTO_MATCH`: return the candidates and write nothing.** The same
  refusal-with-a-next-move shape as `ko pull` and `calibre import`, with the same
  `--new`-shaped override for the user who has looked and decided.
- **Merge through `MERGE_RULES`** — a provider record is *partial*, so no-clobber,
  not the device's straight assignment. `docs/decisions.md` is explicit that the
  pattern is chosen by whether the record is complete, and warns against copying
  one to the other.
- **Stamp `field_provenance` on everything written**, and **never overwrite a
  field whose provenance is `user`.** Hold it back and say so in the report — a
  field silently not updated is indistinguishable from a field the provider had
  nothing for.
- Then `download_cover` where the book has none.

`EnrichReport` names every field **filled**, every field **held back** and why,
and the candidates when it refused. A frontend must be able to show what changed
without re-querying.

## The trap: `search.rs`'s scan is not free

`matching.rs`'s comparison runs against the whole library, and every other
importer that does this reads the shelf **once** per row and takes both answers
out of it — `calibre.rs` records that asking for the auto-match and the candidate
band separately read the whole shelf twice per row, which on a four-hundred-book
library is eight hundred loads of it. Enriching a *selection* must not repeat
that mistake.

## Must not

- **Not on the device pull path.** `docs/decisions.md:231` puts "provider
  enrichment on device pull" out of scope and that ruling stands. This is an
  explicit action on a book the user is looking at. `import_book_from_sidecar`
  stays fully offline; do not touch it.
- **Nothing automatic, nothing periodic, and no count.** A "12 books need
  refreshing" badge is task-completion framing, which the design axiom bans by
  name. A staleness *query* is fine — a number that greets the user is not.
- **No new provider.** Adding a fourth source is explicitly out of this wave.
- **No network in tests, ever.** The mock `MetadataProvider` is how the fan-out
  is tested; `wiremock` only for real status codes.

## CLI

`readingbuddy enrich <selector> [--new]`, printing what filled, what was held
back, and — on a refusal — the candidates plus both next moves. All printing
lives in `crates/cli`; the engine does no terminal I/O.

Note `crates/cli/tests/cli.rs` drives the real binary and has a golden of the
**subcommand name set**. Adding `enrich` will fail it; updating that list in the
same commit is the point of it.

## Files

`crates/engine/src/lib.rs`, `crates/engine/src/search.rs` (possibly),
`crates/engine/src/storage/books.rs` (provenance-aware merge only — item 29 owns
the shape), `crates/cli/src/commands/`, tests. Item 31 is running concurrently in
`device.rs` and a new statistics module — no overlap expected. Item 32 will touch
`providers/` **after** you; leave it alone.

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, and it has been run
by hand against a real sidecar-seeded book — the case the whole item exists for —
with the before/after `rb show` in the PR body.

**Push back rather than comply.** In particular: if the `user`-provenance rule
turns out to have no way to be set (there is no user-edit path for most of these
fields yet), say that plainly rather than shipping a protection nothing can
trigger. It may be that this item should *also* carry a `set_field` that records
`user` — decide, and argue for it.

**Report the corrections this forced**, in the shape `docs/decisions.md`'s
existing entries use.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
