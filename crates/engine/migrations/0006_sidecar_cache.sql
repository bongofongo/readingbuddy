-- The device-scan pre-filter.
--
-- A scan of a mounted reader has to answer "what is the state of each book on
-- this device" every time the screen opens. Done naively that means evaluating
-- every sidecar's Lua in mlua — several hundred VM evaluations on a full
-- device — so a scan `stat`s first and, when `(size, mtime)` match the row
-- recorded here, skips the parse entirely.
--
-- **This caches the parse, never the verdict.** New / Unchanged / Updated is
-- recomputed from the database on every scan, because a book deleted here, or
-- linked since, changes a sidecar's state without touching the file. Every
-- column below is something the *file* said; nothing here depends on the
-- library, which is the only reason the cache cannot start lying after an
-- unrelated library edit.
--
-- `title`, `authors`, `ko_percent` and `ko_status` are on that same footing and
-- are stored for the same reason as `partial_md5`: without them a cache hit
-- could not be matched to a book, could not be displayed, and would have to
-- re-parse — which would make the cache do nothing at all.
--
-- `metadata.*.lua.old` backups are never recorded here. KOReader writes one on
-- every flush (`docsettings.lua:340`), 9 of 10 real `.sdr` dirs have one, and
-- it is a *previous* state of the same annotations — importing it would
-- resurrect highlights the user deleted. The walker excludes them by suffix.
CREATE TABLE sidecar_seen (
    path        TEXT PRIMARY KEY,   -- absolute path of the metadata.*.lua
    size        INTEGER NOT NULL,
    -- Nanoseconds since the epoch, not seconds: two edits inside one second are
    -- ordinary on a device that flushes on every page turn, and a
    -- second-resolution stamp would report the second one as unchanged.
    mtime       INTEGER NOT NULL,
    partial_md5 TEXT,
    -- Highlights the file yielded.
    entry_count INTEGER NOT NULL,
    -- One hash over the identity and device-owned payload of every annotation
    -- in the file. Compared against the same hash taken over what the linked
    -- book already holds, which is what lets an unchanged file be called
    -- Unchanged without re-reading it.
    --
    -- A row *count* is not enough and was the first cut: a note rewritten on
    -- the device leaves the count identical, so the edit would be reported by
    -- the one scan that re-read the file and then never again. Both sides go
    -- through `DeviceDigest`, once.
    entry_digest TEXT NOT NULL,
    title       TEXT,               -- doc_props.title, as the matcher sees it
    authors     TEXT,
    ko_percent  REAL,               -- 0.0..=1.0
    ko_status   TEXT,               -- reading|abandoned|complete|<unknown>
    last_seen   INTEGER NOT NULL
);
CREATE INDEX idx_sidecar_seen_last_seen ON sidecar_seen(last_seen);
