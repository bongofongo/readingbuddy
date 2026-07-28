# Prompt — Item 4: the `readings` table and the progress migration

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/engine-readings`).

---

Read `docs/spec-engine-04-07.md` (item 4) and `docs/decisions.md` before
starting. `CLAUDE.md`'s **Engine standards** section is binding.

**Depends on nothing. Blocks items 6b and 7**, so it goes first and merges
first. Owns migration **`0005`** — no other thread may take that number.

## The problem

`books` carries `current_page`, `finished`, `date_started`, `date_finished`
(`migrations/0001_init.sql:19-22`). That models one reading of one book, and
rereads are real. It also puts reading state inside `upsert_book`'s
provider-merge path, where a `finished = MAX(excluded.finished, books.finished)`
clause (`storage/books.rs:106`) has to exist to stop a metadata refresh from
un-finishing a book — a clause that should never have been needed.

Meanwhile the KOReader import already parses the device's status, rating and
`percent_finished` and then **throws them away**, because there is nowhere
correct to put them (`koreader.rs:421-431`).

`readings` fixes all three.

## The work

### Migration `0005_readings.sql`

The full SQL is in the spec. The order inside it matters: create `readings`,
add `highlights.reading_id`, **back-fill from `books`**, then
`ALTER TABLE books DROP COLUMN` the four.

- `CREATE UNIQUE INDEX idx_readings_one_open ON readings(book_id) WHERE finished_at IS NULL;`
  is the "at most one open reading per book" invariant. It is an index, not a
  convention, and `open_reading` must surface its violation as
  `EngineError::InvalidInput` rather than a raw sqlx error.
- `ko_status` / `ko_percent` / `ko_rating` are the **device-owned mirror**, and
  they follow the seam item 2 established: **straight assignment, never
  `COALESCE`**. A sidecar is the complete state. The user's own rating is a
  Review's (item 7) and does not live here.
- `DROP COLUMN` needs SQLite ≥ 3.35, which sqlx's bundled libsqlite3 has. This
  is the **first destructive migration in the repo** — before merging, run it
  against a *copy* of a real `database/app.db` and confirm the progress data
  landed in `readings`. Do not test it only on `sqlite::memory:`.

### Storage

New `crates/engine/src/storage/readings.rs`, registered in `storage/mod.rs:1-11`.
Signatures are in the spec. Two of them carry reasoning worth repeating:

- **`update_progress` keeps its current signature** (`storage/books.rs:199`) so
  its two callers change only their module path —
  `crates/cli/src/commands/book.rs:88` and `crates/tui/src/app.rs:954-968`,
  `:1274-1287`. It opens a reading if none is open.
- **`set_device_state` returns whether anything changed**, exactly like
  `refresh_device_fields` in `storage/highlights.rs`. Share the comparison
  predicate the way `DEVICE_FIELDS_DIFFER` is shared, so a preview can never
  disagree with the write it previews.
- **`attribute_highlights`** assigns `reading_id` by matching `ko_datetime` into
  a reading's date window. When no window contains it, **leave it `NULL`** —
  KOReader's sidecar is per-file and a reread appends to it, so the device
  cannot supply this attribution. Unattributed is correct, not a gap to paper
  over.

### `Book` keeps all four fields, as projections

This is the decision that keeps the diff small, so do not skip it and update
callers instead.

`Book` (`book.rs:24-27`) keeps `current_page`, `finished`, `date_started`,
`date_finished`. `BOOK_COLUMNS` (`storage/books.rs:45-48`) stops naming
`books.*` for those four and selects them through a `LEFT JOIN` on the open
reading, falling back to the most recent reading for `finished` /
`date_finished`. `row_to_book` (`:73-76`) then needs no change, and none of
these consumers change either:

- `crates/cli/src/render.rs:9-14`, `:45-46`
- `crates/tui/src/ui/library.rs:63-83`, `crates/tui/src/ui/book.rs:128-140`
- `crates/cli/src/commands/note.rs:31`, `crates/tui/src/app.rs:779`

What does change is every writer:

- **`upsert_book` (`storage/books.rs:87-154`)** — the four columns leave the
  INSERT list, the set clause and the binds. **Delete the `finished = MAX(...)`
  merge at `:106` entirely.** It was correct while progress lived on `books`; a
  provider upsert has no business touching reading state at all. Everything else
  keeps its `COALESCE` no-clobber — that pattern is still right for providers.
- **`merge_books` (`:253-444`)** — repoint `readings.book_id` inside the
  existing transaction, replacing the fold at `:410-413`. Two open readings
  would violate the partial unique index: **close the older one**
  (`finished_at` = its `last_modified`, `status = 'abandoned'`) rather than
  deleting it — deleting loses a real reading. Respect the transaction rule at
  `:262-264`: nothing inside may call back through `self`, because an in-memory
  pool has one connection and a nested acquire deadlocks.
- **`list_books` / `BookSort::Progress` (`:186-197`)** — order by the joined
  `current_page`.

### The import finally persists device state

In `import_into` (`koreader.rs:723`), after the highlight loop:
`set_device_state` from `sc.summary` / `sc.percent_finished`; map the status
(`Complete` closes the active reading, `Reading` leaves it open, `Abandoned`
sets `status = 'abandoned'`, `Other(_)` leaves ours alone — the
`UnknownDeviceStatus` diagnostic already fires at `:739-747`); then
`attribute_highlights`. **Nothing writes under `dry_run`**, the rule
`link_device_book` already follows at `:684`.

An import opens a reading when the sidecar carries device state, with
`source = 'koreader'`, started at the earliest `ko_datetime` seen. A sidecar
with neither `summary` nor `percent_finished` opens nothing.

### CLI

`progress` gains `--reread` (close the open reading, open a new one) and names
the reading it touched. `show` lists the reading history.

## Tests

- Back-fill round-trip: a book with progress before the migration has exactly
  one reading with the same values after.
- The partial unique index refuses a second open reading.
- **A provider `upsert_book` never changes reading state.** This asserts the
  *absence* of the retired `finished = MAX` behaviour — without it, that clause
  comes back the next time someone "fixes" the upsert.
- `merge_books` moves readings and leaves exactly one open.
- Reread: finish, reopen, two readings, `Book.current_page` follows the new one.
- Every `KoStatus` maps as specified; `Other` leaves ours untouched.
- `attribute_highlights` splits by window and leaves out-of-window highlights
  `NULL`.
- Property where one exists — the back-fill and the reread ordering both have
  general rules, so prefer a property over more examples.

Then `make golden` and **review the diff: it should carry no semantic change.**
The three device keys were already in the report; only persistence is new.
Anything else in that diff is a bug.

## Constraints

- Engine + CLI + the two TUI call sites. No new screen, no new TUI feature.
- No network in tests, ever. `sqlite::memory:` throughout.
- Typed `Diagnostic`s, never pre-formatted strings.
- `EngineError::Other` is last-resort; if a caller might branch on it, add a
  variant.
- Never edit an applied migration. `0005` is yours; do not take `0006`/`0007`.
- Highlight text, note bodies and search queries never go above `trace!`.

## Done when

`make ci` green; a copy of a real `database/app.db` migrates with its progress
intact in `readings`; `rb progress`, `rb progress --reread`,
`rb list --sort progress`, `rb show` and the TUI's finished-toggle all work;
goldens regenerated and their diff reviewed. Run the `cargo-tester` agent before
committing.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
