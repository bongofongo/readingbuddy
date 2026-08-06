---
title: Items 17–28 — the engine the GUI needs, and the GUI
date: 2026-08-04
source: docs/gui-vision.md for the product argument; docs/decisions.md items
        1–16 for what came before
---

# Items 17–28

`docs/decisions.md` item 16 is a bucket, and "the Tauri GUI" is one line in it.
This is that line unpacked. Items 17–24 are engine; 25–28 are the GUI.

**The organising claim:** the GUI is not blocked on anything hard. It is blocked
on a dozen small things that are each individually trivial and collectively
decide whether frontend development is assembly or archaeology. The audit that
produced this list found the engine strong and the *derived* layer missing —
every frontend so far has written its own, and there are already three
independent spellings of "how far into this book am I" across two frontends,
plus a fourth that wraps one of them.

**One settled decision is overturned here**, and `docs/decisions.md:230` needs
editing rather than ignoring: "Shelf view" is listed under *Out of scope for
now*, and item 26 makes it the home surface. The argument is in
`docs/gui-vision.md`; the short version is that the ruling was made against a
shelf that grouped by collection, and this one groups by nothing. "Author/corpus
view" on the same line partially moves with it (17a). "Graph view" and "Orphan
queue" stay out.

**The constraint that shapes the whole wave:** the TUI and the GUI must be
developable independently. That is a constraint on the engine, not on either
frontend. Anything both need lives below both. Item 17 is that item, and it is
first for that reason.

**Migration numbers are pre-allocated** and merge in numeric order. They were
**reshuffled on 2026-08-04** when items 29–32 (`docs/spec-engine-29-32.md`) took
the front of the queue and item 21 moved into that wave: `0011` to item 21,
`0012`/`0013` to items 29/32, then **`0014` to item 20 and `0015` to item 23**.
Nothing else in this wave takes one. The reshuffle is free because nothing had
been built against the old numbers, and it is necessary because the contiguity
test refuses a gap.

**Reshuffled again on 2026-08-06**, for the same reason and with the same
freedom: the non-GUI wave (items 34–38,
`docs/handoff-orchestrator-non-gui-wave.md`) builds *before* item 23 and takes
**`0015` (item 34, highlight FTS) and `0016` (item 35, sort-key indexes)**, so
**item 23 moves to `0017`**. A wave that landed `0016` while `0015` sat
unwritten would leave `main` red on the contiguity test —
`migration_versions_are_contiguous_from_one` fails on a *gap* as well as on a
duplicate. Per `CLAUDE.md`, an applied migration is never edited, and
`migration_versions_are_contiguous_from_one` is what catches two threads both
claiming a number.

---

# The engine wave

## Item 17 — the derived-facts layer

**The single most valuable item in the wave, and the one that makes "work on the
GUI without touching the TUI" true rather than aspirational.**

`crates/cli/src/commands/goodreads.rs:4` states the rule the codebase has
followed: *"All the printing lives here — the engine does no terminal I/O."*
That rule is right and stays. It has been over-applied to *derived facts*. The
engine hands back typed reports and each frontend independently writes the
sorting, the arithmetic, the state vocabulary and the selection policy — so a
second frontend does not extend the app, it re-derives it.

The fix is not to move formatting into the engine. It is to move the **values**
into the engine and leave the *rendering* of them in the frontends. A `Progress`
enum is not terminal I/O; `"p.42"` is.

What moves, in priority order:

**17a. Library sort.** `BookSort` (`crates/engine/src/storage/books.rs:11`) has
`LastModified | Title | Progress`. The TUI defines its own `Sort`
(`crates/tui/src/ui/library.rs:26`) with `Recent | Title | Author | Year` and
applies it **in Rust after fetching 200 rows** (`crates/tui/src/app.rs:1005`).
So `Author` and `Year` do not exist in the engine at all, and `Progress` is
unreachable from the TUI. The CLI accepts a third vocabulary
(`crates/cli/src/commands/book.rs:35`).

`Author` drags a name-parsing library with it that must not be written twice:
`author_key` (`ui/library.rs:103`), `PARTICLES` — 18 entries, de/van/von/ibn/…
(`:118`), `SUFFIXES` (`:124`), and `last_name()` (`:144`), which handles
`Surname, Given` inversion, suffix stripping, particle attachment, mononyms, and
the case where the comma holds only a suffix (*Martin Luther King, Jr.* → King).
A GUI without this files *The Overstory* under nothing.

Move `last_name` and its tables into the engine, add `Author` and `Year` to
`BookSort`, and sort in SQL. A `sort_author` column computed on write is the
obvious follow-on but is **not** required by this item — do the simple thing
first and measure.

**Do not sort in SQL and paginate naively in the same breath.** The TUI's
200-row fetch is a decision with a reason written at the call site
(`crates/tui/src/app.rs:1006`): *"A SQL `ORDER BY … LIMIT 200` would make the
sort key decide which 200 books are on screen, so pressing `s` would swap the
contents of the list rather than reorder it."* That is exactly the bug item 18's
pagination reintroduces if the two items are not designed together. The shelf
wants the whole library anyway, which is the resolution — but the reasoning has
to be stated, not stumbled into.

**17b. Progress.** Three independent implementations, plus a wrapper, one of
them with a divergent guard:

| | rule |
|---|---|
| `crates/tui/src/ui/library.rs:281` | `done` / `{pct}%` / `p.{p}` / `""` — guards `total > 0` |
| `crates/tui/src/ui/home.rs:114` | *delegates* to the above, **then falls back to `Reading::ko_percent`** |
| `crates/tui/src/ui/book.rs:135` | `finished` / `{p} / {t} · {pct}%` / `page {p}` / `not started` |
| `crates/cli/src/render.rs:56` | `[finished]` / `[p/t]` / `[p.{p}]` — **no `total > 0` guard** |

The engine gains one `Progress` value type covering the cases, including the
`ko_percent` fallback — which is real domain knowledge (*a device percentage
stands in for a page we do not have*) currently living in one screen of one
frontend. Frontends keep their own rendering of it.

The CLI's missing guard is **not** a divide-by-zero — it never computes a
percentage, so `page_count = 0` renders `[12/0]`, a false denominator rather
than a crash. Both sites that do divide guard correctly. Fix it here anyway.

**The design question this item must answer, and the reason these were all so
cheap to write:** `Book` carries `current_page`, `finished`, `date_started` and
`date_finished` as **read-only projections of the current reading**, resolved by
a `LEFT JOIN` in `BOOK_COLUMNS`/`BOOK_FROM`. Every one of the sites above reads
the projection, not `readings`. So `Progress` has to decide whether it
takes a `Book` — convenient, matches every existing call site, and silently
loses per-reading truth on a reread — or a `Reading`, which is correct and
changes every call site. Probably both, with the `Book` form documented as *the
current reading's progress* rather than *the book's*.

**17c. Read-number attribution.** `BookView::read_number`
(`crates/tui/src/app.rs:243`) and `shows_read_gutter` (`:259`) map a highlight's
`reading_id` to a 1-based index into `list_readings`, suppress the column when
there is one reading, and render `·` for unattributed. `decisions.md` calls this
out as load-bearing — *"a column that nothing renders is a claim nothing can
check"* — and it silently depends on `list_readings`' ordering contract. It is a
domain rule sitting in frontend state.

**17d. Calibre row state.** `calibre_state_for` (`crates/tui/src/app.rs:600`)
joins one calibre row id against `CalibreReport.books`, `.unmatched` and
`.warnings`, and *invents a synthetic diagnostic* when a row appears in none of
the three. That is a data-model join, and `CalibreReport` should hand back
per-row states directly. A GUI that reimplements it inexactly gets a shelf that
disagrees with the TUI's about the same library.

**17e. Selection predicates.** The sweep rules (`sync_marked`
`crates/tui/src/app.rs:1900`, `sync_calibre_marked` `:2314`) encode which rows a
"sync everything" may touch — notably that calibre `Candidates` rows are never
swept and device `Unreadable` rows are pre-refused rather than allowed to error.

Only half of this is a move. `is_syncable` is **already in the engine**
(`crates/engine/src/device.rs:178`); `is_importable`
(`crates/tui/src/app.rs:320`) is the calibre predicate and is the one that
migrates. So the device half of this item is not "move the predicate" but
"check the sweep reads the engine's predicate rather than re-deriving it" —
which is the smaller and more useful job.

The queue-and-pump machinery stays in the TUI, where it is a workaround for a
single-threaded draw loop and not a domain requirement.

**Explicitly not moving: prose.** `calibre_gains` (`app.rs:657`), `book_detail`
(`ui/goodreads.rs:149`), `detail` (`ui/device.rs:102`) and their CLI twins stay
where they are. They are duplicated, and that is the correct kind of duplication
— pluralisation and phrasing are frontend business. What they should share is
the *counts* they phrase, which items 17b–17d already deliver.

**Migration:** none. **Blocks:** nothing hard, but every GUI item is cheaper
after it. **Parallel with:** 18, 19, 20, 22, 24.

**The TUI is not required to migrate.** It keeps its implementations until it
wants them gone. That is what independence means here, and it is why this item
can land without a TUI thread.

---

## Item 18 — list endpoints that survive a real library

`ListBooks{limit, sort}` is the whole of it. There is no offset, no cursor, no
filter, and nothing anywhere returns a count. `list_books`
(`storage/books.rs:319`) is `ORDER BY … LIMIT ?` and that is all. The TUI copes
by fetching 200 and filtering in Rust (`app.rs:1005`); a shelf cannot.

- **Pagination.** Offset or keyset — but note that keyset is not simply
  available: `BookSort::Progress` already orders by
  `CAST(cur.current_page AS REAL) / NULLIF(books.page_count, 0)`
  (`storage/books.rs:323`), a computed value across a `LEFT JOIN`, which is not
  a stable cursor key. Either that sort keeps offset pagination while the others
  get keyset, or the ratio is materialised. Decide it explicitly.
- **Counts.** `ListBooks` today cannot answer "how many books" without
  returning all of them. A shelf needs the number before it needs the rows.
- **Filters.** Status (reading/finished/abandoned/unstarted), author, year,
  language, tag, has-cover. Every one of these is a `WHERE` clause the TUI
  currently cannot ask for.
- **Notes.** `ListNotes{book_id?}` has **no limit at all**. That is a full table
  scan into a `Vec` on a screen that shows twelve rows.

And read 17a's warning about pagination-versus-sort before designing the cursor:
a limit applied in SQL makes the sort key decide *membership*, not just order,
which is the bug the TUI's 200-row fetch exists to avoid.

Also here, because it is the same shape of gap: `find_books_by_title`
(`storage/books.rs:307`) is a plain `LIKE`, and **highlights and annotations are
in no FTS index** — `notes_fts` (`migrations/0001_init.sql:77`) is the only
virtual table in the repo. A GUI with one search box that searches notes but not
highlights will be reported as a bug, correctly.

**Migration:** none (an FTS index over highlights would take one — split it out
rather than smuggling it in). **Parallel with:** everything.

---

## Item 19 — the shape of an edition, in the engine

`Model::new` (`crates/tui/src/render3d/mod.rs:121`) is nine lines, four of them
arithmetic, and derives a book's physical half-extents: width from the cover's
aspect clamped to `0.55..0.85`, thickness from `page_count` defaulting to a
320-page paperback, clamped `48..1400` pages to `0.05..0.20`. That is not
rendering. That is the answer to *what shape is this edition*, and it belongs in
the engine so a WebGL shelf and a Unicode-glyph book agree about how fat
*Infinite Jest* is.

Move the derivation. Leave the renderer alone — `decisions.md` freezes it, and
freezing it is right.

**The dependency this exposes:** `Model::new` takes a `&Cover`, and
`Cover::aspect` is computed by **decoding the image**
(`render3d/texture.rs:67-73`, format-agnostic). The engine stores no cover
dimensions — no migration defines a `width`, `height` or `accent` column. A
shelf of three hundred spines would decode three hundred images to find out how
wide to draw them. That is item 20.

**Migration:** none here. **Depends on:** 20 for the aspect to come from a
column rather than a decode. Can land first with an aspect parameter and be
rewired.

---

## Item 20 — covers a grid can actually use — migration `0014`

Four things, one migration.

**20a. The filename collision is real, not theoretical.**
`filename_from_url` (`crates/engine/src/images.rs:17`) names the file from the
URL's last path segment, falling back to the literal `"cover.jpg"` when there
isn't one, and `image_from_url` writes it to `images_dir.join(fname)`. Two books
whose provider URLs end in the same segment overwrite each other, and the
fallback makes that reachable rather than merely possible. The path-traversal
guard around it is careful and well-tested; the collision beside it is not
guarded at all. **Name by content hash**, keep the original as a column, exactly
as `book_files` already does for book bytes (`migration 0010`).

**20b. Store what the renderer currently decodes.** Add cover `width`,
`height` and `accent` to `books`. The accent is derived from the image border
(`accent_from_border`, `render3d/texture.rs:91`) and is what gives a spine a
colour when there is no spine art — which is every book, since providers ship
front covers only. Computing both once at download time turns a shelf render
from N image decodes into one query. Back-fill for covers already on disk;
`database/images/` has real files in it.

**20c. Size tiers.** OpenLibrary is pinned to `-M`
(`providers/openlibrary.rs:49`) and Google Books to `thumbnail` with a
`smallThumbnail` fallback (`providers/googlebooks.rs:149`). Both are too small
for a cover-forward hero shot on a retina display. Ask for the larger sizes and
generate a thumbnail tier locally for the shelf.

**20d. Not a gap — recorded so it is not re-investigated.** EPUB cover
extraction **already exists and is wired**: `epub::extract_cover`
(`crates/engine/src/epub.rs:41`), called from `Engine` at
`crates/engine/src/lib.rs:484`.

**Migration:** `0014`. **Parallel with:** everything except 19, which wants it.

---

## Item 21 — `reading_events`, the source-agnostic activity log — migration `0011`

> **Moved.** Built as part of items 29–32 (`docs/spec-engine-29-32.md`), because
> item 31 needs somewhere to put reading time and the settled answer is that no
> source is consumed in its own shape. The design below is unchanged and is
> still the authority. Its `statistics.sqlite3` filler is **item 31**, not item
> 15 — that deferral was about the plugin, and reading a file off an
> already-mounted volume is what `scan_device` does today.

The problem this exists to solve, stated plainly: **reading-time data is
KOReader-only**, and a surface built directly on it opens to blanks for a reader
whose library came from a Goodreads CSV.

The answer is not to skip the surface. It is to stop consuming any source in its
own shape. One table:

```
reading_events(
  book_id, reading_id, day,      -- the grain: a book, a read, a date
  minutes,                       -- nullable
  pages,                         -- nullable
  source,                        -- koreader | vault | goodreads | local | …
  confidence                     -- measured | inferred
)
```

Fillers available **today**, without touching a device:

| filler | supplies | confidence |
|---|---|---|
| `highlights.ko_datetime` | a day you were in the book | inferred |
| `notes.created_at` | a day you thought about it | measured (ours) |
| `readings.started_at` / `finished_at` | the endpoints | measured |
| local reading (item 22) | day + page delta, typed by the user | measured |

And later, changing nothing downstream: KOReader's `statistics.sqlite3` fills
`minutes` and `pages` with `confidence = measured`. Per the vision doc that
lands with **item 15**, the plugin work — not here. What matters is that it
lands as one more filler of an existing table, so no view, no query and no line
of GUI changes when it arrives. Kobo and Kindle likewise.

**The vault is the filler worth dwelling on.** Notes, reflections, annotations
and citations all carry timestamps, and they are data readingbuddy fully
originates. No importer can fail to supply them. It is also the *right* signal
for this app to reward, since the positioning is the desk rather than the
reader.

On top of the table, the aggregation queries the GUI needs and nothing currently
has — there is not a single reading-stats aggregate in the engine today, only
invariant checks: books finished per period, activity days per period, notes and
links created per period, pages and minutes where known.

**Every aggregate must be able to say it does not know.** A month with no
device data returns absent minutes, not zero. This is the same discipline as
`goodreads_for` returning `None` and surfacing `ErrorCode::UnmappedRating`
rather than rounding, and as Goodreads import refusing to invent a start date.
Zero is a claim.

**Migration:** `0011`. **Depends on:** nothing. **Blocks:** 23, 28, and 31.

---

## Item 22 — reading here: the local source

The fourth ownership row from the vision doc. Smaller than it sounds, because
most of it exists.

**Already built:** `book_files` stores any format by lowercased extension, so
`pdf` is already legal (`storage/book_files.rs:22`) — note there is a `format_of`
sanitizer deciding that, which reduces an extension to ASCII alphanumerics, so
`.PDF` and `.pdf.part` are its business and worth a test; `import_file`
(`lib.rs:517`) and `add_file_to_book` (`:527`) already copy bytes in
content-addressed; `update_progress` (`:408`) already writes to the active
reading; `notes.page` already exists and its doc comment already names the case
— "Device/pdf page the note anchors to" (`notes.rs:70`).

**Also already fine, and worth recording because it looks like it would need a
migration and does not:** `readings.source` is plain `TEXT NOT NULL DEFAULT
'manual'` with the vocabulary in a **comment**, not a `CHECK`
(`migrations/0005_readings.sql:19`). `'local'` needs no schema change.

**What is genuinely missing:**

- **PDF metadata extraction.** `epub_info` (`epub.rs:19`) has no PDF twin, so an
  attached PDF has no page count and every progress display degrades to
  `p.{page}` with no denominator. This is the one real piece of new engine work
  the source requires. Title too, where the PDF carries one — most do not, and
  the filename stem is the honest fallback.
- A `source = 'local'` reading opened on attach, and the API surface for typing
  a page into it.
- Feeding `reading_events` (item 21) on each progress update: a day and a page
  delta, `confidence = measured`, because the user typed it.
- **The attach flow's refusal path, rendered.** `import_file` (`lib.rs:513`)
  refuses to create a book over a candidate match and returns
  `FileOutcome::Unmatched` having written nothing, unless `ImportOptions { new:
  true }`. This is the same refusal-with-a-next-move shape the device and
  calibre screens already have, and it is the *first* thing a user attaching a
  PDF of a book already in the library will hit. It is not an error case to
  handle later.

**What must not be invented:** highlights. A locally-read PDF has none and none
are fabricated. Note that KOReader probably cannot supply them either —
`entry_to_highlight` (`koreader.rs:263`) requires a string `pos0`, and on PDF
KOReader stores a table there, so the entry would be skipped in silence. Stated
as reasoning rather than observation on purpose: `docs/koreader-format.md:482`
files PDF annotations under *unobserved*, since both PDF sidecars in the corpus
have an empty `annotations` block. Worth a diagnostic rather than silence, and
worth a real PDF sidecar in the corpus before anyone builds against the
assumption.

**Explicitly out of scope: an embedded PDF viewer.** See the vision doc.

**Migration:** none. **Parallel with:** everything.

---

## Item 23 — moments — migration `0017`

A moment fires once. That requires knowing it fired, which is state the app
keeps about itself rather than a number shown to anyone.

- Derive candidate moments from what already happened: a reading closed, a first
  annotation on a new book, a reflection that reached a book it had not reached
  before, a run of activity days that ended.
- Record which have been surfaced, so the ceremony does not replay on every
  launch.
- **Nothing here is a threshold announced in advance**, and nothing counts down.
  A moment is recognised retrospectively or it is a target wearing a costume.

The awkward case, and it should be decided rather than discovered: a fresh
install importing a 400-book Goodreads CSV would mint 400 moments at once. The
answer is that a moment fires only for events that occur **after** the book is
in the library — an import is history arriving, not a thing you just did. The
cards, though, are minted for all 400, because the shelf is the history.

**Migration:** `0017` (was `0015`; items 34 and 35 took `0015`/`0016` on
2026-08-06). **Depends on:** 21.

---

## Item 24 — vault coherence

**Smaller than it looks.** `Engine::refresh_note_from_disk` (`lib.rs:740`) is
not an orphan — it has an API facade method (`crates/api/src/lib.rs:365`), a
request variant (`RefreshNoteFromDisk`, `protocol.rs:235`) reachable over the
socket today, and a test named
`refresh_note_from_disk_reindexes_an_external_edit`. What does not exist is
anything that calls it **automatically**: no frontend issues it and nothing
watches the vault. So edit a note in Obsidian — which `decisions.md` explicitly
supports as a courtesy — and `notes_fts` stays stale indefinitely. The search
box the GUI is about to add is what makes this visible.

The item is therefore a watcher, not a wiring job. `crates/engine/src/watch.rs`
is already wired for mounts (`crates/tui/src/app.rs:3883`), and its debounce is
the right shape to reuse; note its module doc's constraint (*it may scan; it may
not sync* — it holds no `Storage` at all), so the vault watcher either follows
that pattern or consciously departs from it, and says which. A vault watcher
that refreshes the index is a *write*, so this is a real departure and needs an
argument, not a copy.

One known gap to leave known: `notes.title` is not unique, so an edge resolved
to one of two same-titled notes dangles again if that one is deleted. `item 9`
pinned this deliberately; it is write-side and this item does not paper over it.

**Migration:** none. **Parallel with:** everything.

---

# The GUI wave

## Item 25 — scaffold, and the seam

Tauri + Svelte, per `decisions.md`. The Rust backend links **`readingbuddy-api`
in-process**, behind a client trait, so `readingbuddyd` drops in later without
touching a line above the trait.

Two rules that matter more than the scaffold:

- **Every call goes through the API vocabulary**, even though the engine is
  right there. A gap in the API surface must be a compile error rather than a
  temptation. CI already guards the sibling case — the plain
  `cargo check --workspace` job exists precisely to catch a frontend reopening
  `Engine::storage()` through the `internals` feature. This deserves the same
  discipline.
- **The GUI is the API crate's first *semantic* in-repo client.** `readingbuddyd`
  links it, but deliberately never names a method — it is a byte pump. So the 77
  request variants in `crates/api/src/protocol.rs` are exercised only by
  `crates/api/tests/api.rs`. Expect to find gaps; each one is an item-18-shaped
  fix in the engine, not a workaround in the frontend.

Known holes to design around rather than rediscover: covers and paths cross as
strings, not bytes, so the GUI reads `images_dir` off `Paths` and loads from
disk; there is **no push channel**, so anything background is polled; and
`insert_flashcard` exists on storage only (`storage/flashcards.rs:17`) with the
KOReader importer as its sole caller — **the GUI cannot create a flashcard**
until a facade method exists.

## Item 26 — the shelf

The home surface. Spine-out, thickness from item 19, colour from item 20b.

**Render in WebGL, not through the Rust ray tracer.** `render_rgba`
(`render3d/raster.rs:174`) is clean and public and would work — but it exists
because a terminal has no GPU, and a shelf is an interactive camera over
hundreds of objects, which is what a canvas is for. What crosses is the *model*,
which is item 19 and is four lines of arithmetic. `decisions.md` says the
renderer survives the frontend change intact; it survives by not being needed,
which is the same thing.

Currently-reading pulled proud. Selection slides a book out and turns it
cover-forward — the existing single-book scene, so it is one camera path rather
than a screen transition. No number anywhere on this surface.

## Item 27 — the book, and the notes

The book view: info, notes, highlights, and the links pane. The TUI's `ui/book.rs`
is the reference for what belongs here, and its choices are mostly right — the
links pane replacing the note list *in place* rather than opening a modal is the
axiom's "nothing is modal-by-default" doing real work.

Two things the TUI never built and this must:

- **Note search.** `SearchNotes` and `notes_fts` have existed since migration
  `0001` and have no interface outside the CLI. A full-text index over
  everything you have ever thought, with no search box.
- **Citations.** `Cite` / `CitationsFor` exist and `rb cite` surfaces them
  (`crates/cli/src/commands/reflect.rs:184`). No TUI surface, and citing a
  highlight from the passage you are looking at is the natural gesture a mouse
  and a book view make available.

## Item 28 — the chain, and the reading-life page

The moment (item 23) → the card → the shelf (item 26). The card is **per
reading**, so a reread mints a second one beside the first. The moment ends by
opening the reflection; it does not end in a dialog.

The reading-life page is the one place counts are allowed to appear, because it
is a place you chose to go. Everything on it is past tense. It renders the item
21 aggregates, including their absences: a month with no device data says so
rather than showing a zero.

---

# Order

```
17 (derived facts)  ─┐
18 (list endpoints) ─┤
20 (covers, 0014)   ─┼─ all parallel, no shared files
21 (events, 0011)   ─┤  — moved to items 29–32; see spec-engine-29-32.md
22 (local source)   ─┤
24 (vault watch)    ─┘
        ↓
19 (edition shape)     — wants 20b's stored aspect
23 (moments, 0017)     — wants 21
        ↓
25 (scaffold)
        ↓
26 (shelf) ── 27 (book + notes) ── 28 (chain + reading life)
```

Migrations merge in numeric order: `0011` (21), `0012` (29), `0013` (32),
`0014` (20), `0015` (34), `0016` (35), `0017` (23).

**What is deliberately not in this wave**, so it is not quietly added: KOReader
`statistics.sqlite3` (item 15, with the plugin); goals of any kind (decided
against); graph view; note tags; collections; an embedded reader; new importers
(Kobo, Kindle, Storygraph); export beyond what exists — though export is named
in the vision doc as the weakest part of the system and should be the wave after
this one.

# Rules carried from the last wave

- One PR per thread, green CI, human review, nothing auto-merges.
- Pre-allocate migration numbers; never edit an applied migration.
- **Tell each thread to push back rather than comply.** Four of five did last
  wave and each time they were right. Items 17 and 21 in particular are designs
  made from an audit rather than from writing the code, and the thread that
  writes them will know something this document does not.
