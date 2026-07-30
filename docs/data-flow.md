# Data flow

Every read and write in the system as it stands today: what comes in from
outside, what readingbuddy originates itself, and what leaves. This is a
description of the code, not a plan — where something does not flow, that is
stated as a fact rather than as a gap to be filled.

Companion documents: `docs/decisions.md` (what is settled), `docs/ux-positioning.md`
(why), `CLAUDE.md` (architecture). The rules here are enforced in
`crates/engine/src/`; file and function names are given so a claim can be checked.

---

## 1. The two stores

State lives in two places and neither is authoritative over the other.

```
data root (--data-dir | READINGBUDDY_DATA_DIR | $PWD)
├── database/app.db          SQLite. Every row. Migrated on connect.
├── database/images/         Cover bitmaps. Referenced by books.cover_path.
├── database/files/<ab>/     Owned ebook files, content-addressed:
│                              <sha256>.<ext>, sharded on the first hex byte.
└── vault/<book-slug>/       Note markdown. One file per note, frontmatter +
                             body, Obsidian-openable. Referenced by
                             notes.file_path (relative).
```

The database is the index; the filesystem holds the bytes. Three consequences
that recur throughout:

- **A note's prose is on disk, not in the DB.** `notes` holds metadata; the body
  lives in the vault file. `notes_fts` is a *searchable cache* of the body, kept
  in step by delete+insert on every save (`storage/notes.rs::refresh_note_body`).
  Editing a vault file in Obsidian therefore leaves the FTS index stale until
  `Engine::refresh_note_from_disk` re-reads it.
- **A file's address is derived, never stored.** `Engine::file_path` computes
  `files_dir/<ab>/<sha256>.<ext>` from the row. There is no path column that
  could disagree with the content hash.
- **Deleting a row does not delete bytes.** Cascades handle rows;
  `Engine::delete_book` and `Engine::remove_file` explicitly `remove_file` the
  cover and owned files, and `Engine::merge_books` deletes the orphaned cover the
  storage layer reports back.

---

## 2. Table inventory — who may write what

| Table | Written by | Merge rule |
|---|---|---|
| `books` | providers, epub, calibre, goodreads, sidecar pull, user | **provider no-clobber** (`MERGE_RULES`) |
| `readings` | user (progress/reread), KOReader import, goodreads import | ours; device mirror columns straight-assigned |
| `highlights` | KOReader import only (`text`/`pos*`/`ko_*`/`color`/`chapter`/`page`) | **device straight assignment** |
| `highlights.annotation` | user only | never touched by import |
| `notes` + vault file | user; goodreads import (review / private notes) | prose already written here is kept |
| `note_links` | derived from note bodies on every save | replace, not merge |
| `notes_fts` | derived from note bodies on every save | delete + insert |
| `flashcards` | KOReader import (single-word highlights) | `UNIQUE(book_id, word)`, insert-or-skip |
| `device_books` | epub import, file import, calibre import, KO import (`auto`); user (`manual`) | `auto` never overwrites; `manual` repoints |
| `sidecar_seen` | device scan | cache of the *parse*, never of the verdict |
| `book_files` | `Engine::import_file` / `add_file_to_book` | `sha256` PK — identical bytes are one row |
| `book_tags` | goodreads, calibre | inert provenance, insert-or-skip |
| `external_ids` | goodreads (`Book Id`), calibre (uuid) | `(source, external_id)` PK, repoints on conflict |
| `rating_scales` / `rating_map` | seeded by migration; user via `rating scale|map` | explicit lookup, never a formula |
| `review_ratings` | user; goodreads import (`goodreads` scale only) | raw value + scale id, never the mapped integer |
| `citations` | user (`cite` / `uncite`) | by reference, `(note_id, highlight_id)` |

**Three merge patterns, and choosing between them is the recurring decision.**

1. **Provider no-clobber** — `MERGE_RULES` in `storage/books.rs`, shared verbatim
   by `upsert_book`'s `ON CONFLICT DO UPDATE` and `enrich_book`'s plain `UPDATE`.
   NOT NULL text keeps what it has unless the incoming value is non-empty; NOT
   NULL JSON lists likewise unless the incoming is not `[]`; everything nullable
   is `COALESCE(new, old)`. Used when the incoming record is **partial** — a
   provider hit, a `calibredb list` row (which carries no page count at all), a
   Goodreads row.
2. **Device straight assignment** — `refresh_device_fields` (highlights) and
   `set_device_state` (readings). A sidecar is the device's *complete* state, so
   a missing note means the user deleted it and `COALESCE` would resurrect it.
   Predicate lives once in `DEVICE_FIELDS_DIFFER`, shared with the dry-run
   preview so a preview cannot disagree with the import it previews.
3. **Merge-books, `dst` wins** — `Storage::merge_books`, the inverse of (1).
   `src` only fills what `dst` does not have, because `dst` is the row the user
   chose to keep.

The pattern is chosen by **whether the record is complete**, not by who owns it.
calibre is named an origin for curated metadata in `docs/decisions.md` and still
gets pattern (1), because a `calibredb list` row is partial.

---

## 3. Ownership: who is the origin of what

readingbuddy keeps a durable local copy of everything and is the *origin* of
almost none of it. The columns say so.

| Field | Origin | Ours |
|---|---|---|
| highlight text, `pos0`/`pos1`, `ko_datetime` | KOReader | — |
| `highlights.ko_note` | KOReader | `annotation` beside it |
| `highlights.color`, `chapter`, `page` | KOReader | — |
| `readings.ko_status` / `ko_percent` / `ko_rating` | KOReader | `status`, `current_page`, `started_at`, `finished_at` |
| ebook file bytes | calibre / the user | a copy in `book_files` |
| bibliographic metadata | OpenLibrary / Google Books / calibre | edits via `save_book` |
| shelves | Goodreads / calibre | recorded in `book_tags`, read by nothing |
| notes, reflections, reviews, ratings, citations | **readingbuddy** | — |

`highlights.last_seen_ko_note` exists and is written but read by nothing today:
a future two-way sync cannot tell "changed here" from "changed there" if only
the merged result is ever stored, and back-filling it afterwards would be a
guess.

---

## 4. Inflow — the whole map

```
                       ┌──────────────────────────────────────────┐
  OpenLibrary ────┐    │                                          │
  Google Books ───┼──► │  search.rs   federated fan-out, dedup,   │
                  │    │              field-wise merge, rank      │
                  │    └───────────────┬──────────────────────────┘
                  │                    │ Book (no id yet)
                  │                    ▼
  .epub file ────►│  epub.rs ──► lookup_isbn ──► upsert_book ──► books
                  │                                 ▲  ▲
  file (any fmt) ─┼─ files.rs identify ─────────────┘  │  + book_files + bytes
                  │      sha256 / partial_md5 / ISBN   │  + device_books(auto)
                  │                                    │
  KOReader .sdr ──┼─ koreader.rs parse (mlua) ─────────┤  + highlights
                  │      match: md5 → sibling ISBN →   │  + flashcards
                  │             title+author band      │  + readings (device
                  │                                    │    mirror + status)
                  │                                    │  + device_books(auto)
  mounted device ─┼─ device.rs scan (read-only)        │
                  │      sidecar_seen cache            │
                  │      → sync_device = N × pull ─────┤
                  │                                    │
  calibre lib ────┼─ calibre.rs calibredb list ────────┤  + external_ids(uuid)
                  │      match: uuid → ISBN →          │  + book_tags
                  │             partial_md5 → title    │  + device_books(auto)
                  │                                    │  + cover copy
  Goodreads CSV ──┼─ goodreads.rs parse ───────────────┘  + external_ids
                  │      match: Book Id → ISBN13 →        + book_tags
                  │             ISBN10 → title            + readings
                  │                                       + review note + rating
  the user ───────┘  save_book / update_progress / reread / notes / annotation
                     / cite / rating / link_* / merge_books
```

### 4.1 Providers (`search.rs`, `providers/`)

Read-only against the network; writes nothing by itself. Fan-out with a 5s
per-provider timeout, a provider failure becomes a `Diagnostic` rather than an
abort. Dedup keys on canonical ISBN-13, else a fuzzy title+author fingerprint.
Field-wise merge is fixed: OpenLibrary wins isbn/pages, Google Books wins
description/language. `rank()` is pure (exact ISBN 1000 ≫ title 40 > author 25 >
publisher 10).

The **CLI/TUI** turn a result into a row: `download_cover` (fetches the bitmap
into `images/` and writes `cover_path`) then `save_book` → `upsert_book`
(`crates/cli/src/commands/search.rs:95`).

### 4.2 Owned files (`files.rs`)

`identify` is read-only and computes both hashes: **sha256** (content address)
and **partial_md5** (KOReader's twelve-window sampling hash — good for matching,
wrong as a content address; both are computed and neither substitutes).

`store` is crash-safe by construction: one pass copies and hashes into a temp
file *inside* `files_dir`, then `rename`s — so a name can only ever be a name
for bytes that were fully written and flushed. `format_of` sanitizes the
extension (it becomes a path component) down to ASCII alphanumerics or `bin`.

### 4.3 KOReader (`koreader.rs`, `device.rs`, `watch.rs`)

Sidecars are Lua, evaluated in a sandboxed `mlua` VM (`StdLib::NONE`, 5M
instruction budget). Both formats are handled — modern `annotations`, legacy
`highlight`+`bookmarks`. `metadata.*.lua.old` backups are excluded by suffix:
KOReader writes one on every flush and it is a *previous* state, so importing it
would resurrect deleted highlights.

A **scan** (`device.rs`) writes only `sidecar_seen`; it never touches the
library. A **sync** is N × pull. A mount **watcher** (`watch.rs`) may scan and
holds no `Storage` at all, so "read-only on arrival" is a property of the code
rather than a rule about it.

### 4.4 calibre (`calibre.rs`)

Two tiers over calibre's own command line: `ebook-convert` and
`calibredb list --for-machine`. No new dependency, no linking (shelling out to a
GPL-3 binary is not linking). Both binaries are resolved **once**, at
`Engine::open`, over `calibre_bin_dir` → `PATH` → calibre's own install
directories; they are two `Option`s, not one flag, so a half install degrades to
the half that works.

`library_root` refuses a directory with no `metadata.db` **before the binary
runs** — `calibredb --with-library /typo list` otherwise *creates* a library
there and reports `[]` with exit 0.

### 4.5 Goodreads (`goodreads.rs`)

The API died in 2020, so the file is the interface. ISBNs arrive Excel-armoured
(`="0316769487"`) and go through `strip_armour` then `normalize_isbn`.
`Exclusive Shelf` maps onto readings: `read` → finished, `currently-reading` →
open, `to-read` → **no reading at all**. A row with no shelf column at all (our
own export writes eight columns) falls back to "a `Date Read` means it was read".

### 4.6 The user

`save_book`, `update_progress`, `reread`, `create_note` / `update_note_body` /
`delete_note`, `open_reflection` / `open_review`, `set_annotation`, `cite` /
`uncite`, `set_rating`, `put_rating_scale` / `map_rating`, `link_sidecar` /
`link_calibre_book` / `link_goodreads_row`, `merge_books`, `delete_book`,
`import_file` / `add_file_to_book` / `remove_file`.

---

## 5. Matching: one ladder, one matcher

Every path that meets a book from somewhere else runs the **same** comparison.
`matching.rs` holds the rule; `koreader::scores_for` is the storage half and
`koreader::band` filters what is left.

```
   certain ─────────────────────────────────────────────────► guess
   recorded id      content hash      bibliographic id     fuzzy
   ┌──────────┐    ┌────────────┐    ┌────────────┐    ┌──────────────┐
   │external_ │    │ sha256     │    │ ISBN-13    │    │ title+author │
   │ids (uuid,│ ►  │ partial_md5│ ►  │ ISBN-10    │ ►  │ score        │
   │Book Id)  │    │(device_    │    │            │    │              │
   └──────────┘    │ books)     │    └────────────┘    └──────────────┘
                   └────────────┘                             │
                                                    ┌─────────┴─────────┐
                                            ≥ 0.85  │                   │ 0.60–0.85
                                          AUTO_MATCH│                   │CANDIDATE_MIN
                                                    ▼                   ▼
                                                link it        offer the band,
                                                               write nothing
                                                                        │
                                                              < 0.60 or no shared
                                                              content word → None
```

Which rungs a given caller uses:

| Caller | Rungs, in order |
|---|---|
| KOReader sidecar | `partial_md5` → sibling `.epub`'s ISBN → title+author |
| owned file | sha256 → ISBN (epub only) → `partial_md5` → title+author |
| calibre row | uuid → ISBN → `partial_md5` (every format) → title+author |
| Goodreads row | `Book Id` → ISBN-13 → ISBN-10 → title+author |

**The score, and why it is not jaro-winkler.** It was, and that was a bug. JW is
a character-transposition metric with a four-character prefix bonus built for
personal names; `search::normalize` drops a leading article, which makes a
shared first word — and therefore that bonus — *more* likely. Over 780 pairs of
real titles, 10% landed in the 0.60–0.85 band, and the band is reported as the
maximum over the whole library, so at fifty books the chance of a spurious hit is
99.5%. `Dune` / `Dune Messiah` scored 0.87 and **linked itself to the wrong
book**.

Two signals fix it:

- **Titles must share a content word.** `0.65·jaro_winkler + 0.35·dice` over
  stopword-stripped tokens; no shared token at all is `None` — not a low score —
  unless the raw strings are above `TYPO_ONLY` = 0.90 (a misspelled one-word
  title rather than a coincidence).
- **Authors must not disagree.** A **veto only**, at `AUTHOR_DROP` = 0.55, and
  deliberately that low: no threshold separates `J.R.R. Tolkien` from
  `John Ronald Reuel Tolkien` (one person, 0.65) without also merging
  `Frank Herbert` with `Brian Herbert` (two, 0.90). Asked to decide that, the
  signal would be wrong either way, so it is not asked. **Absence is not
  disagreement** — either side having no author leans on the title alone, which
  is the common case (a sidecar-seeded book, a file matched by its filename
  stem). `author_key` compares alphabetically sorted tokens, because calibre says
  `Min Jin Lee` and an epub's `author_sort` says `Lee, Min Jin`.

`compare` returning `None` is the half that matters: the old matcher had no way
to say *nothing here looks like it*, so it always named its best coincidence.

**The band is a refusal with a next move, never a silent duplicate.** An
importer that lands in it writes nothing and hands back candidates; `--new` is
the override on every path that has one (`ko pull`, `goodreads import`,
`calibre import`, `import_file`). Note the seam: `files::import` enforces this
**in the engine**, while `ko pull`'s refusal lives in the **CLI**
(`crates/cli/src/commands/ko.rs:42`) — `import_book_from_sidecar` itself creates
unconditionally, keyed on `device_books` for idempotency.

---

## 6. Write capability — what readingbuddy originates

Everything below is ours, and no import path writes any of it.

- **Readings.** `readings` is a history, not a flag: a reread is a row.
  `idx_readings_one_open` makes "at most one open reading per book" an invariant.
  `update_progress` takes a *book* id and opens a reading when none is open —
  except when clearing `finished`, which reopens the most recent one, or the
  TUI's finished-toggle would mint an empty reading per keypress. `Book`'s four
  progress fields are read-only projections of the current reading (open if there
  is one, else most recent), resolved by one `LEFT JOIN` in `BOOK_FROM`.
- **Notes.** `create_note` writes the vault file first, then inserts metadata +
  the FTS row + wikilink edges **in one transaction**. Dangling `[[wikilinks]]`
  are kept as text and back-resolved the moment a note with that title is
  written — which is why `backlinks` is a plain `WHERE to_note = ?` with no
  dangling-by-title union: the two directions must be one edge set read from
  opposite ends.
- **Reflection and review.** Notes with a `kind`, not a parallel vault — a
  reflection is meant to be the hub and `note_links` *is* the graph.
  `idx_one_reflection` / `idx_one_review` make "one of each per **reading**" an
  invariant; `open_reflection` / `open_review` find the existing note first.
  **No shared body, ever** — a review is a rewrite for a different audience, not
  a slice of the private one.
- **Ratings.** `review_ratings` keeps the **raw value plus the scale id**, never
  only the mapped integer, because the map is user-editable and the mapping has
  to stay re-derivable. `rating_map` is an explicit lookup — formulas are always
  wrong at the ends. `RatingScale::step_index` / `value_at` is the one quantizer
  both sides go through, because `rating_map.value` is a REAL in a PRIMARY KEY
  and `min + 3*0.1` is not the `0.3` the user typed.
- **Annotations and citations.** `highlights.annotation` is ours beside the
  device's `ko_note`. Citations are by reference, so a review stays live across
  `refresh_device_fields`.
- **Merge.** `merge_books` folds a duplicate back in, in **one transaction**.
  `book_id` is an input to a highlight's `identity_hash`, so every moved row's
  hash is recomputed; a row that then collides is the *same annotation* and is
  dropped — after its notes and flashcards are repointed at the survivor.
  Idempotent: a repeat merge finds `src` gone and returns every count zero.

---

## 7. Outflow

| Export | Shape | Rules |
|---|---|---|
| Goodreads CSV (`export_goodreads`) | the **eight** columns Goodreads' importer reads | see below |
| Anki TSV (`export_flashcards`) | word / context / book title, tab-separated | `#separator:tab`, `#html:false`; tabs and newlines escaped to spaces |
| The vault | plain markdown, always | not an export step — it is the storage format |
| Owned files | `database/files/<ab>/<sha256>.<ext>` | plain files on disk, no container |
| Covers | `database/images/` | plain bitmaps |
| The API (`crates/api`) | serde DTOs, `readingbuddyd` over a unix socket | one JSON object per line |

**Goodreads export is the one with judgment in it.** Ordered by what the data
says, never by row id — a re-import into an empty library would otherwise
reorder itself. ISBNs go back out **armoured**, so a spreadsheet cannot eat the
leading zero and so our own reader meets, in its own output, the shape it has to
survive from theirs. Two losses are reported rather than papered over:

- **An unmapped rating skips its row.** Goodreads takes integers 0–5 with no
  halves; a rounded star is precisely what the explicit lookup table exists to
  refuse.
- **A reread exports its most recent reading only.** The importable CSV has no
  read-count column, and silent truncation looks like data loss on the far side.

The property that covers the lot is `export → import → export is stable`,
asserted into a *fresh* library.

**The DTO seam.** Domain types stay serde-free: `Book` carries `OffsetDateTime`,
reports carry `PathBuf`, `Diagnostic` carries a `Duration` — a derive would pick
each wire encoding by accident and then the accident is the API. A `PathBuf`
crosses as `to_string_lossy`, so a non-UTF-8 filename does not round-trip.

**What does not leave.** No telemetry, no sync, no cloud. Highlight text, note
bodies and search queries are the user's private reading and never rise above
`trace!` in the logs; API keys go through `googlebooks::scrub_key` on every error
path.

---

## 8. Four walkthroughs

### 8.1 Two books, same title, different metadata

There is no title uniqueness anywhere in the schema, and titles are never a key.
What happens depends on how the second one arrives.

**Both carry the same ISBN.** They are one book. `books.isbn_10` and
`books.isbn_13` are both `UNIQUE`, and `upsert_book` branches
isbn_10 → isbn_13 → plain insert, so the second arrival takes the
`ON CONFLICT DO UPDATE` path and merges under the no-clobber rules. A different
publisher fills a NULL `publisher` and does not overwrite one. A different title
**does** overwrite, because `title` is `NonEmptyText` — the last non-empty title
wins. Two genuinely different editions with the same ISBN are not
distinguishable and would collapse into one row.

**Different ISBNs.** Two rows. Nothing merges them and nothing tries: they are
different editions, which is a fact the schema can hold.

**Neither has an ISBN.** This is where the matcher decides. Every importer runs
the ladder in §5 before creating anything:

- ≥ `AUTO_MATCH` (0.85) **and** the author veto did not fire → linked to the
  existing book, and the incoming record is merged with `enrich_book` (a plain
  `UPDATE` by id), *not* `upsert_book` — whose third branch is an unconditional
  insert that ignores `Book::id` and would silently make a second copy on every
  run.
- 0.60–0.85 → **the band**. Nothing is written. The caller is handed candidates
  and offers `--new`, `link`, or nothing.
- Below that, or no shared content word → nothing looks like it, and a new book
  is created.

The author veto is what separates the same-title case that matters.
`Dune` / `Dune Messiah` is caught by the title rule (the score is high but the
titles differ on a content word); two different books called `Drive` by different
authors are caught by the veto; the *same* book listed as `Frank Herbert` in one
place and `Herbert, Frank` in another survives it, because `author_key` sorts
tokens. Two editions of one title by one author score high and **link** — which
is correct, because they are the same book.

**When a duplicate happens anyway** — and it will, since the ISBN-less path takes
an unconditional insert — `merge_books` is the move: one transaction, `dst`
wins on fields, highlight hashes recomputed against `dst`, colliding rows dropped
after their notes and flashcards are repointed, `readings.book_id` repointed with
the older of two open readings closed as `abandoned` rather than deleted,
`external_ids` and `device_books` repointed (losing an `external_id` would make
the next import recreate the duplicate the merge just folded in).

### 8.2 A book found by search, with no file behind it

The plain case, and the one with the least machinery.

```
search → SearchOutcome (in memory only, nothing written)
  ↓ user picks one
download_cover  → GET cover_url → database/images/<file>   → book.cover_path
save_book       → upsert_book                              → books (1 row)
```

Written: **one `books` row**, and a cover bitmap if the result carried a
`cover_url`. Nothing else. No `readings` row — a book on the shelf is not a book
you are reading, and `docs/decisions.md` bans task-completion framing, so nothing
counts it as unstarted. No `book_files` row, no `device_books` row, no
`external_ids` row.

Progress starts a reading, and only then: `update_progress` opens one lazily
(`readings.source = 'manual'`), or `reread` closes the open one and opens a
fresh one. `Book::current_page` / `finished` / `date_started` / `date_finished`
read back through the `BOOK_FROM` join and are `NULL`/`false` until then.

If the same book later arrives with a file — from calibre, from a device, from
`import_file` — it matches on ISBN (which the provider supplied) and attaches to
this row rather than creating a second.

### 8.3 A calibre book: file yes, highlights no

Per row of `calibredb list --for-machine`:

```
1. hash every format calibre holds     → partial_md5 ×N  (unreadable ones skipped)
2. match: uuid → ISBN → partial_md5 → title+author band
3. matched?  enrich_book(id, partial)      ← no-clobber, not straight assignment
   new?      upsert_book(partial)
             + set_book_created_at(timestamp)   ← only for a book just created
   band?     report candidates, write nothing   (--new overrides)
4. link_external_id('calibre', uuid, book_id)
5. add_book_tags(book_id, 'calibre', tags)      ← inert provenance
6. cover: copied into images/ only if we have none  ← fills a gap, never replaces
7. link_device_book(md5, book_id, Auto)  for every format hash
```

**The file itself is not copied.** No `book_files` row, no bytes in
`database/files/`. calibre owns the files (`docs/decisions.md`: per-field
ownership); what is recorded is their **KOReader identity**, in `device_books`.
That is the whole payoff, and it arrives later and elsewhere: when this book's
sidecar comes off the reader, `match_book` takes its `Md5` branch and links
outright instead of guessing at a title — including when KOReader is configured
to keep sidecars in `dir` or `hash` mode, away from the book. Taking ownership of
the bytes is a separate, explicit act: `Engine::import_file` /
`add_file_to_book`, which is engine-only today and has no CLI or TUI surface.

**No highlights, no readings, no rating.** calibre has none of the first, knows
nothing about the second, and the third is deliberately not imported: `list`
reports 0–10 half-stars while `set_metadata` takes 0–5, but the structural reason
is that a rating here lives on a *review*, which anchors to a *reading*, which
calibre knows nothing about — importing one would mean fabricating reading
history and then guessing at the explicit lookup table ratings must go through.
Series is dropped (no column, and `book_tags` is for shelves). `external_ids`
records calibre's **uuid only**, never the Goodreads identifier calibre may also
carry: that table repoints on conflict, and one system minting another's ids
would silently redirect a later Goodreads import.

**uuid, never `id`.** calibre ids are per-library and reused after a delete, and
`external_ids` has no library column to tell two libraries' id 4 apart.
`CalibreBookReport` carries `calibre_id` anyway, because a report line otherwise
names its row only by title and two editions of one title tie to each other.

Re-import writes nothing new: uuid matches, `enrich_book` fills nothing,
`add_book_tags` skips, `unlinked` is counted *before* the links are written so
the report says 0 files identified rather than announcing work it did not do.

### 8.4 A KOReader sidecar: file, highlights, notes, device state

The fullest inflow. Two entry points that differ **only** in how they pick the
book — `import_into` is shared, so the two paths cannot count differently.

```
parse (mlua, StdLib::NONE) → KoSidecar {
    doc_props / stats   title, authors, language, pages
    partial_md5_checksum
    percent_finished
    summary             status, rating, note (the user's review)
    highlights[]        text, pos0/pos1, ko_datetime, color, chapter, page, note
}
```

**Picking the book.**
`import` (a library sweep) matches or reports unmatched; it **never creates**.
`import_book_from_sidecar` (a pull) creates from the sidecar's own metadata —
`stats` first, then `doc_props`, `N/A` filtered, authors newline-split — and is
**fully offline**: no provider enrichment, so no ISBN, cover or description. Its
idempotency cannot come from `upsert_book` (a sidecar-seeded book has neither
ISBN, so the third branch would insert again on every pull); the guard is
`device_books` keyed on `partial_md5`. A sidecar without one still imports but
emits `SidecarNotIdentified`, because the duplicate would otherwise appear
silently much later.

**Per highlight** (`import_into`):

```
identity_hash = sha256(book_id | ko_datetime | pos0 | text)
insert_highlight  ON CONFLICT DO NOTHING
   ├─ Some(id) → inserted; single word? → insert_flashcard (UNIQUE(book_id,word))
   └─ None     → refresh_device_fields: straight-assign ko_note/color/chapter/page
                 changed? → updated   unchanged? → skipped
```

`DO NOTHING` rather than `DO UPDATE` is deliberate: `DO UPDATE` would make
`RETURNING id` yield a row on the conflict path, so `Some(id)` would stop meaning
"newly inserted" and the inserted/skipped counts the goldens assert would
collapse. The refresh updates **in place** — `notes.highlight_id` and
`flashcards.highlight_id` are foreign keys, so delete-and-reinsert would null
note anchors and cascade flashcards away.

`annotation` is never touched. A single-word highlight becomes a flashcard
candidate (space-less CJK text counts as one "word").

**Device state** (`persist_device_state`, never under `dry_run`):

```
if status or percent or rating present:
    ensure_reading(book_id, earliest ko_datetime, 'koreader')
        └─ opens one only when the book has NO reading at all
    set_device_state → ko_status / ko_percent / ko_rating   (straight assignment)
    status: complete  → finish_reading      (closes it)
            abandoned → abandon_reading     (marks, does NOT close — an abandoned
                                             book is one you might pick up)
            reading   → nothing new
            unknown   → ours untouched; UnknownDeviceStatus already fired
    attribute_highlights(book_id)
```

A sidecar with none of those says nothing about whether the book was ever read,
so it opens nothing and its highlights stay unattributed — a state that can be
acted on rather than a guess that cannot be undone. The reading starts at the
**earliest `ko_datetime` seen**: KOReader does not record when a book was opened,
and the first annotation is the earliest moment we can prove the user was in it.
"None open" rather than "no reading" is what stops a `complete` sidecar adding a
reading on every re-import.

**Attribution** places a highlight by matching `ko_datetime` into a reading's
window and **leaves `reading_id` NULL when no window holds it**. KOReader's
sidecar is per-file and a reread appends to it, so the device cannot supply this
attribution and inventing it would be worse. A missing `started_at` is *derived*,
not read as −∞: an unstarted reading begins one second after the latest
`finished_at` of another reading of the same book that is not after its own end.
The first cut `COALESCE`d to ±8.64e12, which makes an unstarted reading's window
*contain* every earlier one — so the newest read collected the whole book's
highlights and the earlier reads held none, permanently, with nothing on screen
looking wrong.

**`summary.note` — the user's own review on the device — is parsed and not
written.** It is private reading, the same class as highlight text; it is absent
from every log field and no import path stores it.

**Scanning versus syncing.** `scan_device` is read-only and returns a
`DeviceState` per book — New{candidates} / Unchanged / Updated{new,refreshed} /
Unreadable(Diagnostic). Its only write is `sidecar_seen`: `stat` first, and when
`(size, mtime)` match the cached row the Lua parse is skipped entirely. It caches
the **parse**, never the verdict — a book deleted here, or linked since, changes
a sidecar's state without touching the file, so New/Unchanged/Updated is
recomputed from the DB every scan. `mtime` is nanoseconds, because a device
flushes on every page turn and two edits in one second are ordinary. The cheap
path compares `entry_digest`, not a row count: a note rewritten on the device
leaves the count identical, so a count made the cache say "unchanged" forever
about a change it had already seen. A sidecar that does not parse is **not
cached** — every column is what the file said, and it said nothing.

**Repeat import writes nothing.** `inserted == 0`, no row mutates, and
`updated == 0` unless the device genuinely changed something.

---

## 9. What deliberately does not flow

- **Collections.** `book_tags` is written by two importers and read by nothing.
  Three systems minting shelves is a merge problem with no good default, so the
  raw names are kept and the design is deferred until it can be made against real
  data.
- **calibre → files.** Hashes are recorded; bytes are not copied. Ownership is
  an explicit act.
- **calibre → ratings, series.** See §8.3.
- **Goodreads ratings → the user's own scale.** Stored raw against the seeded
  `goodreads` scale and never reversed through `rating_map`: the map is
  many-to-one, so its inverse is a guess. An empty cell and an explicit `0` are
  different cells and *neither* is a rating.
- **Two-way sync to the device.** Nothing writes to a mounted reader.
  `last_seen_ko_note` is the column that would make it possible.
- **The mount watcher across the API.** It is a stream; request/response has no
  shape for one, and a polling wrapper would give the far side a different
  debounce from the one `watch.rs` guarantees.
- **Provider enrichment on a device pull.** Deliberately offline; the user
  enriches later via search and then merges.
- **`readings` from calibre.** It has no such concept.
