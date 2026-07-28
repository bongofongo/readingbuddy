-- readingbuddy owns its files.
--
-- Content-addressed storage: the bytes live at
-- `database/files/<ab>/<sha256>.<ext>` and this table is what says whose they
-- are. The **original filename is a column, not the path** — a filename is
-- something a user (or calibre, or a download) chose, it collides, it is not
-- unique and it is not stable, so it is provenance rather than identity.
--
-- `sha256` is the primary key, and that is the first of the three dedup levels
-- stated outright: identical bytes are one file, wherever they came from and
-- however many times they are imported. A second import of the same content
-- therefore cannot create a second row, and the on-disk copy at that address is
-- written once.
--
-- The consequence worth stating: because the key is global rather than
-- `(book_id, sha256)`, the same bytes cannot hang off two books. That is not a
-- limitation, it is the level-1 answer — if one file is genuinely both books,
-- the books are duplicates and `merge_books` is the move. It is also what makes
-- the merge's repoint a plain `UPDATE`: no collision is possible.
--
-- **Level 2 is `book_id` and nothing else**: many files, one book (epub + azw3
-- = two rows, one book). **Level 3 — same book, unknown file — is not a column
-- here at all**: it is ISBN, then `partial_md5` through the *existing*
-- `device_books` table, then `search.rs`'s jaro-winkler title fingerprint.
-- Deliberately no `partial_md5` column: `device_books` already maps that value
-- to a book and is the table both the sidecar matcher and the
-- `statistics.sqlite3` join read, so a second copy here would be a second
-- answer to one question.
CREATE TABLE book_files (
    sha256        TEXT PRIMARY KEY,
    book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    -- Lowercased extension: 'epub', 'azw3', 'pdf'. Also the extension of the
    -- stored file, so the address is derivable from the row.
    format        TEXT NOT NULL,
    -- What the file was called when it arrived. Provenance; nothing resolves by it.
    original_name TEXT,
    size          INTEGER NOT NULL,
    added_at      INTEGER NOT NULL
);

-- "What files does this book have?" is the only other way this table is read,
-- and it is read on every book view once a frontend shows formats.
CREATE INDEX idx_book_files_book ON book_files(book_id);
