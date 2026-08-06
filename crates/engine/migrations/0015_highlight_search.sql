-- Highlights become searchable (item 34).
--
-- `notes_fts` (0001) has no triggers and is maintained from application code,
-- and `storage/notes.rs` records that as a settled answer rather than a
-- deferral: a trigger copies between tables, and a note's body is **not in the
-- database** — `notes` has no body column, so there is nothing for a trigger to
-- read. Highlights are the exact opposite. `highlights.text` is a column, so a
-- trigger can read it, and an *external-content* index can avoid copying it at
-- all. The reason `notes_fts` cannot have triggers is the reason this one must.
--
-- Four things follow from `content='highlights'`, and each was rehearsed
-- against sqlite3 3.51 before this file was written rather than reasoned about.
--
-- 1. **One copy of the passage, not two.** An ordinary fts5 table would store a
--    second copy of every highlight the user has ever taken; external content
--    stores only the index and reads the columns back off `highlights` when
--    `snippet()` needs them. Highlight text is the user's private reading, and
--    the fewer places it is written the better.
-- 2. **The back-fill is one statement.** `'rebuild'` reads the content table
--    and builds the index from it, so unlike `0012` and `0014` — the repo's two
--    deliberate non-back-fills — this migration can back-fill honestly, because
--    the values are right there and no one has to guess at them.
-- 3. **`AFTER UPDATE OF`, not `AFTER UPDATE`.** `attribute_highlights` rewrites
--    `reading_id` for every highlight of a book on every import, and
--    `merge_books` rewrites `book_id`/`identity_hash` per moved row. Neither
--    touches an indexed column, and an unrestricted update trigger would make
--    both pay a delete-and-reinsert of the index per row for nothing. Naming
--    the three columns means those two statements do not fire at all —
--    measured, not assumed.
-- 4. **A cascade fires the delete trigger.** `highlights.book_id` is
--    `ON DELETE CASCADE`, so `delete_book` removes highlights without ever
--    naming them; SQLite runs this table's AFTER DELETE trigger for those rows,
--    which is what stops a deleted book leaving hits behind it in the index.
--    Checked by running it both ways: with the trigger the rows go, without it
--    they stay and `MATCH` keeps returning their rowids.
--
-- What that buys is the property this index exists for: **there is no write
-- path to `highlights` that can skip it.** `insert_highlight`,
-- `refresh_device_fields`, `set_annotation`, `merge_books`' drop, `delete_book`'s
-- cascade — and `crates/corpus`' `gen-devdb`, which writes raw SQL and by design
-- does not link the engine, so a hand-written Rust writer would have had to be
-- re-implemented there (as `notes_fts` in fact is, with a comment saying so).
-- A future insert path gets the index for free instead of having to remember it.
--
-- `annotation` is indexed beside `text` and `ko_note` even though it is *ours*
-- and the other two are the device's. The ownership seam is about who may
-- overwrite what; a search box is asked "where did I read that" and all three
-- are honest answers to it. `chapter` is deliberately **not** indexed: every
-- highlight in a book carries one, so it matches broadly and means nothing.

CREATE VIRTUAL TABLE highlights_fts USING fts5(
    text,
    ko_note,
    annotation,
    content='highlights',
    content_rowid='id',
    tokenize='porter unicode61'
);

CREATE TRIGGER highlights_fts_ai AFTER INSERT ON highlights BEGIN
    INSERT INTO highlights_fts (rowid, text, ko_note, annotation)
    VALUES (new.id, new.text, new.ko_note, new.annotation);
END;

CREATE TRIGGER highlights_fts_ad AFTER DELETE ON highlights BEGIN
    INSERT INTO highlights_fts (highlights_fts, rowid, text, ko_note, annotation)
    VALUES ('delete', old.id, old.text, old.ko_note, old.annotation);
END;

CREATE TRIGGER highlights_fts_au
AFTER UPDATE OF text, ko_note, annotation ON highlights BEGIN
    INSERT INTO highlights_fts (highlights_fts, rowid, text, ko_note, annotation)
    VALUES ('delete', old.id, old.text, old.ko_note, old.annotation);
    INSERT INTO highlights_fts (rowid, text, ko_note, annotation)
    VALUES (new.id, new.text, new.ko_note, new.annotation);
END;

-- Every highlight taken before this migration, indexed from the column that
-- already held it.
INSERT INTO highlights_fts (highlights_fts) VALUES ('rebuild');
