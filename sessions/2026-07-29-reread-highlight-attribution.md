# 2026-07-29 — reread highlight attribution, and making `reading_id` visible

Picks up from `77190c0` on `main`. Started as a design conversation — "are
rereads a real feature right now?" — and turned into two bug fixes plus the
surfacing of a column that had been written and never read since migration
`0005`.

Branch `rereads-attribution`, PR rather than direct to `main` (asked for).

## What the audit found

Rereads were already first-class and more complete than the question assumed:
`readings` + `idx_readings_one_open`, `Storage::reread` / `rb progress --reread`,
`Book`'s four progress fields as projections of the current reading,
reflections/reviews anchored to a reading, Goodreads `Read Count` materialising
several, `merge_books` closing the older of two open readings as `abandoned`.

What was **not** real was the half the question was actually about:

- **`highlights.reading_id` was write-only.** `attribute_highlights` computed it;
  nothing read it. Not in `HIGHLIGHT_COLUMNS`, no field on `Highlight`, not on
  `HighlightDto`, nothing in the CLI or TUI. Attribution was therefore being
  computed correctly *and, for a while, incorrectly* with no way to observe
  either.
- **The attribution itself was wrong in two ordinary cases** (below).
- No highlight deletion exists anywhere (only `merge_books`' collision drop) —
  deliberately left alone, see Deferred.

## The bug

`attribute_highlights` `COALESCE`d an absent bound straight to ±8.64e12. That
makes an unstarted reading's window **contain every earlier reading's window**,
and since the match is "the latest window that holds it", the newest reading
collected the older readings' highlights and the older readings held none —
permanently, and with nothing on screen looking wrong.

Two ordinary doors in, both confirmed by scratch probe before writing anything:

- **Goodreads `Read Count > 1`.** Item 10 writes those readings with NULL
  `started_at` *by design* — the CSV has no start date and `goodreads.rs` refuses
  to invent one — closing each at its `Date Read`. So reading 2's window is
  `(-∞, date2]`, which contains reading 1 entirely. Probe: `old -> Some(2)`,
  `new -> Some(2)`. Every highlight on the last read; earlier reads empty.
- **`record_reading` / `open_reading(.., None, ..)`.** Same shape through the
  reread door. Probe: `first-read -> Some(2)` — the first read's highlight
  *moved* onto the reread. (`Storage::reread` itself passes `Some(now_unix())`,
  so the manual `rb progress --reread` path was never affected — which is
  probably why this survived.)

### The fix

An absent `started_at` **derives** a bound: an unstarted reading begins at the
latest `finished_at` of another reading of the same book that is not after its
own end, `+ 1`. With no such reading it really is −∞ — the one case the old
`COALESCE` had right.

- **No reading-order convention needed.** The derivation uses only the dates the
  rows already carry. An earlier attempt used a row-value tuple comparison
  (`(p.key, p.id) < (r.key, r.id)`) to define "previous"; the date-only form is
  both simpler and doesn't need `list_readings`' ordering to stay put.
- **`+ 1` is not an epsilon.** These are integer unix seconds, so it is the next
  representable value. It makes the derived start *exclusive* while an explicit
  one stays inclusive — a reading owns the instant it closed. Without it that
  second lies in both windows and the `ORDER BY` hands it to the newer reading,
  a one-second hole that only shows up on real device data where a highlight and
  a "finished" tap land in the same second.
- Rewritten as `WITH windows AS (…) UPDATE …`. SQLite has taken a `with-clause`
  on `UPDATE` since 3.8.3. The window arithmetic is unreadable inline in a
  correlated subquery and the `ORDER BY` would have needed a second copy of it.
- The `ORDER BY` survives as a **tie-break only**: derived windows are disjoint
  by construction, but a user can give two readings explicitly overlapping dates.

## What was surfaced

- `Highlight.reading_id`, `HIGHLIGHT_COLUMNS`, `row_to_highlight`.
- `Storage::highlights_for_reading` / `Engine::highlights_for_reading`.
- `HighlightDto.reading_id`; `Request::HighlightsForReading` + dispatch arm.
- **`rb highlights` groups by read** once a book has more than one, using
  `render::reading_line` and the same 1-based numbering `rb show` prints.
  Reading headers print **even when empty** — a read that marked nothing is a
  different fact from a read the list forgot. Unattributed ones last, under
  `not placed in a reading`, listed plainly. One reading prints flat.
- **TUI: a one-cell read gutter** on the highlight list (`1` / `2` / `·`), the
  notes list's `◆`/`◇` shape. `BookView` gained `readings` plus `read_number` /
  `shows_read_gutter`.

### Two shape decisions

- **`highlights_for_reading` deliberately cannot ask for the unattributed rows.**
  `reading_id IS NULL` is a property of the *book's* list, not of any reading,
  and such a method would need a book id anyway. The field is on the row
  precisely so grouping is the frontend's job.
- **The gutter is dropped entirely for a book read once**, and `shows_read_gutter`
  is asked once for the whole list rather than per row — deciding per row would
  leave an unattributed highlight flush against the border while its neighbours
  were indented. A column reading `1` on every row is a column that says nothing.

## Technical gotchas

- **A regression test that never saw the regression proves nothing.** All three
  new attribution tests were run against the *old* query (patched back in, via a
  scratch copy of the file) and confirmed failing, then restored. Worth the two
  minutes: the existing attribution tests all gave their readings explicit start
  dates, which is exactly why they were green through the bug.
- **The TUI crate cannot reach `sqlx` or `Storage::pool()`** — `pool` is
  `pub(crate)` and `sqlx` is not a tui dependency, even with `internals` on. A
  test needing hand-set reading dates has to go through `record_reading` on its
  own book rather than `UPDATE readings`.
- **`test_app`'s seeded reading is opened by `update_progress` at whatever "now"
  is on the machine running the suite.** Any test choosing dates around specific
  captures must create its own book, or it is date-dependent.
- **A drawn TUI line spans the whole terminal**, so the book object's glyphs are
  on the same line as the list row. Assertions have to cut the pane out (split on
  `│`, take the segment containing the needle) *and* `trim_end`. First attempt
  compared against the full 100-column line.
- **`contains` cannot see a gutter.** The text is identical with or without it;
  only the offset changes. The assertion has to be on what the row *starts*
  with — hence `row_body`, which strips the selection caret and its padding.
- **The section pane is ~22 columns** at 100×24 with the book object beside it,
  so fixture text longer than that is clipped before an assertion can see the
  end of it. Short highlight texts (`alpha`/`beta`/`gamma`) rather than a wider
  test terminal.
- `engine_facade.rs` needed `highlight` and `seed_book` added to its
  `use common::{…}` — the helpers exist and were simply not imported there.

## Verification

- Full gate: `cargo fmt --all --check`, `clippy --workspace --all-targets
  --locked -D warnings`, `cargo check --workspace --locked`, workspace tests.
- **711 → 718 passing / 0 failed** (711 is the previous session's count). Seven
  new tests.
  - 3 × `storage::readings` — the Goodreads shape, the unstarted reread, and the
    exclusive/inclusive boundary. All three verified failing on the old query.
  - 2 × `engine_facade` — `reading_id` reaches the facade; `highlights_for_reading`
    returns that read's share and not the unattributed ones.
  - 2 × `app` — the gutter appears at two readings and not at one.
- `cargo-nextest` is not installed on this machine, so the Makefile degrades to
  plain `cargo test`; steps were run individually so a fmt failure could not hide
  the rest.

## Deferred

Explicitly, by the user — "the wiping thing" is not a problem right now.

- **Wiping / archiving highlights.** The hard part is not plumbing: KOReader owns
  highlights and `insert_highlight` is `ON CONFLICT DO NOTHING` on
  `identity_hash`, so a locally deleted row **comes straight back on the next
  scan** — no conflict, so it re-inserts. A wipe against a live origin is a fight
  the origin wins. Three shapes considered: a `hidden_at` column (archive, keeps
  "conflicts resolve toward the origin" true), a tombstone table keyed on
  `identity_hash` (real deletion, but a local assertion overriding the origin,
  which `docs/decisions.md` argues against), or **reading-scoped discard** —
  "drop this reread and its highlights", using the `ON DELETE SET NULL` already
  there. The third is the one the original question actually had the shape of.
- **`merge_readings` / `split_reading`.** Discussed and argued *for*, as
  user-driven scissors rather than importer behaviour. The rejected version was
  auto-deferring each import stream into its own reading: a reading is a
  user-meaningful object (*the time I read this*), and letting streams mint them
  makes the reading count an artifact of how many places data arrived from —
  import one book from calibre, a CSV and the device and you have three
  "readings" of a book read once. Provenance already has homes that are not the
  reading (`highlights.source`, `readings.source`, `external_ids`,
  `device_books`). Cheaper than `merge_books` if built: `reading_id` is not in
  the identity hash, so it is a pure repoint; the only real question is what
  happens to two reflections.
- **Which file a reading was on.** Nothing records it — `book_files` (`0010`) and
  `device_books` both exist, neither links to `readings`. A
  `readings.file_sha256` would answer "these came from the paperback, those from
  the epub" and give attribution a second signal besides dates.
- **No re-attribution sweep.** `attribute_highlights` only recomputes when an
  import touches that book, so an existing library would keep the wrong
  attribution until re-imported. Confirmed with the user that **no real reread
  data exists yet**, so no one-shot command or migration-time sweep was written.
  It becomes necessary the moment a database predates this fix with real rereads
  in it.
