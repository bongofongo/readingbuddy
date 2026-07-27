-- Maps a KOReader device file to a book in the library.
--
-- The key is the sidecar's root `partial_md5_checksum` — NOT `stats.md5`, which
-- the spec assumed and which does not exist on any 2024.11+ device (see
-- docs/koreader-format.md §2.1). It is the one stable book identifier a sidecar
-- carries: filenames get renamed, titles get edited, and sidecars do not always
-- live beside their book, but the checksum follows the file.
--
-- Once a sidecar has been linked the decision is recorded here and never
-- re-guessed, which is what stops a fuzzy title match from drifting onto a
-- different book on a later import. `linked_by` distinguishes a match we made
-- from one the user confirmed, so a future UI can offer to revisit the former
-- and must not touch the latter.
CREATE TABLE device_books (
    partial_md5 TEXT PRIMARY KEY,
    book_id     INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    linked_by   TEXT NOT NULL,     -- 'auto' | 'manual'
    first_seen  INTEGER NOT NULL,
    last_seen   INTEGER NOT NULL
);
CREATE INDEX idx_device_books_book ON device_books(book_id);
