---
title: Item 22 — reading here: the local source
date: 2026-08-05
source: docs/gui/spec-gui-17-28.md item 22; docs/gui/gui-vision.md for the fourth
        ownership row; docs/decisions.md entry 17 for what Progress now answers
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Prompt — Item 22: reading here, the local source

Paste into a fresh session at the repo root, on branch `feat/engine-local-source`,
branched from `main`. Parallel-safe with items 18, 19, 20 and 24 — see *Launch
order* in `docs/next-thread-handoff.md`.

Read `CLAUDE.md` (**Engine standards** is binding), then
`crates/engine/CLAUDE.md` (owned files, importers, the vault), then item 22 in
`docs/gui/spec-gui-17-28.md`, then `docs/decisions.md`'s **Data ownership** and
**Files** sections.

**Engine, plus the API surface for it. No migration** — `0014` is item 20's and
`0015` is item 23's. That is not a constraint you have to fight: see below, the
one thing that *looks* like it needs a schema change does not.

## What the item is

The fourth ownership row from the vision doc: KOReader owns highlights and
reading state, calibre owns files, providers own bibliographic metadata — and
**readingbuddy owns what you read here**. A PDF you attached and are reading in
this app, with a page you typed.

**Smaller than it sounds, because most of it exists.**

## Already built — do not rebuild it

- `book_files` stores any format by lowercased extension, so `pdf` is already
  legal (`storage/book_files.rs`). Note the `format_of` sanitizer that decides
  this: it reduces an extension to ASCII alphanumerics, so `.PDF` and
  `.pdf.part` are its business and worth a test.
- `import_file` and `add_file_to_book` (`crates/engine/src/lib.rs`) already copy
  bytes in, content-addressed.
- `update_progress` already writes to the active reading, opening one where none
  is open.
- `notes.page` already exists and its doc comment already names the case —
  *"Device/pdf page the note anchors to"*.

**And the thing that looks like a migration and is not:** `readings.source` is
plain `TEXT NOT NULL DEFAULT 'manual'` with the vocabulary in a **comment**, not
a `CHECK` (`migrations/0005_readings.sql`). `'local'` needs no schema change.
That comment convention is deliberate and item 29 followed it again for
`field_provenance`; extend the comment, do not add a constraint.

## What is genuinely missing

- **PDF metadata extraction.** `epub_info` (`crates/engine/src/epub.rs`) has no
  PDF twin. This is the one real piece of new engine work. Page count is the
  point of it; title too, where the PDF carries one — most do not, and the
  **filename stem is the honest fallback**.
- **A `source = 'local'` reading**, opened on attach, and the API surface for
  typing a page into it.
- **Feeding `reading_events`** (item 21, landed) on each progress update: a day
  and a page delta, `confidence = measured`, because the user typed it. Read the
  `reading_events` section of `crates/engine/src/storage/CLAUDE.md` first — the
  fillers share one **no-clobber merge** (`EVENT_MERGE`), `confidence` ratchets
  to `measured` and never back (which is what makes the fillers commute), and the
  `DO UPDATE` carries `WHERE {EVENT_DIFFERS}` so `rows_affected` means *changed*
  rather than *seen*. You are adding a fifth filler, not a new pattern.
- **The attach flow's refusal path, rendered.** `import_file` refuses to create a
  book over a candidate match and returns `FileOutcome::Unmatched` **having
  written nothing**, unless `ImportOptions { new: true }`. This is the same
  refusal-with-a-next-move shape the device and calibre screens already have, and
  it is the *first* thing a user attaching a PDF of a book already in the library
  will hit. **It is not an error case to handle later.**

## What item 17 changed under you

- **`Progress` is now one value type** (`crates/engine/src/progress.rs`), and it
  is exactly the thing a PDF stresses. A PDF with no extractable page count is
  `Progress::Started { page: Some(n), of: None, fraction: None }` — the engine
  already says "there is no percentage" rather than dividing by a zero or a
  guess. **You do not need to add anything for that case; you need to not break
  it.** Specifically: if PDF extraction fails, write `NULL`, never `0`. A zero
  page count is a *false denominator* and item 17 spent an item establishing that
  absence is not zero. `a_reported_length_is_never_zero` is the property.
- **`Progress::of_reading(r, page_count)` exists** and takes the reading's own
  page. A screen showing one specific reading should use it rather than the
  `Book` projection.
- **`readings.status` is typed on the wire** (`ReadingState`, with `Other(raw)`)
  while the column stays a `String`. **`readings.source` is deliberately still a
  `String`** and item 17 recorded why: it is the *name of a writer*, it grows by
  one every time an importer is added, and nothing branches on it. You are the
  importer that grows it. Do not turn it into an enum on the way past — that
  would make a second list of importers to keep in step with the first.

## What must not be invented

- **Highlights.** A locally-read PDF has none and none are fabricated.
- Note KOReader probably cannot supply them either: `entry_to_highlight`
  (`crates/engine/src/koreader.rs`) requires a **string** `pos0`, and on PDF
  KOReader stores a *table* there, so the entry would be skipped **in silence**.
  That is stated as reasoning rather than observation on purpose —
  `docs/koreader-format.md` files PDF annotations under *unobserved*, since both
  PDF sidecars in the corpus have an empty `annotations` block. It is worth a
  `Diagnostic` rather than silence, and worth a real PDF sidecar in the corpus
  before anyone builds against the assumption.
- **An embedded PDF viewer.** Explicitly out of scope; see `docs/gui/gui-vision.md`.

## The dependency you are about to add, and the licence gate

A PDF metadata crate is a new dependency, and `deny.toml` is
**permissive-licences-only** with five named exceptions. `cargo deny check bans
licenses sources` is a CI job. Run it *before* you build on a crate, not after:
the whole point of that file is that *"a copyleft or source-available licence must
not arrive with a routine `cargo update`"*, and it is doubly true of one arriving
deliberately.

Two more constraints on the choice: **`epub` is pinned `=2.1.4`** because it
shipped a breaking metadata API change in a *patch* release, so prefer a
dependency you are willing to pin exactly; and it is GPL-3.0, which already makes
a distributed binary GPL-3.0 as a whole — do not make that worse by accident.

## Files you own

`crates/engine/src/pdf.rs` (new), `crates/engine/src/lib.rs`,
`crates/engine/src/storage/readings.rs`,
`crates/engine/src/storage/reading_events.rs`, `crates/api/`. Item 17 touched
`readings.rs` (it added `ReadNumbering` at the top); nothing else in this wave
does. **No collisions** with 18, 19, 20 or 24 beyond the `lib.rs` export list.

## Push back rather than comply

Four of five threads in the last wave did, and each time they were right. The
line to argue with: whether `source = 'local'` earns a reading at all, or whether
attaching a file and typing a page is just `update_progress` with a different
provenance stamp. The vision doc says it is an ownership row; the schema says it
is one word in a `TEXT` column. If those two disagree, that is worth surfacing
rather than splitting the difference.

## Done means

- `make ci` exit 0, and `cargo deny check bans licenses sources` clean.
- `make ts` run and `bindings.ts` committed with any DTO edit.
- The `cargo-tester` agent before you call it done.
- **A test with a real PDF fixture** — and if it must skip when the fixture is
  absent, it prints `SKIPPED:` and honours `READINGBUDDY_REQUIRE_FIXTURES=1`. A
  test that `return`s silently is green without asserting anything; `epub.rs` had
  two of those for months.
- **The corrections this build forced, written into `docs/decisions.md`.**
- A session log, via the `wrap-session` skill.
