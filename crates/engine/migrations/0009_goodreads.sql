-- Goodreads CSV, in and out.
--
-- Three tables' worth of decisions, and one of them is a trap this migration
-- exists to disarm.
--
-- `book_tags` and `external_ids` are both **inert provenance**: what another
-- system said about a book, recorded verbatim, with no merge semantics and no
-- UI. `docs/decisions.md` defers collections outright — three systems minting
-- them is a merge problem with no good default — so the data is kept meanwhile
-- and a later design can be made against real shelves rather than guesses.

-- Goodreads' `Bookshelves`, one row per shelf. `raw` beside `tag` because the
-- exported value is the *user's* string and the tag is our normalization of it;
-- keeping only the normalized form would make the original unrecoverable, and
-- the whole point of provenance is that the origin's version survives.
CREATE TABLE book_tags (
    book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    source  TEXT NOT NULL,          -- 'goodreads' today, 'calibre' next
    raw     TEXT,
    PRIMARY KEY (book_id, tag, source)
);
CREATE INDEX idx_book_tags_tag ON book_tags(tag);

-- Goodreads' own `Book Id`, and what makes re-importing the same CSV
-- idempotent: without it a row whose ISBN we do not have and whose title we
-- edited is a new book on every import.
--
-- **General rather than a `goodreads_id` column on `books`.** Calibre (item 13)
-- needs exactly the same mapping, and two columns that mean "some other
-- system's id for this book" would be one too many — the second one is where
-- the query that forgets to check both starts.
CREATE TABLE external_ids (
    source      TEXT NOT NULL,
    external_id TEXT NOT NULL,
    book_id     INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (source, external_id)
);
CREATE INDEX idx_external_ids_book ON external_ids(book_id);

-- ---------------------------------------------------------------------------
-- The trap.
--
-- `active_rating_scale()` was `ORDER BY created_at DESC, id DESC LIMIT 1`, i.e.
-- "the most recently created scale". Seeding a `goodreads` scale below would
-- therefore make *it* the active one, and `rating show` and `set_rating` would
-- silently start defaulting to Goodreads' integers — this item changing the
-- meaning of two commands it has nothing to do with, with nothing on screen
-- looking wrong.
--
-- So the choice becomes a stored flag rather than an accident of ordering, and
-- a partial unique index makes "exactly one default" an invariant rather than a
-- convention. `put_rating_scale` sets it on a *new* scale (preserving the old
-- "new ratings use the scale you just made" behaviour) and leaves it alone when
-- redefining an existing one's bounds.
ALTER TABLE rating_scales ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0;

-- Back-fill: whichever scale the old ordering would have picked keeps being the
-- one, so no existing library's active scale moves under it.
UPDATE rating_scales SET is_default = 1
 WHERE id = (SELECT id FROM rating_scales ORDER BY created_at DESC, id DESC LIMIT 1);

CREATE UNIQUE INDEX idx_one_default_scale ON rating_scales(is_default) WHERE is_default = 1;

-- Goodreads' own scale: integers 0–5, no halves. It is a scale like any other
-- so that an imported `My Rating` can be stored as the **raw value plus the
-- scale id** — never reversed through `rating_map` onto the user's own scale,
-- because that map is many-to-one and its inverse is a guess.
--
-- `is_default` is 0, and that is the whole point of the column.
INSERT INTO rating_scales (name, min, max, step, created_at, is_default)
VALUES ('goodreads', 0.0, 5.0, 1.0, strftime('%s','now'), 0);

-- Identity map. Trivially true here and still an explicit lookup table rather
-- than a formula, for the same reason as `default`'s: the table is the thing
-- the user edits, and a formula has nowhere to record an edit.
INSERT INTO rating_map (scale_id, value, goodreads)
SELECT id, v.value, v.value
  FROM rating_scales, (SELECT 0 AS value UNION ALL SELECT 1 UNION ALL SELECT 2
                       UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5) v
 WHERE rating_scales.name = 'goodreads';
