---
title: The KOReader Sidecar Format
date: 2026-07-27
spec: docs/spec-engine-01-03.md (item 1a)
sources: KOReader master @ 2026-07-27; 10 real `.sdr` exports (361 annotations)
---

# The KOReader sidecar format

What a `.sdr` sidecar actually contains, why our parser reads it the way it
does, and the two places our own spec was wrong about it.

Three sources, in the spec's order of authority:

1. **KOReader's own source.** Fetched from `koreader/koreader@master` on
   2026-07-27. Line numbers below are against that revision. This is the spec; a
   fixture is only ever evidence.
2. **Real sidecars.** The user's own Kobo/Kindle-side calibre library, harvested
   from `personal_data/Calibre/` — 10 `.sdr` directories, 19 files, 361
   annotations, spanning 2025-10-02 to 2026-07-27.
3. No third source was needed.

**No KOReader desktop build was installed.** The spec offers minting our own
sidecars as source 2; the user's device export supersedes it for realism, and
the blocking question below is settled by source-reading plus a natural
experiment already present in the real data (§1.3). This is recorded rather than
glossed: nothing here rests on a fixture we wrote ourselves.

---

## 1. The blocking question

> Does editing an annotation's note in KOReader change that annotation's
> `datetime`?

**No.** `datetime` is the creation timestamp and is never rewritten. Edits are
recorded in a *separate* field, `datetime_updated`.

This is the answer item 2 was waiting on. It means our identity hash —
`sha256(book_id | ko_datetime | pos0 | text)`, `storage/highlights.rs:20` — is
stable across a note edit, that the edit is therefore currently **dropped
silently** on re-import, and that item 2's targeted-update fix is correct as
designed. No re-planning of the hash is required.

### 1.1 Source

`frontend/apps/reader/modules/readerannotation.lua:66-68`, the table
`buildAnnotation` returns — the single constructor every annotation passes
through:

```lua
    return { -- annotation
        datetime         = bm.datetime, -- creation time, not changeable
        datetime_updated = nil,         -- last modification time
```

The comment is the maintainers' own, and the code holds them to it:

- `addItem` (`:506-507`) is the only assignment, and it is conditional:
  ```lua
  function ReaderAnnotation:addItem(item)
      item.datetime = item.datetime or os.date("%Y-%m-%d %H:%M:%S")
  ```
- Every later mutation routes through `onAnnotationsModified` (`:515-518`),
  which writes the *other* field:
  ```lua
  function ReaderAnnotation:onAnnotationsModified(items)
      if items.index_modified == nil or items.modify_datetime then -- not needed when annotation added or removed
          items[1].datetime_updated = os.date("%Y-%m-%d %H:%M:%S")
      end
  ```
- The note-edit path reaches exactly that. `readerhighlight.lua:addNote` →
  `ReaderHighlight:editNote` → `ReaderBookmark:setBookmarkNote`
  (`readerbookmark.lua:1375`). Its Save button assigns
  `annotation.note = value` and raises `AnnotationsModified`. It never touches
  `datetime`.

Corroborating: KOReader's *own* annotation-import merge treats the two fields as
distinct, taking `datetime_updated or datetime` as "last touched" while keying
identity elsewhere (`readerannotation.lua:312-327`).

### 1.2 Real sidecars

Across 361 annotations in 8 files (the 2 PDF sidecars have empty `annotations`):

| | count |
|---|---|
| annotations total | 361 |
| with `pos0` (real highlights) | 347 |
| with `note` | 55 |
| with `datetime_updated` | 81 |

All 55 note-bearing annotations carry a `datetime_updated`, and it is later than
`datetime` in every single case — zero negative deltas.

| | count | min | median | max |
|---|---|---|---|---|
| with `note` | 55 | 9 s | 27 s | 140 s |
| `datetime_updated` but no `note` | 26 | 2 s | 5 s | 28 s |

The note deltas are the time it takes to type a note after making the highlight.
The no-note deltas are style/colour/extent tweaks, or a note added and then
cleared.

### 1.3 The clincher

Deltas alone can't distinguish "datetime is immutable" from "datetime is
rewritten on edit, and edits always happen seconds later". This pair does.

`David Copperfield - Charles Dickens (40).sdr/metadata.epub.lua`, entries
`[37]` and `[38]` (the array is in document order, so `[38]` is later in the
book):

```lua
        [37] = {
            ["datetime"] = "2025-10-02 20:31:11",
            ["datetime_updated"] = "2025-10-02 20:31:31",
            ["pos0"] = "/body/DocFragment[14]/body/div/p[91]/text().47",
        },
        [38] = {
            ["datetime"] = "2025-10-02 20:31:14",
            ["pos0"] = "/body/DocFragment[14]/body/div/p[91]/text().177",
        },
```

`[37]` was created at 20:31:11. `[38]` was created at 20:31:14. `[37]` was then
**modified at 20:31:31** — three seconds after `[38]` already existed — and
`[37]`'s `datetime` still reads 20:31:11. If an edit rewrote `datetime`, it
would read 20:31:31.

### 1.4 What follows for us

- `datetime` is safe as identity material. Keep it in the hash.
- `datetime_updated` is the device's change signal. Item 1 parses it into
  `NewHighlight::ko_datetime_updated`; item 2 persists it. Coalesce it —
  `datetime_updated or datetime` — for "last touched".
- **`datetime_updated` is not a "has note" test.** A note that was written and
  then cleared leaves the field behind with no `note` key.

---

## 2. Two corrections to `docs/spec-engine-01-03.md`

### 2.1 `stats.md5` does not exist

The spec's `KoStats` field list is `title, authors, pages, series, language,
md5, total_time_in_sec, highlights, notes`, and `docs/decisions.md` says the
device-link decision is "recorded against `stats.md5`".

On a 2024.11+ device, `stats` is exactly:

```lua
    ["stats"] = {
        ["authors"] = "Haruki Murakami",
        ["highlights"] = 45,
        ["language"] = "en",
        ["notes"] = 38,
        ["pages"] = 2177,
        ["performance_in_pages"] = {},
        ["series"] = "N/A",
        ["title"] = "1Q84",
    },
```

Identical key set in all 10 files. **No `md5`, no `total_time_in_sec.`** A grep
for any md5-shaped key over all 19 files returns only `partial_md5_checksum`,
19 times — the root key, once per file.

Why: the statistics plugin moved its per-book bookkeeping into
`statistics.sqlite3`. Nothing in current KOReader *writes* `stats` into a
sidecar; the plugin only *reads* it, once, to migrate a pre-DB sidecar
(`plugins/statistics.koplugin/main.lua:674`). The `stats` blocks in these files
are residue that survives every rewrite because DocSettings serialises whatever
it loaded. `md5` and `total_time_in_sec` are fields of the plugin's `book_stats`
record (`:596-600`), not of the sidecar subtable.

**Consequence:** `device_books` keys on the root **`partial_md5_checksum`**, not
`stats.md5`. `KoStats` keeps `md5` and `total_time_in_sec` as `Option` so an
older sidecar that has them still round-trips, but nothing may depend on them.

Also worth knowing, since it is the only thing `stats` is good for: the counters
partition the annotations. `highlights` counts entries *without* a note, `notes`
counts entries *with* one, and bookmark-type entries are in neither. Verified
arithmetically on all 8 files with annotations — 1Q84: 86 entries = 3 bookmarks
+ 45 + 38. Both counters are stale the moment the sidecar is edited off-device.

### 2.2 `summary` has no `note`

The spec asks whether `summary.note` is the user's review. It is — but this
version of KOReader never writes it. Observed fields, all 10 files:

```lua
    ["summary"] = {
        ["modified"] = "2026-07-22",
        ["rating"] = 5,
        ["status"] = "complete",
    },
```

`note` is absent from every one. The field is real and current —
`bookstatuswidget.lua:565-586` is a live input dialog writing `self.summary.note`
— so we parse it. But a book-level review is not something we can expect to find
in the wild, and any feature that assumes one will find nothing.

---

## 3. Field reference

### 3.1 Root keys

Present in every sidecar, EPUB and PDF alike: `annotations`, `doc_pages`,
`doc_path`, `doc_props`, `partial_md5_checksum`, `percent_finished`, `stats`,
`summary`, plus a couple of dozen UI-preference keys we ignore
(`config_panel_index`, `highlight_color`, `page_overlap_style`, the `copt_*`
family on EPUB, the `kopt_*` family on PDF).

The file is one `return { ... }` preceded by a comment holding the **on-device**
path, keys sorted alphabetically, 4-space indent, trailing commas:

```lua
-- /mnt/us/Calibre/Children of Ash and Elm - Neil Price; (35).sdr/metadata.epub.lua
return {
```

**`partial_md5_checksum`** — 32-char lowercase hex, the one stable book
identifier the sidecar carries. Already parsed (`koreader.rs:78`). Its algorithm
is §5.

**`percent_finished`** — float `0.0..=1.0`, written on close by
`readerrolling.lua:338` (EPUB) and `readerpaging.lua:163` (PDF). Serialised at
full precision (`0.99770326136886`), but a completed book is written as the bare
integer **`1`** — a parser must not require a decimal point. It also drifts on
re-render: `0.22745098039216` → `0.2265625` in *To the Lighthouse*, purely
because the page count changed.

**`doc_pages`** — integer, equals `stats.pages` in the current files; they
diverged by one in a `.old` before a re-render, so do not treat them as the same
number.

**`doc_props`** — `title`, `authors`, and on EPUB usually `language` and
`identifiers`. PDF sidecars carry only `title` and `authors`. `identifiers` is a
newline-joined string that on calibre-managed books includes a bare ISBN:

```lua
        ["identifiers"] = "calibre:35\
uuid:9d6c4cee-5634-4790-af6e-d2c292f66a63\
9780465096992",
```

Not used yet. It is a cheaper ISBN source than opening the sibling epub, and
worth revisiting when item 3 needs to create books from sidecars.

**`annotations_externally_modified`** — a boolean KOReader sets when it detects
the sidecar was edited off-device (KOHighlights and friends), and **deletes**
after reconciling (`readerannotation.lua:112-157`). Observed appearing in one
`.old` and gone from its successor. Any writer we ever build must set it.

### 3.2 `annotations` entries

Complete per-entry key inventory over all 361 entries, with occurrence counts:

`text` 361, `pageno` 361, `page` 361, `datetime` 361, `chapter` 361, `pos1` 347,
`pos0` 347, `drawer` 347, `color` 347, `datetime_updated` 81, `note` 55. **No
other keys appear** — no id, no `pboxes` (those are PDF-only, and both PDF
sidecars here are empty), no sort key.

A highlight with a note, showing everything:

```lua
        [12] = {
            ["chapter"] = "Chapter Ten",
            ["color"] = "gray",
            ["datetime"] = "2026-07-27 09:18:26",
            ["datetime_updated"] = "2026-07-27 09:20:46",
            ["drawer"] = "lighten",
            ["note"] = "veils over self expression […]\
\
",
            ["page"] = "/body/DocFragment[18]/body/p[135]/text().158",
            ["pageno"] = 235,
            ["pos0"] = "/body/DocFragment[18]/body/p[135]/text().158",
            ["pos1"] = "/body/DocFragment[18]/body/p[137]/text().87",
            ["text"] = "“Do you think I imagine for one moment […]",
        },
```

A bookmark-type entry — 14 of the 361 — has no `pos0`/`pos1`/`color`/`drawer`,
and a synthetic `text` of the form `"in <chapter>"`:

```lua
        [13] = {
            ["chapter"] = "Chapter Eleven",
            ["datetime"] = "2026-07-27 12:57:09",
            ["page"] = "/body/DocFragment[19]/body/p[123]/text().0",
            ["pageno"] = 252,
            ["text"] = "in Chapter Eleven",
        },
```

This vindicates `entry_to_highlight`'s rule (`koreader.rs:113-117`) that a real
highlight is one with a `pos0`; without it we would import 14 fake highlights
whose text is a chapter name.

Things that will bite a consumer:

- **Multi-line strings use Lua's `\<newline>` escape**, a literal backslash then
  a real newline — not `\n`. mlua handles it; a regex parser would not.
- **`page` is byte-identical to `pos0`** whenever `pos0` exists — verified on all
  347, **all of which are EPUB**, where both are cre xpointers. On PDF `page` is
  a number and `pos0` is a *table*, so the two are not even the same Lua type and
  the identity above says nothing about that case. §6 has what is and is not
  settled there; the short version is that a consumer must test `pos0`'s **type**
  before reading it as a string, because `get`ting it as one succeeds on a number
  by Lua coercion and fails on a table by returning nothing at all.
- **The array is document order, not time order.** `pageno` is monotonically
  non-decreasing in all 8 files, but `datetime` inverts — `David Copperfield`
  `[33]`/`[34]` read `19:44:53` then `19:44:51`, and `Norwegian Wood` has two
  more. Never sort by `datetime` and assume you have reproduced file order.
  `parse_annotations` sorts by the table index, which is the right key.
- **`pageno` drifts between saves.** `To the Lighthouse` moved 29→30, 30→31,
  42→43 on a re-render with no user action. It is display metadata, not identity
  — which is why the identity hash excludes it and item 2's refresh includes it.
- **Timestamps are naive device-local wall clock**, `"%Y-%m-%d %H:%M:%S"`, no
  timezone or offset, from a device clock we do not control.
- `color` was `"gray"` in 100% of entries; `drawer` was `"lighten"` (335) or
  `"underscore"` (12).

### 3.3 `summary`

| field | type | notes |
|---|---|---|
| `status` | string | `"reading"` \| `"abandoned"` \| `"complete"` |
| `rating` | integer 1–5 | **omitted when unset** |
| `note` | string | the user's review. Never written in practice (§2.2) |
| `modified` | string | `"%Y-%m-%d"` — date only, unlike annotation datetimes |

The status set is `bookstatuswidget.lua:507`, the toggle's `args`:

```lua
        toggle = { _("Reading"), _("On hold"), _("Finished"), },
        args = { "reading", "abandoned", "complete", },
```

Lowercase. The doc-comment at `bookstatuswidget.lua:37-41` showing
`["status"] = "Reading"` is stale and should not be believed. `readerstatus.lua:214`
confirms the same two values on the mark-as-finished path.

Observed here: `reading` (4 books), `complete` (6). **No `abandoned`** — so our
handling of it is source-derived, not observed.

`rating` is deleted rather than zeroed when the user clears it
(`bookstatuswidget.lua:220-222` sets it to `nil` when the tap lands on star 1
and the rating was already 1). **Missing means unrated; it does not mean 0.**

Both `onChangeBookStatus` (`:197-199`) and `readerstatus.lua:215` stamp
`summary.modified` on every status change.

Nothing here is persisted by item 1 — `readings` (item 4) is where status,
rating and `percent_finished` belong. They ride in `BookImportStats` so the
import can report them.

### 3.4 The legacy format

Pre-2024 sidecars have no `annotations`. Instead:

- `highlight[pageno][idx]` — the highlighted passages.
- `bookmarks[idx]` — where a user note lives, joined to its highlight by
  `datetime`. Confusingly, in a bookmark `text` is the *note* and `notes` is the
  *highlighted passage*; `bookmark_notes` (`koreader.rs:181-194`) already
  handles that inversion and skips bookmarks that merely echo their passage.

`ReaderAnnotation:migrateToAnnotations` (`:159`) converts these on first open of
a book by a modern build and deletes the old keys, which is why `annotations`
winning over `highlight` (`koreader.rs:87-94`) is the correct precedence: a file
carrying both is mid-migration, and `annotations` is the newer truth.

**No legacy sidecar exists in the user's library.** All 10 are modern; a grep
for `["highlight"]` and `["bookmarks"]` returns zero across all 19 files.

Does the legacy format carry `summary`, `stats` and `percent_finished`? **Yes —
source-derived, not observed.** All three are DocSettings *root* keys written by
subsystems that never look at the annotations layout: `summary` by
`bookstatuswidget`/`readerstatus`, `percent_finished` by
`readerrolling`/`readerpaging`, `stats` by the pre-DB statistics plugin — which
is *older* than the annotations format, so a legacy sidecar is in fact the most
likely place to find a `stats` block with `md5` and `total_time_in_sec` in it.
Our parser reads all three from the root before dispatching on layout, so this
costs nothing. `Gen-Summary-Legacy.sdr` pins it.

---

## 4. Files on disk

`<Book Name>.sdr/metadata.<ext>.lua`, where `<ext>` is the document's own
extension — `DocSettings.getSidecarFilename` (`docsettings.lua:143-146`) does a
literal `doc_path:match(".*%.(.+)")`. So `metadata.epub.lua`, `metadata.pdf.lua`,
and in principle anything else. `is_sidecar_file` (`koreader.rs:299`) matches on
`metadata.` + `.lua`, which is the right shape.

**`.old` backups.** Every flush writes one first — `docsettings.lua:340`,
`LuaSettings:backup(sidecar_file) -- "*.old"`. 9 of the user's 10 `.sdr` dirs
contain a `metadata.epub.lua.old`. These must not be imported: they are a
previous state of the same annotations, and importing both would resurrect
highlights the user deleted. Our walker ignores them because `.lua.old` does not
end in `.lua` — incidental, but correct, and now guarded by test.

They are also a free natural experiment, and §1.3 and the `pageno`-drift
observation both came out of diffing one against its successor.

**Sidecars are not always beside the book.** `DocSettings:getSidecarDir`
(`:118`) supports three locations — `doc` (beside the file, the default and what
we see here), `dir` (a central folder), and `hash` (filed under the
`partial_md5_checksum`, `DocSettings.getSettingsArcFile` at `:609`). The
sibling-epub ISBN branch in `match_book` only works in `doc` mode. That is
another reason the md5 branch goes first.

**Version is not recorded.** No `version`, `koreader_version` or `git_commit`
key anywhere. The closest proxy is `cre_dom_version = 20240114` on EPUB
sidecars. Combined with `datetime_updated`, per-entry `color`/`drawer`,
`partial_rerendering`, `handmade_flows_*` and `annotations_externally_modified`,
this library is a **2024.11-or-later** build. Any version-dependent behaviour we
add has to be inferred from which keys are present, never read.

---

## 5. `util.partialMD5` — implemented in `crates/engine/src/partial_md5.rs`

`frontend/util.lua:1111-1128`:

```lua
function util.partialMD5(filepath)
    if not filepath then return end
    local file = io.open(filepath, "rb")
    if not file then return end
    local step, size = 1024, 1024
    local update = md5()
    for i = -1, 10 do
        file:seek("set", lshift(step, 2*i))
        local sample = file:read(size)
        if sample then
            update(sample)
        else
            break
        end
    end
    file:close()
    return update()
end
```

MD5 over up to twelve 1024-byte samples taken at offsets `lshift(1024, 2*i)` for
`i = -1..10` — that is **0**, 1 Ki, 4 Ki, 16 Ki, 64 Ki, 256 Ki, 1 Mi, 4 Mi,
16 Mi, 64 Mi, 256 Mi, 1 Gi. It stops at the first offset that reads nothing, so a
small file contributes fewer samples.

**The first offset is 0, and an earlier draft of this section said 256.** That
correction is the one thing here worth remembering. `lshift` is LuaJIT's BitOp,
which works on 32-bit integers and takes the shift count **modulo 32**: it is
not an arithmetic shift, and a negative count is not a right shift. So
`lshift(1024, -2)` is `lshift(1024, 30)`, which is `2^40` truncated to 32 bits —
`0`. Reading `2*i = -2` as `1024 >> 2` gives 256, which is wrong, and wrong in a
way nothing catches until a checksum disagrees.

Settled by the device itself, not by re-reading the Lua: the three checksums
recorded in this document — `8cb32bca81b36ca0816851073e5661d3`,
`a5b01da92a68bbbb6d88c12483cf3b56`, `25dc3d7e5bd746db64267cff902d3edd` — all
reproduce from offset 0 over the files KOReader computed them from, and none
reproduces from 256. `partial_md5::tests::agrees_with_the_device` is that check,
kept.

A corollary of the first window starting at 0: only a genuinely **empty** file
hashes to the MD5 of nothing (`d41d8cd98f00b204e9800998ecf8427e`). Under the
256 misreading, every file below 256 bytes would have.

Cheap and stable, but it is a sampling hash, not a content hash — two files
identical at those twelve windows collide. That is fine for its three jobs
(dedup, sidecar↔book matching, and joining into the device's
`statistics.sqlite3`) and not fine as a content address; `docs/decisions.md`
already puts sha256 in that role.

---

## 6. Open questions

- **`abandoned` is unobserved.** Handled from source. If a real one ever turns
  up, check the spelling before trusting this document.
- **PDF annotations are still unobserved, and item 36 did not change that.**
  Both PDF sidecars here have empty `annotations`, so no PDF highlight KOReader
  wrote has ever been read on this machine. Read the next three sentences as one
  claim, because the useful part is the seam between them.

  **Settled:** on a paging document — PDF, DjVu — `pos0`/`pos1` are **tables**
  and not xpointer strings. A scanned page has no text stream to point into, so
  the anchor is a page plus coordinates; this is source-derived, from the same
  split that gives `readerrolling` a cre xpointer and `readerpaging` a page
  number. `page` is a number on PDF for the same reason.

  **Not settled, and do not treat the fixture as evidence:** *what keys that
  table carries.* `Gen-Pdf-Anchors.sdr` writes `page`/`x`/`y`/`zoom`/`rotation`
  with a sibling `pboxes` array, and **those key names are a reconstruction, not
  an observation** — nobody here has ever seen the file they are modelling. They
  are safe to have in a committed fixture only because **nothing reads them**:
  `koreader::anchor` branches on the value being a Lua table and on nothing
  inside it, so no golden can bless a key name, and if a real device turns out to
  write `pos` or `rect` instead, neither the fixture nor the engine has to
  change. A later thread wanting the *inside* of that table — to store a PDF
  anchor rather than merely count it — needs a real sidecar first, and this
  document is not it.

  **We still cannot import a PDF highlight. We no longer do it in silence.**
  Those entries used to hit `get_str(item, "pos0")` → `None` → `?` → dropped,
  with no count and no diagnostic, so a PDF library imported nothing and gave no
  reason. They are now counted (`KoSidecar::unsupported_anchors`) and reported as
  one `DiagnosticKind::SidecarAnchorsUnsupported` per **file**, carrying the
  number — not one per entry, which on a 300-highlight PDF would have replaced
  silence with noise. A plain bookmark, and an entry with no `text`, are
  deliberately *not* in that count: neither is a highlight that went missing.
- **`stats.md5` on a genuinely old sidecar** — asserted from source, never seen.
  `KoStats` parses it; nothing depends on it.
