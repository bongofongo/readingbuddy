# 2026-08-07 — the search, the years, and the word that hid in an empty state

Items **50**, **51** and **52**, built sequentially in one session (no worktree
workers) from `docs/handoff-orchestrator-gui-50-plus.md`. Base `480ebe9`.

## Decisions locked

- **Item 50's search box is above both bands, not beside the Notes heading**
  where `gui/CLAUDE.md` specified it. `SearchMarks` returns one ranked list over
  *both* indexes, and the two things it ranks are drawn by two different bands —
  a box inside one answering about the other is a heading that lies. Scoping it
  to `source: 'note'` to make the heading true would have thrown away the
  passages.
- **One list, never two sections.** Two `source`-scoped calls spend one `limit`
  across two lists whose lengths then vary with the query; one call re-grouped
  above the seam invents the ranking the method exists to make unnecessary.
- **`ReadingYearsDto` carries `years` *and* `open`.** The API audit found that a
  bare `Vec<i32>` cannot partition a wall that deliberately holds open readings
  — a reader picking every year in turn would never see the book they are in.
  `open` is a **bool**: the picker needs to know the chip exists, and a figure on
  a control is the badge the axiom bans. No per-year counts either.
- **The scope picker is three cases (`all` / `year` / `open`), not a nullable
  year.** *Still reading* is not a year and cannot be spelled as one.
- **Item 52's guard is a source scan, not a route assertion.** Adding `yet` to
  `the library surface greets you with no numbers` would have caught **none** of
  the eight strings: six live in empty states the full fixture never renders.
- **No prompt files for 50–52.** They were minted from a handoff rather than a
  spec row — item 33's shape, which `new-wave-item` already documents.

## Bugs found

- **A stale prohibition in `gui/CLAUDE.md`**, present tense, asserting
  `SearchMarks` has no `book_id` — false since item 40, and it had survived a
  whole wave in the file every GUI thread is told to read in full. Item 50's real
  subject. Rule: *an item that lands corrects the text that said it could not.*
- **`wall.ts`'s inherited current-year clamp becomes a live bug the moment the
  years get better.** `yearRange` ends the current year at today, which is right
  on `/life`; with years from `readings.finished_at`, a future `finished_at` (a
  Goodreads `Date Read`, a device clock) makes the span `2027-01-01 … today` —
  inverted, refused by `DayRange`, wall replaced by an error. Unreachable under
  the proxy, which is a log of what has happened. Clamp deleted.
- **A note hit printed itself twice** — the marked snippet, then the same title
  dim underneath — because `snippet(…, -1, …)` returns whichever column matched
  and `notes_fts` indexes the title. The wire cannot say which column matched, so
  the fix compares text.
- **`WallControls`' labelling argument had expired.** It labelled only the Order
  group on the written grounds that a row of years is self-evidently a filter.
  Item 51 appended a text chip to that group and the argument became false with
  nothing failing — reproducing the two-lit-brass-chips ambiguity a previous
  review had fixed. Both groups are labelled now (`Show` / `Order`).
- **Placeholder contrast 2.35:1** — WebKit's default `#a9a9a9`; nothing in the
  repo had ever set `::placeholder`. It was also the only statement of what the
  search searched, and it clipped mid-word at phone width.

## Technical gotchas

- **`noUncheckedIndexedAccess` does not carry a truthiness check across two
  reads of `arr[i]`** when `i` is a mutable loop variable. Bind first.
- **A discriminant check on `hits[0].kind` does not narrow a second `hits[0]`.**
  The test passed at runtime by short-circuit and was untypechecked.
- **`gui/` ships no `@types/node`**, so a test cannot `import 'node:fs'`. Vite's
  `import.meta.glob('/src/**/*.svelte', { query: '?raw', eager: true })` reads
  files through the same resolution the app builds with, and needs no dependency.
- **`cargo fmt` is the leg that fails last and costs a whole re-run.** Every one
  of the five diffs was line-width rewrapping in code this session added. Same
  for `prettier` under `gui/` — `web-check` fails on formatting after everything
  slow has run.
- **The workspace test suite takes ~28 minutes here.** Start `cargo-tester` and
  write documentation while it runs. It also went quiet after finishing —
  `SendMessage` to it produced the report (the handoff's known failure mode).
- **`EXPLAIN QUERY PLAN` confirms `idx_readings_finished_at` covers the year
  grouping** only with `FROM readings` alone; adding `count_readings`' `JOIN
  books` scans `books` and searches `readings` per book. The plan test asserts
  `COVERING INDEX` and uses the joined spelling as the control.

## Verification

- `make fmt lint build-check test`: clippy clean, `cargo check --workspace`
  clean, **1182 passed / 0 failed / 11 ignored**.
- vitest **242 passed** (17 files); svelte-check 0 errors; eslint clean.
- Playwright **108 routes** on WebKit, three viewports (102 before).
- The axiom guard was verified by planting a violating `.svelte` and watching it
  go red, then removing it.
- `screenshot-reviewer` read the PNGs and found four real problems; three fixed
  in-session, one declined and recorded.

## Deferred (all in the new handoff)

- No dark-theme screenshots anywhere in the suite — two of the three required
  theme states render nowhere.
- `Card.svelte`'s title link has no affordance at rest; `/book/[id]/cards` draws
  the same destination in brass.
- `search_marks` has no tie-break after `ORDER BY rank` (blocks a *show more*).
- `SearchHitDto` does not say which field matched; the snippet's `>>`/`<<`
  markers are in-band and unescapable.
- `/book/[id]/cards` could take the year picker now and does not.
