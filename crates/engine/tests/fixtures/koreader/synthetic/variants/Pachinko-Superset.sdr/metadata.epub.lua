-- Superset of ../Pachinko.sdr: the same three entries plus a fourth.
-- Used by `appends_only_new_on_partial_reimport` to prove a re-import of a
-- grown export appends only the new row. Lives under `variants/` because
-- fixture discovery is a NON-recursive read_dir, so this is invisible to
-- the golden loop and needs no golden of its own.
--
-- Synthetic KOReader sidecar: modern `annotations` layout (2024+).
-- Mixes a real highlight-with-note, a single-word highlight (flashcard
-- candidate), and a plain bookmark (no pos0) that import must skip.
return {
    ["annotations"] = {
        [1] = {
            ["chapter"] = "Chapter One",
            ["datetime"] = "2026-01-05 21:14:08",
            ["drawer"] = "lighten",
            ["color"] = "yellow",
            ["page"] = "/body/DocFragment[8]/body/p[12]/text().0",
            ["pageno"] = 42,
            ["pos0"] = "/body/DocFragment[8]/body/p[12]/text().0",
            ["pos1"] = "/body/DocFragment[8]/body/p[12]/text().57",
            ["text"] = "History has failed us, but no matter.",
            ["note"] = "Opening line - sets the whole register.",
        },
        [2] = {
            ["chapter"] = "Chapter Two",
            ["datetime"] = "2026-01-06 08:02:11",
            ["drawer"] = "lighten",
            ["pageno"] = 55,
            ["pos0"] = "/body/DocFragment[9]/body/p[4]/text().10",
            ["pos1"] = "/body/DocFragment[9]/body/p[4]/text().19",
            ["text"] = "pachinko",
        },
        [3] = {
            -- plain bookmark, no pos0: must be skipped
            ["datetime"] = "2026-01-06 09:00:00",
            ["pageno"] = 60,
            ["text"] = "dogear",
        },
        [4] = {
            ["chapter"] = "Chapter Three",
            ["datetime"] = "2026-01-07 10:00:00",
            ["drawer"] = "lighten",
            ["pageno"] = 70,
            ["pos0"] = "/body/DocFragment[10]/body/p[1]/text().0",
            ["pos1"] = "/body/DocFragment[10]/body/p[1]/text().12",
            ["text"] = "A brand new highlight added later.",
        },
    },
    ["doc_props"] = {
        ["authors"] = "Min Jin Lee",
        ["title"] = "Pachinko",
        ["language"] = "en",
    },
    ["partial_md5_checksum"] = "0d6ba6c47caf63b8b3d1a2b3c4d5e6f7",
}
