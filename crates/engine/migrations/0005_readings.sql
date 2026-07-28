-- Reading state leaves `books`.
--
-- `books` modelled one reading of one book, and rereads are real. It also put
-- reading state inside `upsert_book`'s provider-merge path, where a
-- `finished = MAX(excluded.finished, books.finished)` clause had to exist to
-- stop a metadata refresh from un-finishing a book — a clause that should never
-- have been needed, and which goes away with the columns it guarded.
--
-- The order below matters: create, then add `highlights.reading_id`, then
-- back-fill from `books`, and only then drop the four columns. `DROP COLUMN`
-- needs SQLite >= 3.35, which sqlx's bundled libsqlite3 is well past.

CREATE TABLE readings (
    id            INTEGER PRIMARY KEY,
    book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    started_at    INTEGER,
    finished_at   INTEGER,
    status        TEXT NOT NULL DEFAULT 'reading',  -- ours: reading|finished|abandoned
    source        TEXT NOT NULL DEFAULT 'manual',   -- manual|koreader|migrated
    current_page  INTEGER,
    ko_status     TEXT,        -- device-owned mirror
    ko_percent    REAL,        -- device-owned, 0.0..=1.0
    ko_rating     INTEGER,     -- device-owned, 1..5; absent = unrated, never 0
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);
CREATE INDEX idx_readings_book ON readings(book_id);

-- At most one open reading per book. The invariant, not a convention.
CREATE UNIQUE INDEX idx_readings_one_open ON readings(book_id) WHERE finished_at IS NULL;

ALTER TABLE highlights ADD COLUMN reading_id INTEGER REFERENCES readings(id) ON DELETE SET NULL;
CREATE INDEX idx_highlights_reading ON highlights(reading_id);

-- Back-fill BEFORE dropping. Same migration, this order.
INSERT INTO readings (book_id, started_at, finished_at, status, source,
                      current_page, created_at, last_modified)
SELECT id, date_started, date_finished,
       CASE WHEN finished = 1 THEN 'finished' ELSE 'reading' END,
       'migrated', current_page, strftime('%s','now'), strftime('%s','now')
  FROM books
 WHERE current_page IS NOT NULL OR finished = 1
    OR date_started IS NOT NULL OR date_finished IS NOT NULL;

ALTER TABLE books DROP COLUMN current_page;
ALTER TABLE books DROP COLUMN finished;
ALTER TABLE books DROP COLUMN date_started;
ALTER TABLE books DROP COLUMN date_finished;
