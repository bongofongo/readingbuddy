# Prompt — Item 8a: the currently-reading query

Paste into a fresh session at the repo root, on branch
`feat/engine-currently-reading`.

---

Read `docs/spec-08-10.md` (item 8a) and `docs/decisions.md` before starting.
`CLAUDE.md`'s **Engine standards** section is binding.

**Engine only. No migration. No TUI** — the home screen is thread 8b, and this
is what unblocks it.

## What to build

### `Storage::list_open_readings(limit) -> Vec<(Book, Reading)>`

Build it as a variant of the join that already exists. `BOOK_FROM`
(`crates/engine/src/storage/books.rs:74-79`) already resolves "the current
reading" through a `LEFT JOIN` whose subquery orders by
`(finished_at IS NULL) DESC, COALESCE(started_at, created_at) DESC`, and
`BOOK_COLUMNS` projects `current_page` / `finished` / `date_started` /
`date_finished` off it.

> **Corrected after this prompt was executed (PR #5).** It said to add
> `WHERE cur.finished_at IS NULL`. That is wrong and silently so — the join is a
> `LEFT JOIN`, so a book with **no reading at all** has every `cur` column NULL
> and satisfies the predicate, putting the entire library on the home screen.
> The filter is **`WHERE cur.id IS NOT NULL AND cur.finished_at IS NULL`**,
> found by writing the empty-library test first.

**Do not write a second join.** One that disagreed with `BOOK_COLUMNS` would put
`Book`'s four projected fields out of step with the screen displaying them, and
the drift would be invisible until someone noticed a progress bar showing the
wrong reading.

Order by most-recently-touched. A home screen is a place you return to, so the
book you last did something with is the one that should be at the top.

### `Engine::currently_reading(limit)`

On the facade. The TUI reaches into the public `engine.storage` field today, and
unpicking that habit is most of what item 14 is; a new feature should not add to
it.

### A `NoteRecord`-returning way to open a reflection

`open_reflection` returns `CreatedNote { id, title, file, links }`, but every
editor path in both frontends needs a `NoteRecord` — the CLI already patches
around it with a follow-up `storage.get_note(note.id)`
(`crates/cli/src/commands/reflect.rs:60-64`). Item 8b's entire action is "open
the reflection", so it hits this on the first screen.

Add a record-returning wrapper over the existing `open_anchored`, and a review
twin. **Leave `open_reflection`'s signature alone** — the CLI uses it, and
changing it is a diff in a file this thread has no other reason to touch.

## Tests

In `crates/engine/tests/` — and note there is now a shared harness at
`tests/common/mod.rs` (`engine()`, `seed_book`, `highlight`, `place`,
`rewrite_sidecar`, `skipped`). Use it; do not add a fourth copy of `engine()`.

- a book with an open reading appears; a finished one does not
- finishing a book removes it; a reread reopens it and it comes back
- ordering is most-recently-touched
- the projected fields agree with what `Book` reports for the same book — the
  assertion that stops a second join drifting
- the record-returning wrapper is the *same note* as `open_reflection`'s, on the
  second call as well as the first

There is a narrative suite at `crates/engine/tests/workflows.rs` for stories that
cross build items. If this query has a story that crosses one, it goes there;
otherwise leave it alone.

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, and the PR body says
what changed and what was deliberately left out. CI gates the whole workspace on
ubuntu now, so a break anywhere shows up on the PR.
