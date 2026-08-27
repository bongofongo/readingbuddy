-- The readers readingbuddy has installed its plugin onto (item 15a).
--
-- ## Why a table at all, when the plugin is a directory on a mount
--
-- Without this, "linked" is only ever a fact about a volume that happens to be
-- plugged in right now — every frontend could answer it by walking a mount, and
-- none of them could answer it when the reader is in a bag. Pairing is a
-- relationship, and a relationship the app cannot recall between sessions is
-- not one. The device's own copy of the same facts lives in
-- `readingbuddy.koplugin/pairing.lua`; this is our half.
--
-- ## `device_id` is identity; `last_mount_path` is a breadcrumb
--
-- Mount points move — between sessions, between machines, and between a Kobo
-- plugged into one USB port and the same Kobo plugged into another. So the
-- unique key is the uuid we mint at install time and write into `pairing.lua`,
-- and **nothing may ever join on `last_mount_path`**. It exists so a frontend
-- can say "last seen at /run/media/oliver/KOBOeReader", which is a sentence
-- about the past and is allowed to be stale.
--
-- ## The token
--
-- 32 random bytes, hex, minted here and written into the device's
-- `pairing.lua`. There is no listener yet and `pairing.lua` deliberately
-- carries no host or port (see `docs/prompts/15a-the-plugin-and-the-pairing.md`
-- for that argument) — the token exists so that when item 15b gives the plugin
-- an address, the user types nothing. Pairing happened over USB, once, while
-- the reader was in their hand.
--
-- **It is never logged, at any level.** `CLAUDE.md`'s tracing rule names
-- highlight text, note bodies and search queries; this joins them, and it is
-- stricter than they are — not even `trace!`.
--
-- ## `plugin_version` is what we installed, not what is there now
--
-- The authority on the installed version is the device's own `_meta.lua`, read
-- back through the sidecar sandbox. This column records what *we* put there, so
-- that a plugin upgraded by some other means is visible as a disagreement
-- rather than silently assumed to be ours.
CREATE TABLE paired_devices (
    id              INTEGER PRIMARY KEY,
    device_id       TEXT NOT NULL UNIQUE,
    label           TEXT,
    token           TEXT NOT NULL,
    plugin_version  INTEGER NOT NULL,
    installed_at    INTEGER NOT NULL,
    last_mount_path TEXT,
    last_seen_at    INTEGER
);
