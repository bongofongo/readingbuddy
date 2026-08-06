---
title: Handoff — the engine half is finished; the GUI half is what is left
date: 2026-08-06
source: sessions/2026-08-05-covers-a-grid-can-use.md (20),
        sessions/2026-08-05-the-shape-of-an-edition.md (19),
        sessions/2026-08-05-reading-here-the-local-source.md (22),
        sessions/2026-08-05-vault-coherence.md (24),
        sessions/2026-08-06-list-endpoints-that-survive-a-real-library.md (18);
        docs/decisions.md entries 18–24 for the rulings
supersedes: the 2026-08-05 handoff — all five of its prompts have landed
---

# Handoff

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where things stand

**`make ci` exit 0 on `main` at `da589fc`.** Working tree clean, committed
straight to `main` (nothing is pushed; this repo has no remote it uses). The
gate is the full one: fmt, lint, build-check, ts-check, whole-workspace test,
web-check, and 30 Playwright routes on WebKit.

**Items 18, 19, 20, 22 and 24 are all merged** — the rest of the engine half.
Migration `0014` is applied (item 20's cover metrics). `0015` still belongs to
item 23.

Each item's own corrections are in `docs/decisions.md`, which is in **build
order and not numeric order** — the wave appended 20, 22, 24, 19, 18 in that
sequence. Read those five entries before touching any of it; several are
load-bearing and three of them overturned what the spec asked for.

## What the wave overturned, in one line each

The spec lost five arguments and was right to. These are the ones a later thread
is likeliest to re-open by accident:

- **`readings.source = 'local'` was not added** (item 22). That column names the
  *writer of the row*, and a typed page's writer is `update_progress`, which
  already writes `manual` — so `local` there is a synonym, and
  `readings_from_source` (every importer's idempotency) would have had to know
  both. The ownership row landed on `reading_events.source`, where the
  vocabulary is *claimants* rather than writers.
- **Attaching a file opens no reading** (item 22). The spec asked for one; it
  would mark five newly attached PDFs as five books you are currently reading.
- **Cover metrics are deliberately *not* `MERGE_RULES` rows** (item 20) — the
  opposite of what `0013` did, and the first time that has been right.
  `the_cover_metrics_sit_outside_the_merge_table_and_the_path_does_not` stops a
  later thread quietly adding a `Rule`, and stops it removing `cover_path`.
- **Pagination is offset everywhere, not the spec's hybrid** (item 18). The two
  sorts with no cursor key (`Progress`, `Author`) are exactly the two whose
  pages are already whole-table reads, so keyset buys nothing where it is needed
  and costs a second shape at every call site. A count composes with an offset
  and not with a cursor.
- **`sort_author` was refused twice**, by items 20 and 18, on the same ground:
  SQLite cannot compute the value, so the column is NULL for every existing row
  and `ORDER BY sort_author` is silently *wrong* until a back-fill nobody has
  run yet runs — which adds an arm beside the slow one instead of replacing it.

## Three traps this wave found, all still live

- **`sort_title` has never been computed by anything.** It is a `MERGE_RULES`
  column, `Federated::Local`, present on `Book` and on the DTO — and
  `BookSort::Title` orders by `books.title COLLATE NOCASE`. **A sort-key column
  added without a writer looks answered and is not.** Check any column you plan
  to lean on actually has a writer.
- **A behavioural test that cannot fail is not a guard.** Item 18's offset
  paging needs a total order, so every SQL arm ends in `books.id` — and removing
  those tie-breaks leaves the behavioural partition test **green**, measured.
  SQLite's sorter is deterministic for one plan over one set of rows; that
  determinism belongs to the query plan, not the schema. The guard is
  `order_by_is_a_total_order`, which reads the SQL.
- **`create_note` indexed the body it was handed, not the one it wrote** (found
  by item 24; fixed). They differ by a trim and a newline, so no note was ever
  byte-identical to its own index — the first comparison anybody wrote would
  have re-indexed the whole vault while looking correct.

## What is next

### The GUI half — items 26, 27, 28, and item 23

**These must run in sequence, never in parallel.** Three agents there produce
three dialects of one app. `docs/gui/claude-code-plan.md` item 7 argues it, and
the current library screen is a plain grid *on purpose* rather than a half-built
item 26 in the wrong dialect.

And the standing instruction, unchanged: **do not let an agent decide the
shelf's feel.** It can build the shelf, screenshot it and say what is on it.
Whether it reads as a place is the user's call.

**Item 26 is unblocked.** Item 19 landed `readingbuddy::EditionShape` and item
18 carried it across the seam as `BookDto.shape` (`EditionShapeDto`,
`ShapeSourceDto`), so a WebGL shelf reads the engine's proportions instead of
re-deriving the arithmetic in TypeScript. **Do not push a scene constant back
down into that derivation** — `HALF_HEIGHT` is the renderer's and the whole
point is that two frontends scale the same ratios differently.

**But run `make covers` — or rather, build its door first.** See finding 1
below: every book in `make dev-db` has a `cover_path` and a **NULL**
`cover_aspect`, so a shelf built today will conclude the column does not work.

### Open work, none of it allocated a number

The user allocates numbers; these are stated well enough to allocate.

1. **The cover back-fill has no door.** `Engine::measure_stored_covers` exists
   and nothing calls it. The whole fix is a `Covers` variant in
   `crates/cli/src/main.rs` plus one line in the `dev-db` target. Item 20 left
   `crates/cli` alone for merge cost, not doubt. **Do this before item 26.**
2. **A `highlights` FTS index.** `notes_fts` is still the only virtual table.
   Needs a migration, plus a trigger trio or an explicit writer beside
   `insert_highlight`, and a `search_highlights` returning the `snippet()` shape
   `NoteSearchHit` has. Item 18's framing is the good one: the *surface* wants
   **one** request answering notes and highlights together, because two lists a
   frontend interleaves is a relevance ordering invented above the seam — and
   `find_books_by_title` belongs in the same item, as a `title` predicate on
   `BookFilter`, so search arrives as a filter rather than a seventh endpoint.
   Item 27's search box is what needs it.
3. **Indexes on the sort keys** (`books.last_modified`,
   `books.title COLLATE NOCASE`, `books.publish_year`). There is no index on any
   of them today, so `ORDER BY title` sorts the whole table however you
   paginate. This is what makes a deep page cheap, and it is what turns item
   18's `books.id` tie-break from insurance into load-bearing.
4. **`sort_author`**, refused twice above — it only pays *inside* (3), where the
   back-fill and the index arrive together. Item 20 made the write side cheap:
   `invalidate_cover_metrics` is the pattern (a companion clause generated from
   another column's value expression, bound from Rust because SQL cannot derive
   it).
5. **`MatchCandidateDto` carries no author**, though `koreader::band` already
   holds the whole `Book` — so the "which Dune is this" chooser, which is the
   *first* screen a refusal sends you to, costs an N+1 `get_book` per candidate.
   Reported independently by items 22 and 18. Narrow, migration-free.
6. **The border-median accent arithmetic is duplicated** between
   `crates/engine/src/images.rs` and `crates/tui/src/render3d/texture.rs`. Not a
   drive-by: `images.rs` measures the original file and `texture.rs` measures the
   *scaled texture*, so they can legitimately differ, and the renderer is frozen.
   Deleting the renderer's copy is a decision about what it draws.
7. **A real PDF sidecar in the corpus.** `entry_to_highlight` requires a string
   `pos0` and KOReader stores a *table* there on PDF, so those entries are
   skipped **in silence** — reasoned, not observed, and it deserves a
   `Diagnostic` rather than silence. `docs/koreader-format.md` files PDF
   annotations under *unobserved* for this reason.
8. **`gen-devdb` and `gui/src/lib/api/fake.ts` can diverge** and nothing asserts
   they agree; the fake serves **no covers**, so cover layout still has no
   headless regression test. Pre-existing, and item 26 is what makes it bite.

## Running the next wave as multiple workers

The mechanics worked again — five items, five worktrees, `make ci` green after
every merge. What changed since the last handoff:

- **Reset the last worker's worktree onto the *finished* main rather than
  rebasing it afterwards.** Item 18 was the one with a real file collision
  (`storage/books.rs`, against item 20) and it merged with **zero conflicts**,
  because its base already contained all four siblings. Rebase-after is the
  thing that produces semantic conflicts git cannot see; not needing one is
  better than surviving one.
- **`docs/decisions.md` is the guaranteed conflict.** Three of the four merges
  conflicted there and nowhere else. Tell every worker to **append** its entry
  and restructure nothing; then the resolution is deleting three marker lines.
- **A worker cannot gate on `make ci`.** A fresh worktree has no
  `gui/node_modules` (gitignored) and `make web-check` / `make routes` degrade
  to a stated `SKIPPED:` — so a worker "passes" them without running them. Gate
  workers on `make fmt lint build-check test ts-check` plus `cargo-tester`, and
  run the full `make ci` **from the main checkout after each merge**. `ts-check`
  needs cargo and no node, so it is the cheap guard on a DTO change.
- **Never read a pipeline's exit code.** `make test | tail -25` reports *tail's*
  status; it was 0 over an unread log. Redirect to a file and read `$?`.
- **APFS-clone `target/debug` into each worktree** (`cp -Rc`, near-zero bytes)
  but **strip `incremental/` and set `CARGO_INCREMENTAL=0`** — it was 26G of the
  51G, and four diverging copies of it is the difference between a full disk and
  a comfortable one. Budget ~10 minutes for the clone; it is not instant.
- **A worker can die instantly on a session limit** and its notification carries
  a one-line transcript that looks like a result. The worktree and branch
  survive untouched, so a relaunch costs nothing but the wall clock — but check
  what actually landed before believing a short report.

## Everything below here is unchanged from the previous handoff

The GUI seam (`bindings.ts` is generated; `cover_path` is a whole path; the
asset protocol scope is set at runtime; `TauriClient` must never fall back to
the fake), the engine-internals rules (`MERGE_RULES` generates six things; a
`user` claim protects a field *pair*; absence is not zero, anywhere), the
fixture rules (`corpus` must never depend on `readingbuddy`; `notes_fts` has no
triggers; `reading_events` is not seeded), the two cloud-session constraints
(gutenberg.org is blocked by the sandbox proxy; Playwright needs
`pnpm exec playwright install webkit`), and the four agents and three skills.
`CLAUDE.md` and the per-crate files carry all of it, and this wave updated them
where it changed something.
