---
title: Item 22 — reading here, and the word the schema refused
date: 2026-08-05
follows: sessions/2026-08-05-the-derived-facts-layer.md
branch: feat/engine-local-source
---

# Session log

Item 22, "reading here: the local source" — the fourth ownership row from the
vision doc, built in a worktree as one of five parallel threads. No migration,
by design: `0014` is item 20's and `0015` is item 23's, and the one thing that
looked like it needed a schema change did not.

The prompt asked to be argued with. It got argued with, on its own nominated
line and on two more.

## The pushback: `source = 'local'` does not earn a reading

The prompt named the line to fight: *whether `source = 'local'` earns a reading
at all, or whether attaching a file and typing a page is just `update_progress`
with a different provenance stamp*. It is the second, and the disagreement
between the vision doc and the schema resolves in the schema's favour.

`readings.source` names the **writer of the row**. `koreader` is the sidecar
importer. `goodreads` is the CSV importer. `migrated` is migration `0005`
itself. `manual` is a person typing a number, here, by hand. Item 22 adds no
new writer: attaching a file opens no reading, and typing a page opens one
through `update_progress`, whose writer is a person typing a number here.
`local` on that column would have been a **synonym for `manual`**, and a synonym
is worse than no word at all — `readings_from_source` is the query every
importer's idempotency rests on, and it would then have had to know both.

The word `local` *did* earn is `reading_events.source`, where the vocabulary is
**claimants** rather than writers: `koreader` means the device said so, `vault`
means a note said so. "The user typed a page here today" is a genuinely new
claimant, and it is a claim neither `koreader` nor `manual` can make on that
user's behalf. The primary key `(book_id, day, source)` is arranged so two
claimants are two rows: a KOReader-sourced reading whose page you corrected this
afternoon carries a `koreader` row *and* a `local` row for the same day, and
both are true.

This is a one-word change to reverse if the wave lead disagrees. It is written
up as `docs/decisions.md` entry 22 with the argument in full, and
`Reading::source`'s doc comment points at it.

## Two more pushbacks

**Attach must not open a reading.** The spec said "a `source = 'local'` reading
opened on attach". Attaching five PDFs would then mark five books as ones you
are currently reading — `idx_readings_one_open` guarantees one open reading per
book, so the home screen fills with fabricated reading state. That is the same
class of invention `attribute_highlights` and `ko_statistics` refuse elsewhere.
A read is earned by a typed page, not by a file landing on disk.

**Migration `0005`'s vocabulary comment cannot be extended.** The brief said to
extend it rather than add a `CHECK`, and that instruction cannot be followed:
the `migrations` CI job refuses any migration that is modified, deleted or
renamed, which is the rule working exactly as intended. The comment has said
`manual|koreader|migrated` since before item 15 added `goodreads`, so it was
already stale. The vocabulary now lives in `Reading::source`'s doc comment,
beside the type every reader of the column goes through, and the SQL comment is
a historical note about what the list was in `0005`. Any future item told to
"extend the comment" should extend that one.

## The licence gate, run before the crate was chosen

`deny.toml`'s own argument is that a copyleft or source-available licence must
not arrive with a routine `cargo update`, and it is doubly true of one arriving
deliberately. So the gate ran first, against candidates, in a throwaway probe
crate carrying this repo's `deny.toml`.

| crate | licence | verdict |
|---|---|---|
| **`lopdf` 0.44.0** | **MIT**, whole transitive tree permissive | **chosen**, pinned `=0.44.0` |
| `pdf` 0.10 (pdf-rs) | MIT, tree permissive | rejected on weight |
| `mupdf` / `poppler` bindings | AGPL / GPL | not evaluated further |

`pdf` 0.10 drags in `jpeg-decoder`, `fax`, `snafu`, the unmaintained `md5` 0.7
and a second `syn 1.0` — a rendering stack, to answer two fields. `lopdf` has
`Document::load_metadata`, which reads the cross-reference table, the catalog
and `/Info` and never touches page contents.

That MIT matters more here than it would elsewhere: the engine already links
GPL-3.0 `epub`, which makes a distributed binary GPL-3.0 as a whole. A second
copyleft reader would make that situation *worse* rather than merely unchanged,
and `deny.toml`'s exception list is meant to stay at one deliberate entry.

Pinned exactly for `epub`'s reason, not out of superstition: this is a
**metadata** surface, and metadata is precisely the surface `epub` broke in a
patch release. `default-features = false` drops chrono, jiff, rayon and `time`.
What remains includes an AES/RustCrypto stack — PDF encryption is not
feature-gated upstream — so `cargo deny check bans` gains duplicate-version
warnings (`sha2`, `md-5`, `digest` on the 0.11 line where the engine is on 0.10;
`rand` 0.10; `syn` 3.0.3). `bans licenses sources` all report **ok**;
`multiple-versions` is `warn` in this repo.

## Two things measured rather than reasoned about

Both changed the code, and neither would have been found by thinking about it.

**lopdf returns `0` for seven different kinds of "could not tell."** Reading
`extract_page_count`'s source: no `/Root`, an unresolvable catalog, a catalog
that is not a dictionary, no `/Pages`, an unresolvable page tree, a page tree
that is not a dictionary, a reference cycle — plus a password-protected file,
whose page tree is behind the encryption. Every one of those is
indistinguishable from a document that genuinely has no pages. So `0` is
normalised to `None` once, at the boundary in `pdf.rs`, and the sentinel never
reaches a caller. A false denominator is the exact failure item 17 spent an item
removing, and nothing downstream can tell one from a real one.

**Two of three real PDFs return `Some("")` for `/Info /Title`.** Run against
`/Applications/TeX/README-hintview.pdf` (1.5), `READ ME FIRST.pdf` (1.7) and one
of Automator's icons (1.6): the first two have an `/Info` dictionary with an
empty `/Title`. `Some("")` survives every `Option` idiom, survives
`unwrap_or_default`, and lands as a book with a blank name. So emptiness folds
into `None` at the same boundary. The same run confirmed the other half: PDF 1.5
and 1.7 files — cross-reference streams, page tree inside a compressed object
stream — parse fine, which is the tier the synthetic fixtures cannot reach.

A title that still carries an authoring-tool extension (*Microsoft Word -
kant_final_v2.doc*) is refused too, and `files.rs`'s filename stem is the
fallback. The rule is **deliberately narrow** — a general "looks like a
filename" heuristic throws away books called *Sync* and *Java*.

## What was built

- **`crates/engine/src/pdf.rs`** — `epub_info`'s twin, and the item's one piece
  of genuinely new engine code. One rule, stated before the API: a length we
  could not read is `None`, never `Some(0)`.
- **`files::identify` reads a PDF**, `FileIdentity` grows `page_count`, and
  `attach_identified` writes it through **`Storage::fill_book`** — the *stored
  row wins* merge, chosen on `calibre.rs`'s rule that the pattern follows
  whether a record is complete. A page count already on the book is a claim
  about a specific edition; an attached PDF is one more partial record beside
  it, not an authority over it. It fills a gap and never overwrites an answer,
  the user's included.
- **`Source::Pdf` beside `Source::Epub`**, not one `File`. They answer different
  questions — an epub supplies a title, authors, a language and an ISBN and
  never a length; a PDF supplies a length and occasionally a title and never the
  rest — and "which file said 512 pages" is what a reader of `field_provenance`
  is asking.
- **The fifth `reading_events` filler**: `Storage::record_typed_page`,
  `source = "local"`, `confidence = measured`, called by `update_progress`
  itself. Before this the log knew the day a read opened and the day it closed,
  so a reader who typed a page every evening for six weeks had one event for it.
- **Per-reading progress on the wire** — see below.

## The three traps in the fifth filler

Each has an obvious implementation that looks right and is wrong.

- **A delta needs two points.** The first typed page files the day with
  `pages: None`. "You are on page 42" is not "you read 42 pages today" — you may
  have read it over a month — and this is the one place where the difference is
  a fabricated number rather than a missing one.
- **Backwards is a correction, not negative pages.** 200 down to 190 contributes
  nothing and erases nothing.
- **The day accumulates rather than replaces.** `EVENT_MERGE` is a `COALESCE`,
  so writing the evening's delta over the morning's would report twenty pages
  and lose the ten. The running total is read back and added to — which also
  makes re-typing the same page a genuine no-op, so `EVENT_DIFFERS` declines to
  touch the row and idempotency stays observable.

`a_days_pages_are_the_forward_moves_of_that_day` is the property, and it is a
property rather than more examples because all three failures are invisible in
any example where the reader only ever moves forward by a comfortable amount.

## The gap the audit found, and closed

`api-surface-auditor` was run against item 22's own screen before it was called
done. Four of five steps were servable; step three was not.

`ReadingDto` carried `current_page` and **no progress**, so a screen showing one
*named* reading had only `BookDto::progress` to reach for — which is
`Progress::of_book`, the current read's numbers, printed under an older read's
heading on any reread. `progress.rs` warns about exactly this in its own doc.
`Progress::of_reading` had been the right answer since item 17 and was reachable
only by a frontend that *links the engine*: the TUI calls it directly, the GUI
cannot. Its only remaining move was `current_page / page_count` above the API —
the row-state derivation `gui/CLAUDE.md` bans, walking into all three hazards
`Progress` exists to remove.

Closed rather than deferred, because it is squarely in this item's own files and
it is what makes item 22's screen renderable. The pairing is a **derivation** and
lives in the engine (`readings_with_progress` and two siblings): `readings` has
no page count, so a caller holding a `Reading` cannot reach `of_reading` at all,
and a caller holding both has to know which book's length goes with which read.
`From<Reading> for ReadingDto` was **removed** rather than made to guess —
filling the field with `of_reading(&r, None)` would report "no percentage" for
every book whose length is known perfectly well.

Two further gaps are recorded and left as narrow, migration-free later items:
`MatchCandidate` drops the author off a `Book` `koreader::band` is already
holding (so the chooser a refusal leads to cannot answer *which Dune is this*
without an N+1), and there is no per-book day aggregate for the activity log.

## Fixtures

Generated in-module (`synthetic_pdf`, behind the `internals` feature so the unit
and integration tiers share **one** definition of a valid fixture), for
`gen-kostats`' reason: a committed `.pdf` is the one fixture nobody can read a
diff of, and its bytes would depend on whichever writer produced it. The
cross-reference offsets are computed while the objects are written, because
`startxref` pointing at the wrong byte is how a "valid" fixture silently tests
the error path.

Realism is a gitignored drop-in at `crates/engine/tests/fixtures/pdf/real/`,
the same shape the KOReader `real/` directory has. Absent, it prints

```
SKIPPED: a_real_pdf_reports_a_plausible_length — no .pdf in …
```

and passes; with `READINGBUDDY_REQUIRE_FIXTURES=1` it **fails**. Both behaviours
were run, not assumed.

## Not built, deliberately

- **No embedded PDF viewer** — out of scope by `docs/gui/gui-vision.md`.
- **No fabricated highlights.** A locally-read PDF has none. KOReader probably
  cannot supply them either: `entry_to_highlight` requires a string `pos0` and
  PDF sidecars store a table there, so entries would be skipped in silence. That
  stays *unobserved* in `docs/koreader-format.md` and wants a real PDF sidecar in
  the corpus before anything is built on it.
- **No CLI or TUI surface**, which `files.rs` has never had.
- **No `readings.source = 'local'`** — the pushback above.

## Verification

From the worktree, `CARGO_INCREMENTAL=0` throughout: `make fmt`, `make lint`,
`make build-check`, `make test` (whole workspace), `make ts-check`, and
`cargo deny check bans licenses sources` → **bans ok, licenses ok, sources ok**.
`make ts` was run and `gui/src/lib/api/bindings.ts` committed with the DTO
changes. `make web-check` and `make routes` were **not** run and must not be
claimed: a worktree has no `gui/node_modules` and both degrade silently to
`SKIPPED:`. Full `make ci` is the wave lead's, on main, after merge.
