-- The activity log, and it is source-agnostic on purpose.
--
-- Reading-time data is KOReader-only today, so a surface built straight on it
-- opens to blanks for a reader whose library came from a Goodreads CSV. The
-- answer is not to skip the surface: it is to stop consuming any source in its
-- own shape. Every source — a sidecar, the vault, an imported CSV, a page typed
-- in here, and later KOReader's `statistics.sqlite3` — writes the same handful
-- of facts about the same grain, so a filler that arrives later changes no
-- query, no view and no line of frontend.
--
-- `day` is TEXT, `YYYY-MM-DD`, in UTC.
--
--   * **TEXT rather than an integer day number.** A zero-padded ISO date sorts
--     and compares lexicographically exactly as it does chronologically, so
--     `BETWEEN '2026-01-01' AND '2026-01-31'` *is* the month query;
--     `date(ts,'unixepoch')` produces it and `strftime` consumes it, so the
--     conversion lives in SQLite rather than in three Rust call sites. And the
--     part an integer cannot do at any price: it reads as a date in a `sqlite3`
--     shell. A day number means nothing without the epoch it counts from, and
--     that epoch would live in a Rust file rather than in the database.
--   * **UTC**, because `ko_datetime_to_unix` already reads the device's zoneless
--     `YYYY-MM-DD HH:MM:SS` as UTC and `attribute_highlights` compares it
--     against `strftime('%s', …)` on the same reading. A second convention here
--     would put an event and the reading it was attributed to hours — and at a
--     boundary, a whole day — apart for everyone not on UTC.
--
-- **The primary key is `(book_id, day, source)`**, and it is the design.
-- Idempotency is the property this table lives or dies on: a filler re-run must
-- refresh what it wrote last time, never duplicate it, and `ON CONFLICT` needs
-- a declared key to aim at.
--
-- `reading_id` is deliberately **not** in that key, though the grain reads "a
-- book, a read, a date". Two reasons, and the second is fatal by itself:
--
--   * An event is not an occurrence; it is the record that this day carried
--     activity of this kind. A read started and finished on one day is one such
--     record, not two, and keeping it one is what makes summing over a period
--     honest.
--   * `reading_id` is nullable with `ON DELETE SET NULL` — the call `notes` made
--     in `0007`, for the same reason: losing the reading must not lose the fact.
--     A key containing a nullable column needs a NULL sentinel, and then
--     deleting a reading with two events on one day would set both rows to that
--     sentinel and collide, so SQLite would refuse the delete with a constraint
--     error raised by a table nobody had touched.
--
-- So the read is an **attribution** — refreshed by the filler the way a device
-- field is, and left NULL when the day's evidence does not agree on one read,
-- which is the same call `attribute_highlights` makes rather than guessing.
--
-- `source` and `confidence` carry their vocabulary in a **comment, not a
-- `CHECK`**. `readings.source` set that precedent in `0005` deliberately, and it
-- is why item 22's `'local'` needs no schema change at all. `source` names where
-- the fact came from, so a reading-endpoint event carries the reading's own
-- source verbatim; several fillers may share one token, and the merge below is
-- what lets them.
--
-- `minutes` and `pages` are nullable and mean **not known**. Never 0. Zero is a
-- claim, and a month with no device data has not made it.

CREATE TABLE reading_events (
    -- NOT NULL is spelled out on all three key columns because SQLite permits
    -- NULLs in a rowid table's PRIMARY KEY — a documented, kept-for-compatibility
    -- departure from every other database, and one that would let a filler with
    -- a NULL day insert the same row for ever.
    book_id    INTEGER NOT NULL REFERENCES books(id)    ON DELETE CASCADE,
    reading_id INTEGER          REFERENCES readings(id) ON DELETE SET NULL,
    day        TEXT    NOT NULL,   -- 'YYYY-MM-DD', UTC
    minutes    INTEGER,            -- NULL = not known, never 0
    pages      INTEGER,            -- NULL = not known, never 0
    source     TEXT    NOT NULL,   -- koreader | vault | goodreads | local | manual | migrated | …
    confidence TEXT    NOT NULL,   -- measured | inferred
    created_at INTEGER NOT NULL,
    PRIMARY KEY (book_id, day, source)
);

-- The aggregates range over days across the whole library, and the primary-key
-- index is on `(book_id, …)`, which cannot serve a query that names no book.
CREATE INDEX idx_reading_events_day ON reading_events(day);

-- `ON DELETE SET NULL` scans the child table for every reading deleted unless
-- the child key is indexed. `idx_highlights_reading` (migration `0005`) exists
-- for the same reason.
CREATE INDEX idx_reading_events_reading ON reading_events(reading_id);
