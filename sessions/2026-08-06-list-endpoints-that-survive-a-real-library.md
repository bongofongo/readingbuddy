---
title: Item 18 — list endpoints that survive a real library, and the tie-break no test could fail
date: 2026-08-06
follows: sessions/2026-08-05-the-shape-of-an-edition.md
---

# Session log

Item 18, built in the worktree `rb-wt/18-list` on `feat/engine-list-endpoints`,
branched from `main` at `a6f4720` — a main already carrying items 20, 22, 24 and
19. Nothing was moving underneath, so no rebase. Not pushed, not merged, no PR;
integration is the orchestrator's.

## What it is

`ListBooks{limit, sort}` was the whole list surface: no offset, no filter,
nothing anywhere returning a count, and `list_notes` with no limit at all — a
full table scan into a `Vec` for a screen showing twelve rows. It now has all
four, plus the per-row summary item 17 handed over, plus item 19's
`EditionShape` on the wire.

New: `crates/engine/src/storage/query.rs` (`BookQuery`, `BookFilter`,
`StatusFilter`, `BookSummary`), `Storage::count_books`,
`Storage::book_summaries`, `Request::{CountBooks, BookSummaries}`,
`Response::{Count, BookSummaries}`, `BookDto.shape`.

## Decisions locked

- **Offset pagination, in every arm, and the spec's own alternative was the
  wrong shape.** The spec offered keyset for the sorts that can and offset for
  `Progress`, which cannot. Item 17 then added `Author`, which has no `ORDER BY`
  at all — so two of five sorts have no cursor key that exists in the database,
  and they are exactly the two whose pages are already whole-table reads.
  `Author` costs the same on page 40 as on page 1 whatever the cursor is. Paying
  a second pagination shape at every call site to speed up the three sorts that
  were already cheap is paying in the wrong currency. Three more: a count
  composes with an offset and not with a cursor, and a shelf that knows its total
  wants page numbers, which *are* offsets; the deep-page cost is a sort over a
  personal library, not a feed; and **there is no index on any sort key**, so
  `ORDER BY title` sorts the whole table however it is paginated — which makes
  the index the real optimisation and the cursor a distraction.

- **Offset paging needs a total order, and the test for it cannot fail.** This is
  the finding of the build. `LIMIT 20 OFFSET 20` is only the successor of
  `LIMIT 20` if both statements break ties the same way; `publish_year DESC` over
  four hundred books sharing a year does not say. So every arm ends in
  `books.id`. Then the honest part: **deleting those tie-breaks leaves
  `a_page_and_its_successor_partition_the_list` green.** Measured, twice, on a
  fixture built entirely out of ties — SQLite's sorter is deterministic for one
  plan over one set of rows. That determinism is a property of the current query
  plan, not of the schema, and the day an index over `title` or a different
  `LIMIT` sends the planner elsewhere the failure is a book on two pages and
  another on none, intermittently, with nothing on screen looking wrong. The
  guard is therefore `order_by_is_a_total_order`, which reads the SQL. A
  behavioural test that cannot fail is not the one holding a line, and the doc
  comments now say which test holds which.

- **Where the TUI's fetch-200-and-narrow policy would break if it adopted this.**
  It is untouched and stays untouched. It breaks the moment there are *two*
  pages: a Rust-side re-sort is only sound over a single page, because page 2
  fetched by recency and then title-sorted in Rust does not concatenate with page
  1 into a title-sorted list. Fixing that means moving the sort into SQL, and
  moving it into SQL is precisely the membership change the 200-row fetch exists
  to avoid. The two are compatible only while the TUI shows one page — which it
  does. A TUI that wants a second page has to choose.

- **Filters and counts share one clause; they do not share a round trip.** One
  `BookFilter::predicate` writes the `WHERE` and both `list_books` and
  `count_books` call it, asserted across ten filters and five sorts. But the
  count is its own request: it is a property of the *filter*, a shelf asks it
  once and pages many times, and bundling it beside the rows would make every
  scroll pay for a scan of the whole matching set. The spec's "build filters and
  counts together" is about the implementation, and that is where it was taken.

- **Four status cases, three reading states.** "No reading" is `cur.id IS NULL` —
  the join's absence, deliberately not `cur.status IS NULL`, since a reading with
  a null status is a book that has been opened. It sits on `StatusFilter`, in the
  question, not as a `ReadingState::NeverOpened`.

- **The per-row summary is counts, and the boolean's cost argument does not
  survive the implementation.** `book_summaries` is three grouped aggregates over
  three existing `book_id` indexes for a whole page — the same three queries
  whether they end in `COUNT(*)` or `EXISTS`, because the cheaper alternative (a
  correlated subquery per row) is not how it is built. So the count is free and
  carries more; the mark is `> 0`, spelled once as `BookSummary::has`. Every
  number is past tense.

- **`list_notes` keeps its unlimited form.** A default cap is the obvious fix and
  the wrong one: `resolve_note` walks every title in the vault so `rb links
  "Reflection: Pachinko"` works, and a cap there would silently stop resolving.
  A truncated correctness pass is worse than a slow one.

- **`EditionShapeDto` landed here rather than as its own item.** Item 19 named
  the gap and blocked item 26 on it. The field is derived and read-only beside
  `progress` and `series_label`, and item 20's stored dimensions make it a
  division of two columns rather than an image decode — the rewire item 19 said
  would need no signature change, and did not.

- **The wire grew and `API_VERSION` did not move.** `ListBooks` carries `offset`
  and `filter` flat with serde defaults, so `{"limit":20,"sort":"title"}` still
  means what it always did. `BookQueryDto` is the typed method's one argument,
  because `limit` and `offset` are both `i64` and adjacent.

## Bugs found by the signature change

Three caps that were limits standing in for their own absence:

- **`koreader::scores_for` read `list_books(10_000, …)`** — the query that
  decides whether a sidecar is a book we already have. A library past ten
  thousand would have started minting duplicates with nothing reporting it.
- **`goodreads::export` read `i64::MAX`.** Same shape, no consequence yet.
- **`rb book list` accepted three of the engine's five sorts.** `author` and
  `year` landed with item 17 and never reached a user, because the CLI's `match`
  was the only door and nobody widened it.

## Reported, not built

Each of these is an item somebody has to number.

- **A `highlights` FTS index.** `notes_fts` is still the only virtual table in
  the repo, so a GUI with one search box that finds notes and not highlights will
  be reported as a bug. Needs a migration, a trigger trio or an explicit writer
  beside `insert_highlight`, and a `search_highlights` returning the `snippet()`
  shape `NoteSearchHit` already has. The *surface* wants one request answering
  both, since two lists a frontend interleaves is a relevance ordering invented
  above the seam. `find_books_by_title` is still a plain `LIKE` and belongs in
  the same item — as a `title` predicate on `BookFilter`, so search arrives as a
  filter rather than a seventh endpoint.
- **Indexes on the sort keys** (`books.last_modified`, `books.title COLLATE
  NOCASE`, `books.publish_year`). The thing that would actually make a deep page
  cheap, and the thing that makes the tie-break load-bearing rather than
  insurance.
- **`sort_author`.** Still open, still argued against on item 20's grounds. It
  only pays as part of the index item, where the back-fill and the index arrive
  together.
- **`MatchCandidateDto` has no author**, though `koreader::band` holds the whole
  `Book` — so "which Dune is this" costs an N+1 `get_book` per candidate. Same
  class as the per-row summary this item built, in a file it did not own.

## Verification

`make fmt lint build-check test ts-check` from the worktree, plus `cargo-tester`.
`make web-check` and `make routes` **cannot run here** — a worktree has no
`gui/node_modules` and the Makefile degrades them to a silent `SKIPPED:`. The
frontend edits are three: `gui/src/lib/api/client.ts` passes the two new
`list_books` params and the new `list_notes` one at their do-nothing values,
`gui/src/lib/api/fake.ts` gains a `shape` on its book fixture, and
`gui/src/lib/api/bindings.ts` is regenerated. **Whoever integrates this must run
`make web-check` and `make routes` before merging.**
