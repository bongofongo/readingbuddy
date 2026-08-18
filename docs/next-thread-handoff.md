---
title: Handoff — the API audit's backlog is closed; what is left is a frontend
date: 2026-08-07
source: docs/decisions.md entries 45, 46, 43, 41 — the four items of this wave,
        in the order they were merged
supersedes: the GUI-wave handoff this file used to be. Its four open items are
            all built; its two process rules are now in the root `CLAUDE.md`
            rather than only here.
---

# Handoff

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where things stand

**`make ci` exit 0 on `main`.** Working tree clean, committed straight to `main`
(nothing is pushed; this repo has no remote it uses). The gate is the full one:
fmt, lint, build-check, ts-check, whole-workspace test, web-check, and 81
Playwright routes on WebKit.

**The API audit's backlog is merged**: items **45**, **46**, **43** and **41**.
Migrations run to **`0018`**. `API_VERSION` is still **2** — everything this
wave added to the wire was additive, and **no existing `Request` variant changed
shape**, which both threads verified rather than asserted.

`docs/decisions.md` is in **build order, not numeric order** — this wave appended
45, 46, 43, 41 in that sequence.

## Read this first: item numbers are registered in `docs/decisions.md`

**`grep '^[0-9]\+\. \*\*' docs/decisions.md` is the register.** The next free
number is **47**, and unlike last time that is a real number: 40–46 are now all
entries rather than allocations. `docs/prompts/` under-reports permanently,
because an item minted mid-session gets a `decisions.md` entry and no prompt
file.

The standing warning still holds in both directions. An allocated number is not
evidence that work is open; a gap in the register is not evidence that it is.
**Only a `decisions.md` entry is.** Renumbering costs one commit touching ~30
files, because a worker writes its item number into module headers, migration
headers, test section comments and `CLAUDE.md` routing rows.

## What this wave overturned, in one line each

- **The read number cannot be a window function** (item 43). `ROW_NUMBER() OVER
  (PARTITION BY book_id …)` is computed over the rows that survived the `WHERE`,
  so a page filtered to 2025 holds a book's second read without its first and
  calls it read 1 — on a card whose entire sentence is "your second read". Two
  correlated subqueries over the unfiltered table instead.
- **Two thirds of `ReadNumbering`'s `None` is unreachable across the wire**
  (item 41). Unattributed and not-in-list are facts about a *highlight*; on a
  list of readings only "read once" survives, which is honestly `of_reads == 1`.
  So `read_number` and `of_reads` cross, **neither an `Option`**, and the
  four-way ambiguity is not shipped rather than being documented.
- **The passage belongs to the new list's row type, not to `ReadingDto`**
  (item 43). Item 44 said it must ride whatever list 43 mints; `CardPassage`
  refused to put it on `ReadingDto`. Both are right. The opt-in-flag alternative
  was rejected because `passage: null` would mean *no marks* on one call and
  *you did not ask* on another.
- **`#[serde(default)]` is a request-compatibility device, not a reply one**
  (item 45). On a reply DTO it only lets an old payload parse a missing handle
  as `0` — a value a client follows to a `NotFound` it cannot tell from a
  deleted book.
- **A tie-break was required where the prompt forbade touching the order**
  (item 46). `page` and `ko_datetime` are both nullable, so two cited passages
  of one note can tie on the whole key; the batch and the single call are
  different plans over different row sets, so their agreement would have been
  green by luck.
- **`activity_summary` could never have used an index on `finished_at`** (item
  43, pre-existing since `0011`). `date(finished_at, 'unixepoch') BETWEEN ?` is
  an expression over the column. `DayRange::unix_bounds` converts once at the
  validated boundary now.

## Traps found this wave, all still live

- **Two threads appending to one file can merge cleanly into nonsense.** Git
  aligns a shared tail. Now in the root `CLAUDE.md`; the short form is *rebuild
  from each side's block, do not delete markers*.
- **A subagent with no `SendMessage` stalls the agent that spawned it.** A
  worker's `cargo-tester` reported to the orchestrator instead, and the worker
  sat completed-but-unfinished until it was relayed.
- **A guard that cannot fail is not a guard**, third instance. The 500-id chunk
  was measured: remove it and 1,203 ids still bind, because the bundled SQLite
  allows 32,766 parameters. The guard reads the generated SQL.
- **An index migration needs a control.** `0016` had `BookSort::Progress` — a
  sort no index can serve — proving its `EXPLAIN QUERY PLAN` assertions were
  capable of failing. Every arm of `ReadingSort` is indexed, so `0018` had to
  *build* one: the year filter written the old way, watched to lose the index.
- **A comma list's terms take their own direction.** Appending ` DESC` to
  `COALESCE(started_at, created_at), id` reverses only `id`.

## What is next, and all of it is a frontend

Every item this wave built mints a request and draws nothing. The requests are
there; the surfaces are not.

- **The wall of cards.** `ListReadingRows`/`CountReadings` is the query it was
  blocked on: a page carries book, reading, passage, `read_number` and
  `of_reads`, filterable by year, with the count as its own request. The year
  filter is `gui-vision.md:151`. `ReadingFilter::book_id` means
  `/book/[id]/cards` can be served by the same list — one call instead of
  `ListReadings` plus N `CardPassage` calls — so that screen should move onto it
  rather than a second one being built beside it.
- **The cited-passage mark.** `CitationsForNotes` carries ids for a page of
  notes in one call. `gui/CLAUDE.md`'s instruction not to build the N+1 is now
  satisfiable rather than binding, and the doc comment in
  `gui/src/lib/book/Passages.svelte` that records the refusal should be updated
  when it is.
- **A capture control for a flashcard.** `CreateFlashcard` exists and nothing
  calls it; until this wave a card could be minted by the KOReader import and by
  nothing else. `FlashcardDto` now carries `book_id`/`highlight_id`, so a card
  can finally be shown beside its passage.

Smaller, and none of it blocking:

- **`Engine::cite` throws away `add_citation`'s bool** — the same information
  loss `CreateFlashcard` refused, one table over. Fixing it is
  `Response::Unit` → `Response::Bool` on a shipped method, i.e. a wire change
  and an `API_VERSION` conversation.
- **`notes.created_at` has no index.** `notes_created`/`links_created` were
  written to the bare column for one dialect rather than for a plan; they become
  cheap the day a migration indexes it, with no query change.
- **Image export of a card.** Item 28's finding, unchanged: a card is live DOM
  with async loads, so it needs canvas rasterisation or a Rust-side renderer.
- **Item 44's passage rule still wants confirming** — longest passage, ties by
  lowest highlight id, which selects for the longest *drag* rather than the best
  passage. It refused a length cap because that is a magic number claiming how
  long a passage may be.

## Two rulings a later thread must not quietly reverse

Both are from the GUI wave and both still stand:

- **The WebGL spine shelf is deferred as cosmetic, not abandoned.** Entry 26 has
  the argument. **The seam it plugs into moved** with the layout rework (entry
  53): `gui/src/lib/shelf/layouts.ts` is gone — a layout used to own the whole
  band, and what varies now is *a group's field*, which `Wall.svelte` draws. That
  is a narrower contract and the one the ray tracer wants. What the switch offers
  is an **arrangement** (`arrangements.ts`), which is a different axis.
- **A number on the home surface may describe one book, never the collection.**
  Stated precisely in `gui-vision.md` and `gui/CLAUDE.md` after item 27's
  screenshot review found the contradiction.

## Running a wave as multiple workers

The whole of it now lives in the root `CLAUDE.md` under "Running a wave as
worktree threads" — base check, the `make fmt lint build-check test ts-check`
gate, the `ts-rs`-makes-a-new-field-required trap, the merge-splice trap, and
the stalled-subagent trap. Two facts worth repeating with numbers:

- **APFS-clone `target/` into each worktree** (`cp -Rc`), then strip
  `incremental/`. Measured again this wave: **100 seconds** per worktree and
  **zero** net disk, since a clone shares blocks.
- **Remove worktrees when the wave closes.** Six from the previous wave were
  still on disk at 188G with 19Gi free. The permission classifier declines
  `git worktree remove` from the agent, so it is the user's command to run.

## Everything below here is unchanged

The GUI seam (`bindings.ts` is generated; `cover_path` is a whole path; the asset
protocol scope is set at runtime; `TauriClient` must never fall back to the
fake), the engine-internals rules (`MERGE_RULES` generates six things; a `user`
claim protects a field *pair*; absence is not zero, anywhere), the fixture rules
(`corpus` must never depend on `readingbuddy`; `notes_fts` has no triggers;
`reading_events` is not seeded), the two cloud-session constraints (gutenberg.org
is blocked by the sandbox proxy; Playwright needs
`pnpm exec playwright install webkit`), and the four agents and three skills.
`CLAUDE.md` and the per-crate files carry all of it.

## What is not play data

`dev-data/` is disposable and rebuilt by `make dev-db`; the user stated on
2026-08-06 that all data currently in use is play data that may be deleted, and
that had not changed by 2026-08-07. **Re-check it before relying on it** — it
expires the day a real library exists, and with it the "renumbering a migration
is legal" and "a shape change needs no data migration" shortcuts.

What stays untouched regardless: `personal_data/`, the `real/` fixtures, and
anything on a mounted KOReader device.
