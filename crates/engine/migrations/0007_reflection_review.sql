-- Reflection and Review.
--
-- Both are markdown in the vault with a DB record, both anchor to a *reading*,
-- and only Review carries a rating. `notes` already supplies all of that
-- machinery — slugify, frontmatter, the `note_links` edge table with dangling
-- targets held as text, the FTS5 body cache — so they are notes with a new
-- `kind` plus three small side tables for what a note has no room for.
--
-- The deciding argument is the graph: a reflection is meant to be the hub, and
-- `note_links` *is* the graph. A separate table would need its own edges, its
-- own FTS index and its own editor path.

ALTER TABLE notes ADD COLUMN reading_id INTEGER REFERENCES readings(id) ON DELETE SET NULL;
CREATE INDEX idx_notes_reading ON notes(reading_id);

-- `kind` gains 'reflection' | 'review'. The old 'final' is superseded: it was
-- "final thoughts after finishing", which is what a reflection is, except a
-- reflection opens mid-book and accretes.
--
-- This runs *before* the partial unique indexes below, and must: every migrated
-- row carries `reading_id = NULL`, and SQLite treats NULLs as distinct in a
-- unique index, so a library with several old `final` notes still applies.
UPDATE notes SET kind = 'reflection' WHERE kind = 'final';

-- One reflection and one review per reading. The invariant, not a convention.
CREATE UNIQUE INDEX idx_one_reflection ON notes(reading_id) WHERE kind = 'reflection';
CREATE UNIQUE INDEX idx_one_review     ON notes(reading_id) WHERE kind = 'review';

CREATE TABLE rating_scales (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    min        REAL NOT NULL,
    max        REAL NOT NULL,
    step       REAL NOT NULL,
    created_at INTEGER NOT NULL
);

-- An explicit lookup, never a formula: formulas are always wrong at the ends.
CREATE TABLE rating_map (
    scale_id  INTEGER NOT NULL REFERENCES rating_scales(id) ON DELETE CASCADE,
    value     REAL NOT NULL,
    goodreads INTEGER NOT NULL,     -- 0..5, 0 = unrated
    PRIMARY KEY (scale_id, value)
);

-- The raw value plus the scale id. Never only the mapped value: the map is
-- user-editable, so the mapping must stay re-derivable.
CREATE TABLE review_ratings (
    note_id  INTEGER PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
    scale_id INTEGER NOT NULL REFERENCES rating_scales(id),
    value    REAL NOT NULL
);

-- Citations by reference, so a review stays live when a highlight's
-- device-owned fields are refreshed, and so "which highlights did I actually
-- use?" is a query rather than a re-read of the prose.
CREATE TABLE citations (
    note_id      INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    highlight_id INTEGER NOT NULL REFERENCES highlights(id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (note_id, highlight_id)
);
CREATE INDEX idx_citations_highlight ON citations(highlight_id);

-- A scale so that rating a review works out of the box; `rating scale` edits it.
INSERT INTO rating_scales (name, min, max, step, created_at)
VALUES ('default', 0.0, 5.0, 0.5, strftime('%s','now'));

-- Seeded map: the whole stars only. That is a lookup table, not a formula — and
-- the halves are left out on purpose, because 0.5 has no honest Goodreads
-- integer and rounding it is exactly what this table exists to refuse. An
-- unmapped value reports itself; `rating map` is how the user decides.
INSERT INTO rating_map (scale_id, value, goodreads)
SELECT id, v.value, v.value
  FROM rating_scales, (SELECT 0 AS value UNION ALL SELECT 1 UNION ALL SELECT 2
                       UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5) v
 WHERE rating_scales.name = 'default';
