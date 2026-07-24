-- Notes gain a location anchor: where in the book the thought belongs. Both
-- nullable — a note may float free, sit on a device/pdf page, or (via the
-- existing highlight_id) hang off a specific highlight.
ALTER TABLE notes ADD COLUMN page INTEGER;      -- device/pdf page the note anchors to
ALTER TABLE notes ADD COLUMN location TEXT;     -- free-form: chapter, "loc 1234", %, etc.
