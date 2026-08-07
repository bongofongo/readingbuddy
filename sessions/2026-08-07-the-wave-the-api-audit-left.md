---
title: The wave the API audit left — four items, two threads
date: 2026-08-07
items: 45, 46, 43, 41 (merge order)
handoff: docs/next-thread-handoff.md, rewritten
---

# The wave the API audit left

Started from "what is the next step in the project?", again. The GUI wave had
closed leaving four numbers with no entry behind them — 41, 43, 45 and 46, all
allocated by an API audit that built four of seven. None of them is a screen.

Four items, **two** threads rather than four: 41 and 43 share `readings.rs` and
`ReadingDto`, and 45 and 46 share nothing but the API files, whose sections do
not overlap. Both threads ran in parallel off one base and merged one at a time
behind a full `make ci`.

## Decisions locked

- **Pair the items into threads by the files they share, not by their numbers.**
  41 wants to sit on whatever row type 43 mints, so building it separately would
  have meant designing the same DTO twice.
- **The workers do Rust only; the TypeScript seam stays in the main checkout.**
  A worktree has no `gui/node_modules`, so a worker gating on `web-check` is a
  worker passing a check that never ran.
- **Six merged worktrees from the previous wave are disposable** (188G, and the
  disk was at 19Gi). The user ran the removal; the classifier declines
  `git worktree remove` from the agent.

## Bugs found — both pre-existing, neither by the item that found them

- **`activity_summary` could never have used an index on `finished_at`, and had
  not since `0011`.** It asked `date(finished_at, 'unixepoch') BETWEEN ? AND ?`
  — an expression over the column — so `books_finished` has been scanning the
  whole `readings` table. Found because item 43's year filter was about to
  inherit the same clause and make `0018` buy nothing. `DayRange::unix_bounds`
  now converts once at the validated boundary and every reader compares the bare
  column.
- **`flashcards` has carried `book_id` and `highlight_id` since
  `0001_init.sql`, and `list_flashcards`' SQL never selected them**, so
  `FlashcardRow` had no fields for them and
  `FlashcardDto` could not carry them. The "missing DTO field" in the handoff was
  three layers deep, not one.
- **`Storage::insert_flashcard`'s only production caller in the whole repo was
  the KOReader import.** A card could be minted by an import and by nothing
  else; every other call site is a test fixture, one of them reaching through the
  `internals` feature.

## Technical gotchas

- **A git merge can splice two structurally similar test bodies together and
  align their shared tail.** Both threads appended a test ending in
  `.dispatch(Request::X { .. }).await` with the same three-line tail, so git
  aligned that tail and interleaved the two bodies around it. Deleting the
  conflict markers would have **compiled**, into two tests asserting something
  neither thread wrote. The resolution is to rebuild the file from each side's
  own append block (`git show <side>:<path>`, slice past the merge-base's line
  count) rather than editing in place. This is the "never read a piped report"
  rule in a third costume: the cheap resolution looks right and is not.
- **A subagent with no `SendMessage` tool stalls the agent that spawned it.**
  Thread A's `cargo-tester` reported its PASS to the orchestrator instead of to
  thread A, and thread A sat completed-but-unfinished until the result was
  relayed by hand. Budget for it: a worker's tester result may arrive at the
  wrong address.
- **`ROW_NUMBER() OVER (PARTITION BY book_id …)` is the wrong read number.** A
  window function is computed over the rows that survived the `WHERE`, so a page
  filtered to 2025 holds a book's second read without its first and calls it
  read 1 — on a card whose entire sentence is "your second read". Two correlated
  subqueries over the unfiltered table instead.
- **A comma list's terms take their own direction.** Appending ` DESC` to
  `COALESCE(started_at, created_at), id` reverses only `id` and leaves the
  leading term ascending — a clause that reads right, runs, and orders by
  neither thing the caller asked for.
- **A `const` cannot be `concat!`ed into another `const`.** Sharing the
  current-reading join between a books-driven and a readings-driven `FROM`
  needed a `macro_rules!` expanding to one string literal.
- **The 500-id chunk guard cannot be tested behaviourally.** Measured: remove the
  split entirely and 1,203 ids still bind and still pass, because the SQLite
  sqlx bundles allows 32,766 parameters — the failure is reserved for whichever
  older build a user happens to have. `book_summaries` chunks and nothing proves
  it does. The guard has to read the generated SQL.
- **`#[serde(default)]` is a *request*-compatibility device.** On a reply DTO it
  only lets an old payload parse a missing handle as `0` — a value a client
  follows to a `NotFound` it cannot tell from a deleted book. `FlashcardDto.book_id`
  is required; `highlight_id` keeps the default, because absent and not-anchored
  are the same true thing.
- **An unvalidated FK write is a *wrongly typed* error, not just a wrong row.**
  `flashcards.highlight_id` is a foreign key, so a stale id comes back as
  `FOREIGN KEY constraint failed` — `internal` for what is really `NotFound`.
  Re-reading the pair fixes the error class as well as the mismatched-book hole.
  `link_foreign_record` had already recorded this one table over.

## Threads that pushed back, and were right

Both did, on six points between them. Every one of the following was specified
in the prompt file and overturned by the worker:

- `#[serde(default)]` on `FlashcardDto.book_id` — wrong for the reason above.
- "Do not change `citations_for`'s ordering" — the required
  batch-agrees-with-single-call property was **not assertable** without a
  tie-break, because `page` and `ko_datetime` are both nullable and the two
  queries are different plans over different row sets. The property would have
  been green by luck. Both statements now name one `CITATION_ORDER` const.
- "Assert chunking with more than 500 ids" — produces a test that cannot fail.
- The passage's shape on item 43's list: the prompt offered a distinct row DTO
  *or* an opt-in flag; the flag was rejected because `passage: null` would mean
  *this reading has no marks* on one call and *you did not ask* on another.
- The read number's `None`: the prompt warned the fold would acquire a fourth
  meaning on a DTO. The correction is that **two thirds of the fold is
  unreachable on a list of readings** — unattributed and not-in-list are facts
  about a *highlight*. What is left is "read once", honestly `of_reads == 1`, so
  the wire carries `read_number` and `of_reads` and neither is an `Option`.
- The index migration's control had to be **built**: `0016` has
  `BookSort::Progress` as a live unindexable sort proving its assertions can
  fail; every arm of `ReadingSort` is indexed, so the control is the year filter
  written the old way and watched to lose the index.

## Verification

- `make ci` exit 0 on `main` after **each** merge — `0515a63` (45/46) and
  `298ac6c` (43/41). Full gate: fmt, lint, build-check, ts-check, whole-workspace
  test, web-check, 81 Playwright routes on WebKit.
- Each thread additionally gated on `make fmt lint build-check test ts-check`
  inside its worktree before reporting.
- **No existing `Request` variant changed shape** in either thread — thread B
  verified it mechanically by parsing the `Request` union out of old and new
  `bindings.ts` and diffing per method. `API_VERSION` stays at 2.
- Migrations run to `0018`.

## Deferred, with numbers

- **The GUI that consumes all four.** Every item here mints a request and draws
  nothing: the wall of cards over `ListReadingRows`, the cited-passage mark over
  `CitationsForNotes`, a capture control over `CreateFlashcard`.
  `gui/CLAUDE.md`'s standing instruction not to build the citations N+1 is now
  satisfiable rather than binding.
- **`Engine::cite` throws away `add_citation`'s bool** — the same information
  loss `CreateFlashcard` refused, one table over. Fixing it is
  `Response::Unit` → `Response::Bool` on a shipped method, i.e. a wire change.
- **`notes.created_at` has no index**, so `notes_created`/`links_created` were
  written to the bare column for one dialect rather than for a plan. They become
  cheap the day a migration indexes it, with no query change.
- **Image export of a card** — still item 28's finding: a card is live DOM with
  async loads, so it needs canvas rasterisation or a Rust-side renderer.
- **Item 44's passage rule still wants confirming** — longest passage, ties by
  lowest highlight id, which selects for the longest *drag* rather than the best
  passage.
