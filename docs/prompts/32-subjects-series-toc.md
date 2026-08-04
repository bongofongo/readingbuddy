# Prompt — Item 32: subjects, series, and the chapter list

Paste into a fresh session at the repo root, on branch `feat/engine-subjects-series`,
**branched from main after items 29 and 30 have merged.**

---

Read `docs/spec-engine-29-32.md` (item 32), `docs/decisions.md` (the
**Collections** section — it decides half of this item) and
`crates/engine/src/providers/CLAUDE.md`. `CLAUDE.md`'s **Engine standards**
section is binding, and `crates/engine/migrations/CLAUDE.md` before you write the
migration.

**Engine only. Owns migration `0013`.** No CLI, no TUI, no API. Both providers,
`epub.rs`, and `MERGE_RULES`.

Three fields nothing captures, each of which makes a book's page understandable
rather than merely correct.

## Migration `0013`

- **Subjects** — Google Books `volumeInfo.categories`, OpenLibrary
  `/works/{}.json` `subjects`. **Separate from `book_tags`**, which is for
  shelves the user or another system minted. `docs/decisions.md:124` defers
  collections precisely because three systems minting them is a merge problem
  with no good default, and a provider subject is not a collection — it is a
  bibliographic fact with an origin, like a publisher. Getting this wrong merges
  two vocabularies that must not merge.
- **Series and index** — no column exists, which is why `calibre.rs` drops series
  outright and records that it did. `matching.rs` already had to be taught not to
  confuse `Dune` with `Dune Messiah`; this is the field that would have made that
  distinction cheap.
- Whatever the TOC needs, if it needs storage at all — see below.

## The table of contents

`epub.rs` extracts a cover and nothing else (`extract_cover`, called from
`lib.rs`). A TOC lets a highlight name its chapter where KOReader did not, and
lets progress read as a position in a structure rather than a bare page number.
Fully offline, from a file already owned.

**Decide whether it is stored or derived on demand**, and defend it. The file is
in `files_dir` and content-addressed, so re-reading it is cheap and always
current; a stored copy is a second answer that can go stale when a better file is
attached. `book_files` already keys on `sha256`, which is the hook either way.

**Do not resolve `pos0` against it.** `docs/decisions.md` is explicit: a `pos0`
is a cre-engine xpointer, and resolving one means reimplementing enough of that
engine to agree with it. When an excerpt view lands it searches the epub for the
highlight's *text*. Naming a chapter is a different and much smaller claim.

## Every new field is a `MERGE_RULES` row

Item 29 made `field_provenance` a third consumer of that table, alongside the
upsert clause and `enrich_book`'s `UPDATE`. Add your fields there and they get
provenance for free; add them anywhere else and you have created exactly the
drift that arrangement exists to prevent. **This is the second reason 29 went
first**, and it is the thing to check before you write a line of provider code.

A provider record is *partial*, so no-clobber — do not copy the device's straight
assignment. And subjects are a **set**, not a scalar: decide what merging two
providers' sets means (union? does an empty set mean "none" or "don't know"?) and
write it down, because `MERGE_RULES` has no vocabulary for a set today and you
are the one adding it.

**Items 29, 30 and 31 have all landed since this prompt was written.** Three
things they changed under you:

- `MERGE_RULES` now generates **five** things, not two: the upsert's
  `ON CONFLICT`, `enrich_book`'s `UPDATE`, `merge_books`' `dst`-wins fill, the
  `field_provenance` stamps, and `Rule::show` (how a held-back field's offered
  value prints). A new column is a new row there and nothing else — but check all
  five, and note `PROBES` in `tests_support`, whose column list is asserted to
  equal `MERGE_RULES`' in order, so your three fields will fail those sweeps with
  a message rather than going quietly uncovered. That is deliberate; extend it.
- **`rb set` exists** (item 30), and it is how a field becomes the user's. Decide
  whether your new fields belong on it. A field a provider can write and a user
  cannot correct is the dead end the axiom bans.
- **Per-field provenance cannot protect a field *pair*, and yours is one.**
  Item 30 hit this for real: a user-owned `isbn_13` held while the unowned
  `isbn_10` landed from a *different edition*, so the row now carries two ISBNs
  of two editions. `series` + `series_index` has exactly that shape — owning
  "Dune" without owning "#2" is not a coherent state. Item 30 left the ISBN case
  recorded rather than patched because the fix is a design decision (does a claim
  attach to an identifier or to an edition?), and it needs a claim without a
  value. **You are the second instance, which is what makes it worth deciding.**
  Say what you concluded even if you conclude it is still not this item's to fix.

## Must not

- **No new provider.** Out of scope for this wave. OpenLibrary's `/works/`
  endpoint is a second *request* to a provider already integrated, not a new
  source — but note it is a request per book and say what that costs.
- **No collections, no shelf UI, no grouping.** You are storing facts. What is
  done with them is a later item and a settled-deferral.
- **No `book_tags` writes.** Different table, different meaning, deliberately.
- No API or DTO surface.

## Files

`crates/engine/migrations/0013_subjects_series.sql`,
`crates/engine/src/storage/books.rs` (`MERGE_RULES`),
`crates/engine/src/providers/openlibrary.rs`,
`crates/engine/src/providers/googlebooks.rs`, `crates/engine/src/epub.rs`,
`crates/engine/src/book.rs`, `lib.rs`, tests. **You run alone** — this item
touches item 29's file and item 30's files, which is why it is last rather than
parallel.

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, no test touches the
network, and there is a real epub fixture exercising the TOC path (`epubs/` holds
samples; a test that returns early when the fixture is absent must print
`SKIPPED:` and honour `READINGBUDDY_REQUIRE_FIXTURES=1` — `CLAUDE.md` forbids the
silent kind).

**Push back rather than comply.** Two places this prompt may be wrong: subjects
may want a controlled vocabulary rather than raw strings (the same question
`book_tags` deferred), and the TOC may not want storage at all. Both are real
design questions and this prompt has picked an answer for neither.

**Report the corrections this forced**, in the shape `docs/decisions.md`'s
existing entries use.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
