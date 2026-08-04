---
title: Items 29–32 — what the engine knows about a book, and where it learned it
date: 2026-08-04
source: docs/decisions.md for what is settled; docs/gui/spec-gui-17-28.md for
        the wave this one interleaves with
---

# Items 29–32

**The organising claim:** the engine is good at *acquiring* a book and bad at
*keeping* one. Every importer — providers, epub, KOReader, calibre, Goodreads —
writes a book once and never looks at it again. There is no path that re-asks a
provider, no record of which source supplied which field or when, no reading-time
data at all, and three fields a shelf needs (subjects, series, chapter list) that
nothing captures.

For a GUI whose whole job is showing the user accurate, current, explicable
facts, that is the gap. Items 17–28 make the *derived* layer coherent; this wave
makes the *acquired* layer complete and attributable.

## What this wave is not

- **Not a fourth provider.** A new source before per-field provenance exists is
  more disagreement with no way to attribute it. `docs/decisions.md`'s data
  ownership table is per-field; the schema records none of it. Fix that first.
- **Not automatic enrichment.** `docs/decisions.md:231` puts "provider enrichment
  on device pull" out of scope and that ruling stands. Item 30 is an *explicit
  user action* on a book the user is looking at. The device pull path is not
  touched.
- **Not two-way sync**, and not the KOReader plugin (item 15). Item 31 reads a
  file off a mounted volume, which is what `scan_device` already does.

## Migration numbers — reshuffled, and why

`docs/gui/spec-gui-17-28.md` pre-allocated `0011`→20, `0012`→21, `0013`→23.
**Nothing has been built against those numbers**, and
`migration_versions_are_contiguous_from_one` fails on a *gap* as well as on a
duplicate — its own message says so. Merging `0012` while `0011` does not exist
turns main red until an unrelated item lands.

So the numbers follow **merge order**, which is the only thing that keeps the
sequence contiguous:

| number | item | wave |
|---|---|---|
| `0011` | 21 — `reading_events` | this one, built first |
| `0012` | 29 — `field_provenance` | this one |
| `0013` | 32 — subjects, series, TOC | this one |
| `0014` | 20 — cover dimensions + accent | 17–28 |
| `0015` | 23 — moments | 17–28 |

A branch holding `0012` before `0011` has merged has a **red contiguity test,
expected and named**. Rebase after the predecessor lands; do not renumber to
make it green.

## Build order and parallelism

```
wave A   item 21 (0011)   ‖   item 29 (0012)
wave B   item 31          ‖   item 30
wave C   item 32 (0013)
```

- **21 ‖ 29** share no files: 21 is a new storage module plus a migration, 29 is
  `storage/books.rs` plus a migration. Both append to `lib.rs`.
- **30 ‖ 31** share no files: 30 is `providers/` + `search.rs`, 31 is `device.rs`
  + a new module. Both append to `lib.rs`.
- **32 alone.** It edits `MERGE_RULES` (29's file) and both providers (30's
  files). Three dialects of one merge table is the failure mode.

---

## Item 21 — `reading_events`, the source-agnostic activity log — migration `0011`

Pulled forward from the 17–28 wave, unchanged in design. Its spec entry in
`docs/gui/spec-gui-17-28.md` is the authority; read it there. It moves because
item 31 needs somewhere to put reading time, and the settled answer is that no
source is consumed in its own shape.

The one clarification this wave adds: that spec defers the `statistics.sqlite3`
filler to "item 15, the plugin work". **The deferral is about the plugin.**
Reading a statistics DB off a volume the user has already mounted is what
`scan_device` does today, and it needs no plugin, no write to the device, and no
network. Item 31 does that and nothing more.

**Blocks:** 23, 28 (in the other wave), and 31 here.

---

## Item 29 — where each field came from — migration `0012`

`docs/decisions.md:37` says *"Authority is per-field. Provenance is recorded."*
The second sentence is not true yet. `external_ids` and `book_tags` record
provenance about **rows**; no column anywhere says where `page_count` came from
or when.

Three consequences, all of them user-visible:

- **A GUI cannot explain a value.** A wrong page count is unattributable, so the
  only move a user has is to distrust the whole record.
- **No-clobber cannot protect a correction.** `MERGE_RULES`'s `COALESCE` pattern
  is right for partial provider records and cannot distinguish *the user typed
  this* from *a provider guessed it*. Item 30 would silently overwrite hand
  corrections on its first run, and that is why 29 lands first.
- **Staleness is unqueryable.** "Up to date" needs a `fetched_at` to mean
  anything.

```sql
CREATE TABLE field_provenance (
  book_id    INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  field      TEXT NOT NULL,
  source     TEXT NOT NULL,   -- openlibrary|googlebooks|calibre|epub|koreader|goodreads|user
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (book_id, field)
);
```

**Written from `MERGE_RULES`, never beside it.** That table already generates
both the upsert and `enrich_book` so the two cannot disagree about what merging a
partial record means; provenance is a third consumer of the same table, not a
fourth hand-maintained list. A field name that exists in one and not the other is
the bug this arrangement makes impossible.

`user` outranks everything. That is the whole reason the column exists.

**Depends on:** nothing. **Blocks:** 30, 32.

---

## Item 30 — `enrich_book_from_providers`

`Storage::enrich_book` exists and **only calibre calls it**. Every book created
without an ISBN — `import_book_from_sidecar` (offline by design),
`import_file` matched on a filename stem — has no cover, no description and no
page count, permanently. On a real library that is most of the shelf, and it is
exactly what a cover-forward GUI looks worst with.

An explicit action on one book, or on a selection the user names:

```rust
pub async fn enrich_book_from_providers(&self, id: i64) -> Result<EnrichReport>
```

- ISBN → `lookup_isbn`. No ISBN → `SearchRequest { title, author }` through the
  **existing** `matching.rs` bands. **No second matcher** — `docs/decisions.md`
  names that rule under **Files** and it holds wherever books are matched.
- Below `AUTO_MATCH`: return candidates, **write nothing**. Same
  refusal-with-a-next-move shape as `ko pull` and `calibre import`, with the same
  `--new`-shaped override.
- Merge through `MERGE_RULES` (provider = partial record = no-clobber), stamping
  `field_provenance`. **A field whose provenance is `user` is never overwritten**,
  and the report says it was held back rather than saying nothing.
- Then `download_cover` where one is missing.

`EnrichReport` names every field filled and every field held, so a frontend can
show what changed. Absence is reported, never prescribed.

**Depends on:** 29. **Must not:** touch the device pull path.

---

## Item 31 — reading time, from the device's own statistics

`partial_md5.rs`'s doc comment names "the `statistics.sqlite3` join" as one of
the three things it exists for, and nothing has ever done it. `device_books`
already holds the keys.

KOReader keeps `koreader/settings/statistics.sqlite3` with a `book` table (keyed
by its own `md5`, which is `partial_md5_checksum`) and a `page_stat_data` table
of per-page durations. That is measured time on a real device — the richest
signal in the app and the only one the user cannot get anywhere else.

It lands as **one more filler of `reading_events`**: day, minutes, pages,
`source = 'koreader'`, `confidence = 'measured'`. No view, no query and no line of
GUI changes when it arrives — which is item 21's entire point.

Three things it must get right:

- **Read-only, copy-then-read.** It is the user's device and a live SQLite file.
- **Absence is ordinary.** No statistics DB, an unknown schema version, a book
  with no row — all `Diagnostic`, never an error, never a zero. *A month with no
  device data returns absent minutes, not zero. Zero is a claim.*
- **Attribution into readings** uses the same window logic as
  `attribute_highlights`, and leaves `reading_id` NULL when no window holds the
  day. The device cannot know about rereads; inventing the attribution is worse
  than not having it.

**Depends on:** 21. **Must not:** write to the device, or become part of `sync_device`'s
default path without the user asking.

---

## Item 32 — subjects, series, and the chapter list — migration `0013`

Three fields nothing captures, each of which makes a book's page understandable
rather than merely correct.

- **Subjects.** Google Books `volumeInfo.categories`, OpenLibrary
  `/works/{}.json` `subjects`. Stored normalized and **separate from
  `book_tags`**, which is for shelves the user or another system minted —
  `docs/decisions.md:124` defers collections precisely because three systems
  minting them is a merge problem, and a provider subject is not a collection.
- **Series and index.** No column exists, which is why `calibre.rs` drops series
  outright and says so. "Dune Messiah (Dune #2)" is currently unrepresentable —
  and `matching.rs` already had to be taught not to confuse that exact pair.
- **The epub's table of contents.** `epub.rs` extracts a cover and nothing else.
  A TOC lets a highlight name its chapter where KOReader did not, and lets
  progress read as a position in a structure rather than a bare page number.
  Fully offline, from a file already owned.

Each new field is a `MERGE_RULES` row and therefore gets `field_provenance` from
day one, which is the second reason 29 goes first.

**Depends on:** 29, 30. **Migration:** `0013`.

---

## What this wave owes back

Per `docs/decisions.md`'s ritual, each item records the corrections it forced
when it lands. The three most likely to be worth having:

- whether `field_provenance` wants a `value_hash` (to detect "the provider
  changed its mind") or whether `fetched_at` is enough;
- what KOReader's statistics schema actually contains across versions, since only
  the user's own device can answer it and it is not in the corpus;
- whether subjects need a controlled vocabulary or stay raw, which is the same
  question `book_tags` deferred.
