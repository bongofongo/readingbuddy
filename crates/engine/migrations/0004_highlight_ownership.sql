-- Make highlight ownership visible in the schema.
--
-- `note` was ambiguous: it is KOReader's, written on the device at capture (or
-- edited there later), and readingbuddy is not its origin. Renaming it says so,
-- and leaves room for the field that IS ours.
--
-- See docs/decisions.md ("Data ownership") and docs/spec-engine-01-03.md
-- (item 2).
ALTER TABLE highlights RENAME COLUMN note TO ko_note;

-- Ours. Never touched by import — a device refresh must not be able to
-- overwrite or delete what the reader wrote here.
ALTER TABLE highlights ADD COLUMN annotation TEXT;

-- The device value as of the last import.
--
-- Unused today, and deliberately added now rather than later: a future two-way
-- sync has to tell "the user changed it here" from "the device changed it
-- there", and that distinction is unrecoverable if only the merged result is
-- ever stored. Backfilling it after the fact would be a guess.
ALTER TABLE highlights ADD COLUMN last_seen_ko_note TEXT;
