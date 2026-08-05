---
title: Item 18 — list endpoints that survive a real library
date: 2026-08-05
source: docs/gui/spec-gui-17-28.md item 18; docs/decisions.md entry 17 for what
        item 17 handed this item and what it settled about sorting
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Prompt — Item 18: list endpoints that survive a real library

Paste into a fresh session at the repo root, on branch `feat/engine-list-endpoints`,
branched from `main` **after item 20 has merged** — see *Launch order* in
`docs/next-thread-handoff.md`. You and item 20 both edit
`crates/engine/src/storage/books.rs`, and a clean textual rebase there is not a
clean rebase.

Read `CLAUDE.md` (**Engine standards** is binding), then
`crates/engine/src/storage/CLAUDE.md`, then item 18 in
`docs/gui/spec-gui-17-28.md`, then **`docs/decisions.md` entry 17** — that one
settles half of your hardest question before you start.

**Engine and API. No migration**, and that constraint is doing work: an FTS index
over highlights would take one, and it must be *split out as its own item* rather
than smuggled in here. `0014` belongs to item 20 and `0015` to item 23.

## What the item is

`ListBooks{limit, sort}` is the whole of the list surface. No offset, no cursor,
no filter, and nothing anywhere returns a count. The TUI copes by fetching 200
rows and narrowing in Rust; a shelf cannot, and `list_notes` has **no limit at
all** — a full table scan into a `Vec` on a screen that shows twelve rows.

## What item 17 changed under you, and it is the crux

**`BookSort`'s contract is now written down, and it is the opposite of the TUI's
policy.** Both are correct; the bug is mistaking one for the other.

- `Storage::list_books(limit, sort)` returns **the first `limit` books *by that
  key***. `limit` selects along the sort key, in every arm alike. That is what
  `LIMIT` means and it is what a paginated shelf needs.
- The TUI's `Sort` (`crates/tui/src/ui/library.rs`) fetches one page **by
  recency** and reorders *that page* in Rust, so pressing `s` reorders the list
  rather than swapping its contents. That is a decision about a fixed page, it
  has a reason written at the call site, and it stays in the TUI.

The spec's own warning — *"a limit applied in SQL makes the sort key decide
membership, not just order"* — is therefore not an open question any more. It is
answered: **membership follows the key, deliberately.** Your job is to make
pagination honour that rather than to re-litigate it. Say in the design where the
TUI's policy would break if it ever adopted your cursor, because eventually
somebody will try.

**`BookSort` gained two variants and one of them is a landmine for keyset
pagination:**

- `BookSort::Year` is `books.publish_year DESC NULLS LAST`. Ordinary.
- **`BookSort::Author` is not an `ORDER BY` at all.** "Alphabetically by last
  name" is a parse of a human name (`readingbuddy::names`) and SQLite has nothing
  to parse one with, so `list_books_by_author` reads the whole table, sorts in
  Rust with `names::sort_key`, and truncates. It has **no cursor key that exists
  in the database.**

So you have three shapes of sort, not two, and the spec only anticipated two:

| sort | orderable in SQL | stable cursor key |
|---|---|---|
| `LastModified`, `Title`, `Year` | yes | yes |
| `Progress` | yes — but a computed ratio across a `LEFT JOIN` | **no** |
| `Author` | **no** | **no** |

Decide this explicitly and defend it. The options are not equal:

- **Offset for the two that cannot keyset, keyset for the rest.** Honest, and the
  API then has two pagination shapes, which is a cost paid at every call site.
- **Offset everywhere.** One shape; the deep-page cost is real but a personal
  library is not a feed.
- **Materialise the ratio, and add `sort_author`.** Both are **columns**, so both
  are a migration, so **neither is this item's** — but say so if that is the
  right answer, because it is a real finding and the next item to take a
  migration should carry it. Item 17 explicitly left `sort_author` open for
  exactly this.

## What you inherited from item 17

**A per-row summary of what is *behind* a book** — whether it has highlights,
notes, a file — reassigned here because it is a query shape and not a derivation.
The detail screen makes **four calls for one book** (`get_book`, `list_readings`,
`list_highlights`, `list_notes`); for a list that is eight hundred, so no list can
show it, and the GUI recorded the fact rather than working around it
(`gui/src/routes/book/[id]/+page.svelte`).

Two things about it:

- **The axiom line it sits against.** A count of *your own highlights* is past
  tense and allowed. A count of what you have not done is not, ever. Nothing here
  produces "12 books unread" — `docs/decisions.md` bans that framing by name and
  `gui/tests/routes.spec.ts` asserts against the words on a rendered page.
- Whether it is a count or a boolean is yours to decide. A tile that says
  *3 highlights* and a tile that shows a mark are different products; the
  cheaper query is the boolean and the more useful answer is probably the count.
  Pick one and say why.

## The rest of it

- **Counts.** `ListBooks` cannot answer "how many books" without returning all of
  them. A shelf needs the number before it needs the rows. This is a `COUNT(*)`
  with the same `WHERE` as the page — which is the argument for building filters
  and counts together rather than in two passes.
- **Filters.** Status (reading / finished / put down / no reading), author, year,
  language, tag, has-cover. Every one is a `WHERE` the TUI currently cannot ask
  for. Note the first one is now typed: `ReadingState`, with an `Other(raw)` arm,
  and **"no reading" is absence rather than a variant** — so a status filter has
  four cases and one of them is `cur.status IS NULL`. Do not add a `NeverOpened`
  to make the filter tidier; that is the banned framing wearing a type, and
  `no_reading_is_absence_rather_than_a_variant` will fail.
- **`list_notes` gets a limit**, and `ListNotes{book_id?}` gets it on the wire.
- **`find_books_by_title` is a plain `LIKE`**, and **highlights and annotations
  are in no FTS index** — `notes_fts` is the only virtual table in the repo. A
  GUI with one search box that searches notes but not highlights will be reported
  as a bug, correctly. The index is a migration and therefore **not yours**:
  report it as an item with a number the user allocates, and say what the search
  surface would need from it.

## What must not happen

- **No migration.** If something here needs storage, that is a finding to report,
  not a file to write. Two candidates are already known (`sort_author`, an
  `highlights` FTS index) and naming them well is worth more than sneaking one in.
- **No count of what the user has not done.** No "unread", no "remaining", no
  goal, no streak.
- **Do not change the TUI's fetch-200-and-narrow policy** to use your pagination.
  It is a deliberate decision with a reason at the call site, and the TUI is not
  required to migrate — that is what independence means here.
- **Do not re-sort in a frontend to work around a cursor you found awkward.**
  That is the exact bug the membership rule exists to prevent.

## Files you own, and the one you share

Yours: `crates/engine/src/storage/notes.rs`, `crates/api/src/protocol.rs`,
`crates/api/src/lib.rs`, and the CLI/TUI call sites that need the new argument
shape. **Shared with item 20: `crates/engine/src/storage/books.rs`** — item 20
adds columns to `books` (and therefore rows to `MERGE_RULES`, `BOOK_COLUMNS` and
`row_to_book`) while you change `list_books`. Branch after it merges.

## Push back rather than comply

Four of five threads in the last wave did, and each time they were right. In
particular: if the pagination shape argued for above is wrong, say so and say
why. The membership rule is the one to argue with — but argue with it on the
merits, not by quietly building around it.

## Done means

- `make ci` exit 0 — fmt, clippy, build-check, **ts-check**, the whole-workspace
  test, **web-check** and **routes**.
- `make ts` run and `gui/src/lib/api/bindings.ts` committed in the same change as
  any DTO edit. CI fails on a stale copy.
- The `cargo-tester` agent before you call it done.
- **The corrections this build forced, written into `docs/decisions.md`** in the
  shape the existing entries use. Every landed item records what building it
  changed about the plan; ask and it arrives, do not ask and the next thread
  rediscovers it.
- A session log, via the `wrap-session` skill.
