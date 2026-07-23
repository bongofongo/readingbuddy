CREATE TABLE books (
    id             INTEGER PRIMARY KEY,
    title          TEXT NOT NULL DEFAULT '',
    sort_title     TEXT,
    authors        TEXT NOT NULL DEFAULT '[]',        -- JSON array of strings
    translators    TEXT NOT NULL DEFAULT '[]',        -- JSON array of strings
    publisher      TEXT,
    publish_year   INTEGER,
    language       TEXT,
    isbn_10        TEXT UNIQUE,                       -- normalized: no hyphens, may end in X
    isbn_13        TEXT UNIQUE,
    openlibrary_key TEXT,
    googlebooks_id TEXT,
    cover_url      TEXT,
    cover_path     TEXT,
    page_count     INTEGER,
    description    TEXT,
    first_sentence TEXT,
    current_page   INTEGER,
    finished       INTEGER NOT NULL DEFAULT 0,
    date_started   INTEGER,                           -- unix seconds
    date_finished  INTEGER,
    created_at     INTEGER NOT NULL,
    last_modified  INTEGER NOT NULL
);

CREATE TABLE highlights (
    id            INTEGER PRIMARY KEY,
    book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    text          TEXT NOT NULL,
    chapter       TEXT,
    page          INTEGER,
    pos0          TEXT,                               -- KOReader xpointer
    pos1          TEXT,
    ko_datetime   TEXT,
    color         TEXT,
    note          TEXT,                               -- KOReader-attached note
    source        TEXT NOT NULL DEFAULT 'koreader',
    identity_hash TEXT NOT NULL,                      -- sha256(book_id|datetime|pos0|text): idempotent import
    created_at    INTEGER NOT NULL,
    UNIQUE(book_id, identity_hash)
);
CREATE INDEX idx_highlights_book ON highlights(book_id);

CREATE TABLE notes (
    id            INTEGER PRIMARY KEY,
    book_id       INTEGER REFERENCES books(id) ON DELETE SET NULL,
    highlight_id  INTEGER REFERENCES highlights(id) ON DELETE SET NULL,
    file_path     TEXT NOT NULL UNIQUE,               -- relative to vault_dir
    title         TEXT NOT NULL,
    kind          TEXT NOT NULL DEFAULT 'note',       -- note | session | final
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);
CREATE INDEX idx_notes_book ON notes(book_id);

CREATE TABLE note_links (
    from_note    INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    to_note      INTEGER REFERENCES notes(id) ON DELETE SET NULL,
    target_title TEXT NOT NULL,                       -- unresolved targets kept as text
    PRIMARY KEY (from_note, target_title)
);

CREATE TABLE flashcards (
    id           INTEGER PRIMARY KEY,
    book_id      INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    highlight_id INTEGER REFERENCES highlights(id) ON DELETE CASCADE,
    word         TEXT NOT NULL,
    context      TEXT,
    exported     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    UNIQUE(book_id, word)
);

-- Ordinary FTS5 table (not contentless): note bodies live on disk, the FTS copy
-- is a searchable cache maintained by the notes engine (delete + insert per save).
CREATE VIRTUAL TABLE notes_fts USING fts5(title, body, tokenize='porter unicode61');
