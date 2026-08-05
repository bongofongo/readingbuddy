---
title: Items 29–32 (plus 21) — what the engine keeps, built by five parallel workers
date: 2026-08-05
---

# Session log

Started as a question — *what else could the engine pull about a book?* — and
became a five-item wave, specced and built in one sitting by headless workers in
git worktrees driven from tmux panes. Main went `e980cd4` → `99ba512`: 64 files,
+9,854/−405, three migrations, four new engine modules, engine lib tests 287 →
320.

## Decisions locked

- **The wave is about what the engine *keeps*, not what it acquires.** Every
  importer wrote a book once and never looked again: no re-ask path, no record of
  which source supplied which field or when, no reading-time data, and three
  fields a shelf needs that nothing captured.
- **Item 21 (`reading_events`) was pulled forward** out of the 17–28 GUI wave,
  because item 31 needed somewhere to put reading time and the settled answer is
  that no source is consumed in its own shape. Its `statistics.sqlite3` filler
  was deferred to item 15 — **that deferral was about the plugin**, and reading a
  file off an already-mounted volume is what `scan_device` does today.
- **Provenance before enrich**, reversing the first plan. Without it, enrich's
  first run silently overwrites hand-corrected fields, because `MERGE_RULES`'s
  `COALESCE` cannot tell "the user typed this" from "a provider guessed it".
  Vindicated within hours: item 29's guard fired on item 30's *first* hand-run
  against live OpenLibrary.
- **No fourth provider this wave.** A new source before per-field provenance
  exists is more disagreement with no way to attribute it.
- **Migration numbers follow merge order, not pre-allocation.** 0011→21,
  0012→29, 0013→32; GUI items 20 and 23 moved to 0014/0015. Both specs and the
  `new-wave-item` skill updated.

## Bugs found

- **Pre-existing: `download_cover` created a second book row instead of a
  cover.** It wrote back through `upsert_book`, whose no-ISBN branch is an
  unconditional insert that ignores `Book::id` — so every sidecar-seeded book,
  the exact case item 30 exists for, got a duplicate. Reachable from
  `Engine::fetch_cover` before this wave.
- **Pre-existing: `merge_books` was a second hand-written copy of
  `MERGE_RULES`** — sixteen columns with `dst`-wins semantics, in the one
  statement where a forgotten column *loses data*. Now generated.
- **Pre-existing: `import_epub` folded two origins into one row** with a
  hand-written `is_none()` per column — `MERGE_RULES`'s merge spelled a second
  time. Two writes now, through `fill_book`.
- **Pre-existing: `ko pull`'s closing hint told users to hand-assemble item 30.**
  It names `readingbuddy enrich <id>` now.
- **Main was briefly red between the 29 and 31 merges.** Item 29 made
  `upsert_book` take `Option<Source>`; item 21's test seeds passed one argument.
  Each branch was green alone and the *library* still compiled —
  `cargo check --workspace` does not resolve dev-dependencies, so only
  `cargo test` sees it. Found by item 30, fixed twice independently.

## Technical gotchas

- **KOReader's `statistics.sqlite3` is WAL.** Copying only the main file reads
  the last checkpoint and can silently miss an entire recent session, with a
  plausible number where the right one belongs. Copy the `-wal` and `-shm`
  siblings. The test must hold a connection open across the import — a clean
  close deletes the WAL and proves nothing.
- **"`stats.md5` does not exist" is about the *sidecar's* `stats` subtable.** The
  statistics database carries `book.md5 = util.partialMD5(file)`, so that join is
  exact. `docs/decisions.md` read like a blanket claim and would have sent the
  next reader to build a title matcher.
- **`page_stat` is a VIEW that rescales pages onto the current page count** with
  integer division and multiplied rows. Read the raw table.
- **A `reading_events` key containing `reading_id` is unrepresentable.** It is
  nullable `ON DELETE SET NULL`, so the key needs a NULL sentinel — and then
  deleting a reading with two events on one day collides, surfacing as a
  constraint error from a table nobody touched. Key is `(book_id, day, source)`;
  the read is an *attribution*.
- **Per-field provenance cannot protect a field *pair*.** A user-owned `isbn_13`
  held while the unowned `isbn_10` landed from a different edition. Item 32's
  fix: guard the pair off either claim — strictly weaker than "a claim with no
  value" and sufficient, because the incoherence only ever enters through the
  unowned half. The guard must live in **two** places from one rule (merge clause
  *and* stamp), because a column held by its partner's claim has no `user` row of
  its own for `stamp`'s `WHERE` to catch.
- **Subject sets cannot merge by union.** `field_provenance` holds one source per
  field, so a unioned value would have to name one of two origins for a value
  that came from both. Replacement is the only attributable merge.
- **`search::merge_into` is an ungenerated fourth consumer of `MERGE_RULES`.** A
  new column compiles cleanly and is silently lost in federated search. Only
  `every_claimed_field_is_a_merge_column` catches it — and it did.
- **OpenLibrary's `search.json` 503s from some IPs while `/isbn/` works.** Worth
  knowing before reading a hand-run as a code failure.
- **`claude -p`'s shell exit code is the pipeline's, not the session's.** A
  transport drop reports `subtype=success` with `is_error=true` — a dead worker
  looks exactly like a finished one. Both runner scripts now read `is_error`.

## Verification

- `make ci` run directly on **every branch after rebase** and on main after every
  merge — never on a worker's own report. Final main: exit 0, engine lib 320
  passed, TUI 291, `cargo deny check bans licenses sources` ok.
- Item 30 hand-run end to end against a real sidecar: `ko pull` → ISBN-less book
  → `rb set --isbn` → `rb enrich` filled publisher, year, language, isbn-10,
  pages, cover, and **held back the user's isbn_13** against OpenLibrary's
  redirect to a different edition.
- No test in the wave touches the network.

## Deferred

- **Disagreement history** `(book_id, field, source, value, fetched_at)` —
  declined independently by items 29 *and* 30. Re-asking makes the value half of
  the case; the report carries it in-band and nothing yet compares across runs.
- **`search::merge_into` generation** — observed with evidence, not fixed.
- **KOReader statistics schema unverified against hardware.**
  `KNOWN_SCHEMA_VERSION` gates it, so an unknown version imports nothing and says
  why.
- **The day skew** between the highlight filler (zoneless local wall clock read
  as UTC) and the statistics filler (real epoch). Correcting by this machine's
  offset would make an import depend on where the laptop is.
- **No CLI/TUI/API surface** for `reading_events`, its aggregates, subjects,
  series, the TOC, or `import_device_statistics`. `rb enrich`/`rb set` are the
  wave's only frontend.
- **`rb set` flags for subjects/series** — `Engine::set_book_fields` already
  carries them; three clap arguments are a CLI item.

## Orchestration notes (what to repeat, what not to)

- **Worktree + APFS-cloned `target/` per worker** (`cp -Rc`, ~90s, ~1 GB
  divergence) — parallel builds with no cold start and no lock contention.
- **Commit a prompt edit *before* creating the worktree from main.** Done
  backwards once; the worker launched against a stale brief and was restarted.
- **Feed each landed item's corrections into the next item's brief.** Item 31
  inheriting item 21's merge semantics is what stopped it writing a
  delete-then-insert that would have wiped the highlight filler's days.
- **Parallel worktrees produce semantic conflicts git cannot see.** A clean
  textual rebase compiled to a signature mismatch. Verify after rebase, always.
- Reading pane scrollback with `grep` misdiagnosed a failure twice; run the
  check directly when the answer matters.
