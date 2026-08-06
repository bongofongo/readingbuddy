-- Moments, and the memory that makes them fire once (item 23).
--
-- A moment is *recognised*, never announced. Everything a moment is made of —
-- a reading that closed, the first mark on a book, a reflection that reached
-- across to another book, a run of days that ended — is already in the
-- database, written by somebody else for another reason. So this file adds no
-- column anywhere those facts live and no table to hold a moment in. **A
-- moment is derived on every ask**, which is what stops it being a thing the
-- app can accumulate, count, or put a badge beside.
--
-- What cannot be derived is what the app has already *shown*. That is state the
-- app keeps about **itself**, it is the whole content of this migration, and it
-- is two tables.
--
-- ## `moments` — the set of ids already surfaced
--
-- One column of substance. The id is the moment's own identity, built in Rust
-- from the rows the moment is made of (`crates/engine/src/storage/moments.rs`,
-- `Moment::id`) and **opaque everywhere else** — no frontend parses it and no
-- SQL here builds one, because an id spelled in two dialects is an id that
-- eventually differs in one of them.
--
-- Deliberately *not* stored beside it: `book_id`, `reading_id`, `occurred_at`,
-- the kind. Every one of those is derivable from the evidence the moment was
-- built out of, and this table's entire job is to remember what was shown. A
-- `book_id` here would earn one thing — an `ON DELETE CASCADE` tidying the row
-- away with its book — and cost the rule that nothing derivable is duplicated.
-- The row that outlives its evidence is **inert**: its moment can never be
-- re-derived, so nothing reads it and nothing shows it. That is a few dozen
-- bytes, once, against a copy of a fact that could disagree with its original.
--
-- Acknowledging is `INSERT … ON CONFLICT DO NOTHING`, so it is idempotent by
-- construction and the *first* surfacing is the one whose time is kept. A
-- client that acknowledges twice, or two clients that acknowledge the same
-- moment, cannot produce a second row or move the first.
--
-- ## `moment_epoch` — when moments began
--
-- Without this, the first launch after this migration is a ceremony replaying
-- an entire reading history: every reading ever closed, every book ever
-- annotated, every run of days ever completed, all at once. That is the same
-- failure `docs/decisions.md` records for a fresh install importing a 400-book
-- Goodreads CSV, in a different costume — history arriving is not a thing you
-- just did.
--
-- The CSV case is answered per book (`books.created_at` already records when a
-- book entered the library, so an event before it is not news). This one cannot
-- be: those books entered the library honestly, months ago, and their events
-- came after. So the answer is a single instant — **the moment moments started
-- existing** — and everything before it is history.
--
-- `strftime('%s','now')` rather than a value the engine writes on first open,
-- because "when did this schema learn about moments" is exactly what a
-- migration knows and nothing else does. It is `CAST` to INTEGER because
-- `strftime` returns TEXT and every other timestamp in this database is unix
-- seconds; a TEXT `began_at` compares as text against them and is silently
-- wrong for two centuries starting in the year 10000, which is the sort of
-- thing that is cheaper to cast than to argue about.
--
-- Both guards **err toward silence**, and that is the property to keep when
-- either is changed: `books.created_at` is an upper bound on when a book really
-- arrived (a merge repoints rows onto the survivor's row, which is older), and
-- the reach time of a wikilink is a *lower* bound (`note_links` carries no
-- timestamp — item 21 recorded that — so a reach is dated no earlier than the
-- later of its two notes). An approximation that can only suppress a moment
-- loses a ceremony. One that can only invent them replays a library.
--
-- The row is a singleton by `CHECK (id = 1)` rather than by convention, for the
-- reason `idx_readings_one_open` and `idx_one_reflection` exist: "there is
-- exactly one of these" is an invariant a table can hold, and a second row here
-- would make the epoch whichever one a query happened to read.

CREATE TABLE moments (
    id          TEXT    PRIMARY KEY,   -- opaque; built by `Moment::id`, parsed by nobody
    surfaced_at INTEGER NOT NULL       -- unix seconds; the *first* surfacing
);

CREATE TABLE moment_epoch (
    id       INTEGER PRIMARY KEY CHECK (id = 1),
    began_at INTEGER NOT NULL          -- unix seconds
);

INSERT INTO moment_epoch (id, began_at)
VALUES (1, CAST(strftime('%s', 'now') AS INTEGER));
