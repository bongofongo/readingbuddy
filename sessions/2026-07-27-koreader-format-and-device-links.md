# 2026-07-27 — KOReader format investigation and device links (engine item 1)

Both halves of `docs/prompts/01-koreader-format.md`. 1a produced
`docs/koreader-format.md`; 1b added `summary`/`stats`/`percent_finished`
parsing, migration `0003_device_books.sql`, and md5-based sidecar→book
matching. The user's real KOReader library landed in `personal_data/Calibre/`
this session and became the primary evidence.

## The blocking question — answered: NO

> Does editing an annotation's note change its `datetime`?

`datetime` is the immutable creation stamp; edits write a **separate
`datetime_updated`**. So the identity hash is stable across a note edit, the
edit is currently dropped silently, and **item 2's targeted-update fix is
correct as designed** — no re-plan.

Three pieces of evidence, all in `docs/koreader-format.md` §1:

1. `readerannotation.lua:67` — `datetime = bm.datetime, -- creation time, not
   changeable`. `addItem:507` is the only assignment and it is `or`-guarded;
   `onAnnotationsModified:517` writes `datetime_updated` instead. The note path
   (`readerbookmark.lua:setBookmarkNote:1375`) never touches `datetime`.
2. 81 `datetime_updated` keys across 361 real annotations; all 55 note-bearing
   entries carry one, zero negative deltas, median +27 s.
3. **The clincher** — deltas alone can't rule out "rewritten on edit, and edits
   always happen seconds later". `David Copperfield` `[37]` was created
   20:31:11 and edited 20:31:31, *after* `[38]` was created at 20:31:14, and
   `[37]`'s `datetime` still reads 20:31:11.

## Decisions locked

- **Real fixtures: copy a subset into gitignored `real/`** (4 of 10 `.sdr`),
  rather than pointing the harness at `personal_data/`. Keeps the existing
  drop-in contract; no second fixture-location concept.
- **No KOReader desktop build.** Source + 361 real annotations already settle
  the question; the doc says so plainly rather than implying an experiment was
  run.
- **Parse `datetime_updated` now, persist in item 2.** Costs nothing here and
  saves item 2 reopening the parser, the fixtures and every golden.
- **Import records the `device_books` link itself** (`linked_by='auto'`, skipped
  under `dry_run`). The spec parks linking in item 3, but without a writer the
  md5 branch is unreachable and untested. Item 3 replaces the call site, not the
  table.
- **Unknown status warns.** `DiagnosticKind::UnknownDeviceStatus` — it is the
  only signal that KOReader grew a status we don't model, and silence would look
  exactly like success. It is also what makes the fixture's golden assert
  anything (`has_warnings`).
- **Fixtures take the `Gen-` prefix**, so the prompt's four are
  `Gen-Summary` / `Gen-Stats` / `Gen-Summary-Legacy` / `Gen-Summary-Unknown-Status`.

## Two corrections to the spec (`decisions.md` updated)

- **`stats.md5` does not exist.** On a 2024.11+ device `stats` is exactly
  `{title, authors, language, series, pages, highlights, notes,
  performance_in_pages}` — no `md5`, no `total_time_in_sec`. Grepping all 19
  files finds only the root `partial_md5_checksum`, 19 times. `decisions.md`
  said the link decision is "recorded against `stats.md5`"; it is recorded
  against the **root checksum**. Both fields stay `Option` on `KoStats` because
  a *legacy* sidecar is the likeliest place they still appear — the pre-DB
  statistics plugin predates the annotations format.
- **`summary.note` is never written.** Real in `bookstatuswidget.lua:565-586`,
  absent from all 10 files. Parse it; expect `None`. Any "import my review"
  feature will find nothing.

## Technical gotchas

- **`percent_finished` for a completed book is the bare integer `1`**, not
  `1.0`. A parser demanding a Lua float silently reads `None` and loses the
  progress. `Gen-Stats` pins it.
- **`pageno` drifts between saves with no user action** — 29→30, 30→31, 42→43 in
  *To the Lighthouse* after a re-render. Display metadata, not identity. Good
  that the hash excludes it; item 2's refresh must include it.
- **The annotations array is document order, not time order.** `datetime`
  inverts in 2 of 8 files (`David Copperfield` `[33]`/`[34]`). Never sort by
  `datetime` and assume file order; `parse_annotations` sorting by table index
  is right.
- **`metadata.*.lua.old` exists in 9 of 10 real `.sdr` dirs** —
  `docsettings.lua:340` backs up on every flush. Importing both would resurrect
  deleted highlights. `is_sidecar_file` already excludes them (`.lua.old` is not
  `.lua`) — *incidental*, so it is now guarded by
  `the_walker_ignores_the_old_backup_koreader_leaves_behind`.
- **The `.old` files are a free natural experiment.** Diffing one against its
  successor is where both the `pageno` drift and the
  `annotations_externally_modified` lifecycle came from.
- **`datetime_updated` is not a "has note" test** — a note written then cleared
  leaves the field behind with no `note` key.
- **PDF highlights cannot currently be imported.** On PDF, `pos0` is a *table*,
  not an xpointer, so `get_str(item, "pos0")` returns `None` and
  `entry_to_highlight` skips the entry. Not a regression, out of scope for item
  1, written into `docs/koreader-format.md` §6 rather than left to be
  rediscovered.
- **Sidecars are not always beside the book.** `DocSettings:getSidecarDir`
  supports `doc` / `dir` / `hash` layouts; the sibling-epub ISBN branch only
  works in `doc` mode. Another reason md5 goes first.
- **`doc_props.identifiers` carries a bare ISBN** on calibre-managed epubs —
  cheaper than opening the sibling file. Noted for item 3, not used.
- **KOReader version is not recorded anywhere.** Closest proxy is
  `cre_dom_version = 20240114`. Version-dependent behaviour must be inferred
  from which keys are present.
- **`Storage::pool()` is `pub(crate)`**, so an integration test cannot count
  rows directly. The harness asserts the link through the public
  `find_book_by_partial_md5` instead of widening the API for a test.
- **`link_device_book` deliberately does not `DO UPDATE` `linked_by`.** A
  library scan re-linking a file must not relabel a `manual` link as `auto`.
  Guarded by `an_automatic_relink_does_not_downgrade_a_manual_one`.
- **`Gen-Isbn-Match.epub` shows an 8-byte diff** on regeneration. Content and
  zip entry timestamps are identical; two consecutive runs are byte-stable. A
  dependency's deflate output drifted since the blob was committed — not a
  determinism bug.
- **`util.partialMD5`'s first offset is 256, not 0** (`1024 << -2`), so the
  first 256 bytes of a file are never hashed. Twelve 1 KiB samples, stopping at
  the first short read. Documented for item 12; not implemented.

## Verification

- `cargo test --workspace`: 322 passed, 0 failed. Clippy clean, fmt clean,
  `make ci` exit 0.
- The 12 pre-existing goldens changed **additively only** — three new null
  fields (`percent_finished`, `status`, `rating`). No behavioural drift.
- **Live end-to-end against the real library** (scratch data dir, books seeded
  offline via `epub` import): 96 highlights across 3 books, 3 `device_books`
  rows written as `auto`, re-import reported `matched_by=md5` with 0 inserted.
  Dry run wrote 0 links. 10 sidecars walked out of 19 `.lua`-ish files — the
  `.old` backups skipped on real data.
- `READINGBUDDY_REQUIRE_FIXTURES=1` confirms `real_exports_are_idempotent`
  actually runs ("verified 4 sidecar(s), 4 book(s) imported") rather than
  skipping.
- `personal_data/` and `real/` both confirmed gitignored before committing.

## Deferred

- **A generated realistic-device corpus tier** — logged under item 1 in
  `decisions.md`. The only realistic-scale coverage is the user's own library,
  which is personal and uncommittable, so it protects nobody else and vanishes
  on another machine. Tier 1 covers *shape*, tier 2 covers Gutenberg *text*;
  neither covers a *device library* (many books, hundreds of annotations,
  `datetime_updated`, mixed statuses, PDF sidecars, `.lua.old` siblings).
- PDF highlight import (`pos0` as a table).
- `doc_props.identifiers` as an ISBN source — item 3.
- `partialMD5` computation — item 12.
- Persisting status / rating / `percent_finished` — item 4's `readings` table.
  They ride in `BookImportStats` and the goldens meanwhile; **nothing was parked
  on `books`.**
- `insert_highlight` stays `ON CONFLICT DO NOTHING`; the refresh path is item 2.
