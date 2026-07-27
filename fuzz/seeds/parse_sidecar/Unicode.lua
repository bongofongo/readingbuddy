-- Synthetic KOReader sidecar: non-ASCII title/author/body (CJK, accents,
-- emoji). Guards Lua string round-trip through the sandboxed VM and title
-- normalization in match_book.
return {
    ["annotations"] = {
        [1] = {
            ["chapter"] = "上",
            ["datetime"] = "2026-03-01 12:00:00",
            ["drawer"] = "lighten",
            ["pageno"] = 12,
            ["pos0"] = "/body/DocFragment[2]/body/p[1]/text().0",
            ["pos1"] = "/body/DocFragment[2]/body/p[1]/text().18",
            ["text"] = "私はその人を常に先生と呼んでいた。",
            ["note"] = "語り手と先生の距離感 — café ☕ で読了",
        },
        [2] = {
            ["chapter"] = "中",
            ["datetime"] = "2026-03-02 12:00:00",
            ["drawer"] = "lighten",
            ["pageno"] = 88,
            ["pos0"] = "/body/DocFragment[4]/body/p[2]/text().0",
            ["pos1"] = "/body/DocFragment[4]/body/p[2]/text().22",
            ["text"] = "naïveté, résumé, and a café façade — déjà vu 🌊",
        },
    },
    ["doc_props"] = {
        ["authors"] = "夏目 漱石",
        ["title"] = "こころ",
        ["language"] = "ja",
    },
    ["partial_md5_checksum"] = "c0ffee00c0ffee00c0ffee00c0ffee00",
}
