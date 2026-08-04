-- Where each field of a book came from, and when.
--
-- `docs/decisions.md` says "Authority is per-field. Provenance is recorded."
-- `external_ids` and `book_tags` (migration 0009) record provenance about
-- *rows* — which book another system means, which shelves it filed it under.
-- Nothing anywhere said where `page_count` came from. This is that.
--
-- Written from `MERGE_RULES` in `storage/books.rs`, never beside it: the same
-- table that generates the upsert's ON CONFLICT clause, `enrich_book`'s UPDATE
-- and `merge_books`'s fill clause also decides which fields get a row here. A
-- field name in one list and not the other is the bug that arrangement makes
-- unrepresentable.
--
-- `source` has a vocabulary and no CHECK constraint, following `readings.source`
-- in 0005: a CHECK is a schema migration every time a source is added, and the
-- writer is a Rust enum, which is the constraint that actually holds.
--
-- **No back-fill, deliberately.** Every existing book has fields from
-- somewhere, and nothing in the row says where. `openlibrary_key`,
-- `googlebooks_id`, `external_ids` and `device_books` are circumstantial: an
-- `openlibrary_key` records that OpenLibrary was *consulted*, not that it
-- supplied the `page_count` beside it, and a `device_books` row records that a
-- file was matched, not that KOReader supplied the title. `fetched_at` is worse
-- still — `books.created_at`/`last_modified` are when the *row* changed, and
-- there is no honest reading of either as when a field was fetched. Inventing
-- provenance is the one thing this table exists to stop, so history stays
-- unattributed and says so by the absence of a row.
--
-- An absent row therefore means exactly "nobody has claimed this field", which
-- is the state item 30 must handle anyway: unattributed is mergeable, `user` is
-- not.
CREATE TABLE field_provenance (
  book_id    INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  field      TEXT NOT NULL,
  -- openlibrary | googlebooks | calibre | epub | koreader | goodreads | user
  source     TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (book_id, field)
);
