# Prompt — Items 43 and 41: readings across the library, and the read number that crosses

Engine + API only. Runs in a worktree, in parallel with the item 45/46 thread —
**touch nothing under `gui/` except the regenerated `bindings.ts`**, and touch
nothing under `crates/engine/src/storage/flashcards.rs` or `notes.rs`.

## Before you write a line

```
git log --oneline -1                    # must be the tip of main
ls crates/engine/migrations/ | tail -2  # must end at 0017_moments.sql
```

If either is wrong, `git reset --hard main`. Four of six worktrees last wave were
cut ~80 commits behind and one nearly wrote a migration into a five-version gap.

Read the root `CLAUDE.md`, `crates/engine/src/storage/CLAUDE.md`,
`crates/engine/migrations/CLAUDE.md`, `crates/api/CLAUDE.md`, and
`docs/decisions.md` entries **18** (list endpoints — this item is its shape
applied to a second table), **35** (sort-key indexes, and how an index is
proved), **44** (the card's passage — it names this item by number) and **17**
(derived facts live in the engine; `ReadNumbering` is item 17c).

**Migration `0018` is pre-allocated to this thread.** Nothing else in this wave
takes one. Indexes only — judged on `EXPLAIN QUERY PLAN` before and after, which
is the only claim an index migration can make.

## What

### Item 43 — readings across the library

`Storage` has twelve public reading methods and every one is scoped to a single
`book_id`/`id`, except `list_open_readings`, which is library-wide and filtered
to `finished_at IS NULL` — so a finished reading is reachable *only* by already
knowing its book. `ActivitySummary::books_finished` counts exactly the rows
nobody can list.

Mint the list surface, in item 18's shape and not a new one:

- a filter struct with **one** `predicate()` shared verbatim by the list and the
  count, `Default` meaning every row;
- **offset paging**, and an `ORDER BY` that is a **total order** — every arm
  ends in `readings.id`. Item 18 learned that a behavioural partition test stays
  green when the tie-break is deleted, so the guard reads the SQL;
- **the count is its own request**, not a field beside the rows. A wall asks it
  once and pages many times.

The **year filter** is the point of the item (`gui-vision.md:151`), so it must
compose with the count like every other filter.

### Item 41 — the read number crosses

`ReadNumbering` (`storage/readings.rs`) answers two questions — *which 1-based
read is this* and *is there more than one read to tell apart* — and stops at the
TUI. Item 28 declined to fake it with `readings.indexOf(id) + 1` and used dates
instead. Cross it, so a card can say **your second read** without a frontend
re-deriving a rule that silently depends on `list_readings`' oldest-first
ordering.

## Three decisions this item has to make, with the trap in each

State your answer and the argument in `docs/decisions.md`. **Push back rather
than comply** if the reasoning below is wrong — four of five threads did last
wave and every one of them was right.

1. **Does the new list carry the card's passage?** Item 44 says it must:
   *"when [item 43] arrives the passage belongs on whatever list it mints rather
   than in N of these"*. But `CardPassage`'s own doc refuses to put a passage on
   `ReadingDto`, because that puts the reader's private highlight text on every
   row of every `ListReadings` including the ones nobody is drawing a card for.
   Both are right, which means the passage belongs to **this list's** row type
   and not to `ReadingDto`. Decide the shape — a distinct row DTO, or an opt-in
   flag on the request — and say which and why. Whatever you choose, one call
   must answer a page of the wall; N `CardPassage` calls is the pathology item
   18 exists to remove.

2. **Where does the read number sit, and what does `None` mean there?**
   `ReadNumbering::number_of` returns `None` for *three* different situations,
   and one of them is **the book has been read once** — a deliberate rule, since
   a column that always reads `1` is not worth a column. Put that `None` on
   `ReadingDto` and it acquires a fourth meaning: *the caller who built this DTO
   did not have the sibling readings to hand*. `GetReading` and `ActiveReading`
   fetch one row and have no siblings; `ReadingDto::new(reading, progress)` is
   the only constructor and it cannot know. A field that means "single read" on
   one request and "not computed" on another is worse than no field. Either put
   the ordinal only where it is honest, or make every path that mints a
   `ReadingDto` able to answer it. Do not ship the ambiguity.

3. **The year filter must not defeat the index you are about to add.**
   `activity_summary` filters with `date(finished_at, 'unixepoch') BETWEEN ? AND ?`
   — an expression over the column, which no index on `finished_at` can serve.
   Convert the year or day range to raw unix-second bounds in Rust and compare
   the bare column, or migration `0018` buys nothing. `0016`'s comment records
   the sibling lesson: SQLite silently ignores an index whose collation differs
   from the comparison's, and it will just as silently ignore one whose column
   is wrapped in a function.

## Must not

- **Touch `crates/engine/src/storage/flashcards.rs` or `notes.rs`** — the other
  thread owns both.
- Change `list_readings`' oldest-first ordering. `ReadNumbering`'s doc comment
  records that the ordering is a silent contract; the CLI's `reading 2/3`, the
  TUI's gutter and this item's numbers all depend on it agreeing.
- Move `API_VERSION`. Everything here is additive — new requests and
  `#[serde(default)]` fields. If you believe something must be removed, stop and
  say so rather than bumping it.
- **Change the shape of an existing `Request` variant if a new one will do.**
  `ts-rs` emits a new field as **required** in TypeScript however
  `#[serde(default)]` the Rust is, so a field added to an existing request
  breaks `gui/src/lib/api/client.ts` — and your own gate cannot see it, because
  a fresh worktree has no `gui/node_modules`. If you must change one, **say so
  loudly in your report**.
- Add a `ReadingState` variant for a filter case. Item 18 settled that: a filter
  case is a question somebody asked once; a variant is a thing a UI eventually
  puts a badge beside, which is the framing `docs/decisions.md` bans.

## Done when

- **The index is asserted against `EXPLAIN QUERY PLAN`**, in the shape of
  `the_sort_key_indexes_are_the_plan_the_planner_picks`
  (`crates/engine/src/storage/books.rs:3255`): the plan names the index *and*
  no longer says `USE TEMP B-TREE FOR ORDER BY`. A behavioural test cannot see
  an index — it returned the right rows the day before the migration too.
- The **total order** has a guard that reads the SQL, not only a partition test.
  Item 18 and item 35 both had to learn this the same way.
- The paging behaves under a filter: a page and its successor partition the
  filtered list, and the count agrees with the number of rows a full page-walk
  yields.
- The read number agrees with what the TUI's gutter shows for the same book —
  that agreement is the whole reason the rule left the frontend.
- A payload written before this wave still parses into every request you touched.
- `make ts` run and `bindings.ts` committed.

## Files

`crates/engine/migrations/0018_*.sql`, `crates/engine/src/storage/readings.rs`,
`crates/engine/src/storage/query.rs`, `crates/engine/src/lib.rs`,
`crates/api/src/{dto,protocol,lib}.rs`, any CLI/TUI call sites the signatures
break, and `gui/src/lib/api/bindings.ts` (generated only).

## How it is gated

**`make fmt lint build-check test ts-check`** — not `make ci`. A worktree has no
`gui/node_modules`, so `web-check` and `routes` print `SKIPPED:` and would pass
unrun. The orchestrator runs the full `make ci` from the main checkout after the
merge.

Run the `cargo-tester` agent before you call it done.

## `docs/decisions.md`

**Append** two entries, 43 and 41, in build order. Restructure nothing: the file
is in build order rather than numeric order and it is the guaranteed conflict of
every merge. Each entry records **the corrections building it forced** — that
paragraph is the most valuable thing the item produces and it is the one that
gets skipped when the tests go green.

## Report back

What you overturned, what the three decisions above resolved to, and whether you
changed the shape of any existing `Request`.
