---
title: Handoff — the GUI wave is finished; what is left is engine work
date: 2026-08-06
source: docs/decisions.md entries 26, 39, 23, 40, 27, 42, 44, 28 — the eight
        items of the GUI wave, in the order they were merged
supersedes: the non-GUI-wave handoff this file used to be. Both of its open
            items are closed (the accent duplication by 39; the GUI half by
            26/27/28), and its numbering advice is superseded by the register
            note below — an allocated number no longer means open work
---

# Handoff

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where things stand

**`make ci` exit 0 on `main`.** Working tree clean, committed straight to `main`
(nothing is pushed; this repo has no remote it uses). The gate is the full one:
fmt, lint, build-check, ts-check, whole-workspace test, web-check, and **81**
Playwright routes on WebKit — 29 Rust suites and 172 vitest tests beside them.

**The GUI wave is merged**: items 25, 26, 27, 28, plus the engine work they
needed — 23 (migration `0017`), 39, 40, 42, 44. Migrations run to **`0017`**.

**`API_VERSION` is still 2.** Everything this wave added to the wire was
additive; items 40, 42 and 44 all say so explicitly in their entries.

`docs/decisions.md` is in **build order, not numeric order** — this wave appended
26, 39, 23, 40, 27, 42, 44, 28 in that sequence.

## Read this first: item numbers are registered in `docs/decisions.md`

The previous wave allocated itself 33–37 by reading `docs/prompts/` for the
highest number and shipped **two items numbered 33** before anyone noticed. The
one it collided with was minted mid-session rather than from a spec, so it has a
`decisions.md` entry and a session log and **no prompt file**. The prompts
directory under-reports permanently, and any item that starts life as a
handoff's open work has that shape.

**`grep '^[0-9]\+\. \*\*' docs/decisions.md` is the register.** The next free
number is **47**.

This wave added a second failure mode to the same lesson, in the opposite
direction: an API audit **allocated 40–46 and built only four of them**, so the
register now contains numbers with no entry behind them. A gap in the register
is not evidence that work is open, and a number in this file is not evidence
that it was built. Only a `decisions.md` entry is.

Renumbering cost one commit touching 32 files, because a worker writes its item
number into module headers, migration headers, test section comments and
`CLAUDE.md` routing rows. A number is a fact scattered through the source, not a
label on a document. `new-wave-item`'s step 2a now says so.

## What the *previous* (non-GUI) wave overturned, in one line each

- **bm25 cannot rank a note against a highlight** (item 34). fts5 computes rank
  from *that index's own* corpus statistics, so a note's −8.2 and a highlight's
  −8.2 are not the same claim and no constant converts one into the other. The
  prompt asked for one bm25-ordered list; what landed is one list merged by
  within-source position, ties breaking on recency — the one key genuinely
  comparable across the two.
- **`BookFilter::title` is `LIKE`, not FTS** (item 34). It must compose with
  `count_books`, five sorts and offset paging out of one shared `WHERE`; it must
  behave like its five neighbours or callers guess; and **infix is what a shelf
  filter means** — `possess` finds *The Dispossessed* under `LIKE` and nothing
  under fts5.
- **`sort_title` left `MERGE_RULES`** (item 35). Giving the column a writer made
  a contradiction visible: the upsert was stamping `field_provenance` for a
  provider whose value `refresh_sort_keys` had already replaced. It now takes
  migration `0014`'s cover-metrics shape — one writer, no claim, ignored by every
  merge. Cost, stated: no user-settable filing name, a door nothing had opened.
- **The sort index is on `COALESCE(sort_title, title) COLLATE NOCASE`** (item
  35), not the bare title the prompt asked for. Indexing the bare column would
  have left `sort_title` written and never read — the same trap one step along.
- **A PDF skip carries no `ErrorClass`** (item 37). That type classifies an
  `EngineError`, and nothing failed: the file read, the chunk evaluated, the
  entry is well formed. One `SidecarAnchorsUnsupported` **per file** carrying a
  count, not one per entry — 300 diagnostics for a 300-highlight PDF replaces
  silence with noise.
- **A `MatchCandidate` carries three fields, not a whole `BookDto`** (item 36),
  measured rather than argued: 1846 B against 98 for a book with a real blurb,
  still 10× with the prose stripped, and candidates are produced per *row* of an
  import. `BookDto` also carries `progress`, which would invite a chooser to draw
  a progress bar on a book it is asking you to identify.
- **The declaration over the two fixtures is hand-edited** (item 38). Emitting it
  from `gen-devdb` would make the Rust half check its own output, and
  `crates/corpus`' whole value is being an independent oracle.

## Traps the previous wave found, all still live

- **A cast defeats a fixture's own stated purpose.** `fake.ts` promised that a
  drifted DTO field is a `tsc` error there, and `as StoredBook` made that true
  for *renamed* fields and false for **added** ones — so item 20's whole cover
  cluster was missing from it for a wave. The habit: a fixture claiming to be
  type-checked must not cast.
- **A behavioural test that cannot fail is not a guard**, and it now has a second
  instance. Item 35 had to assert `EXPLAIN QUERY PLAN` because a behavioural test
  cannot see an index, exactly as item 18 had to read the SQL because removing
  its `books.id` tie-break left the partition test **green**.
  `a_sort_title_index_that_nearly_matches_is_not_used` is the sharper version:
  dropping the `COLLATE NOCASE` loses the index *silently*.
- **A column with no writer looks answered.** Closed for `sort_title` by item 35;
  the habit stands. Before leaning on a column, check something writes it.
- **Absence is not zero, and `''` is not `NULL`.** `books.title` is
  `TEXT NOT NULL DEFAULT ''`, so a stored title is never NULL — and `fake.ts` had
  been modelling `null` there, a state no library can produce.
- **A number in prose is a claim nothing checks.** `make dev-db` printed a
  hand-written "220 books, 20 of them" that went stale the moment item 38 added a
  case. The generator prints the counts now and the Makefile states none.

## What is next

**Next free number is 47.** The register is
`grep '^[0-9]\+\. \*\*' docs/decisions.md` — `docs/prompts/` under-reports
permanently, because an item minted mid-session gets a `decisions.md` entry and
no prompt file. **An allocated number is no longer evidence that work is open**:
this session allocated 40–46 from an audit and built only four of them. Check
`decisions.md` for an entry before assuming.

## The GUI wave is finished

Items **25, 26, 27 and 28** are all merged. `make ci` is exit 0 on `main`:
29 Rust suites, 172 vitest tests, 81 Playwright routes on WebKit.

Built this session, in merge order: **26** (the shelf), **39** (the accent
duplication), **23** (moments, migration `0017`), **40** (scoped search),
**27** (the book and the notes), **42** and **44** (the month as a period, the
card's passage), **28** (the chain and the reading-life page).

There is no half-built screen and no thread still running. The next wave starts
from a clean tree.

### Two rulings the user made, which a later thread must not quietly reverse

- **The WebGL spine shelf is deferred as cosmetic, not abandoned.** Item 26 was
  rebuilt around that rather than half-built toward it: the arrangement is a
  seam (`gui/src/lib/shelf/layouts.ts`) and the spine shelf is a third entry in
  it, needing no change to that file. Entry 26 has the argument.
- **A number on the home surface may describe one book, never the collection.**
  `gui-vision.md` said three times, unqualified, that no number appears there —
  and the shelf has shipped `Reading · 35%` under every tile since item 25.
  Item 27's screenshot review found the contradiction; the user resolved it in
  favour of the shipped behaviour and the rule is now stated precisely in both
  places that carried the absolute form.

### What is left, and none of it is a screen

- **41 — the read number crosses.** `ReadNumbering` is the engine's (item 17c)
  and crosses no DTO. Item 28 declined to fake it with `readings.indexOf(id)+1`
  and used dates instead; a card that wants "your second read" needs this.
- **43 — readings across the library.** The rows behind
  `ActivitySummary::books_finished`. **This is the blocker for the wall of
  cards** and the year filter over it (`gui-vision.md:151`). The user chose
  *per-book cards now, the wall later*, and item 28 shipped the per-book form —
  so this is the item that reopens it. Item 44 notes that its one-call-per-card
  passage is right for a per-book card and **wrong for a wall of 400**.
- **45 — a flashcard can be made.** `Storage::insert_flashcard` has had no
  `Engine` wrapper and no request since it was written; `FlashcardDto` carries
  no `book_id`/`highlight_id`, so a card cannot be shown beside its passage.
- **46 — which passages are already cited.** Today one `CitationsFor` per note,
  an N+1 on a list — the pathology item 18 exists to remove.
- **Image export of a card.** Item 28 left it out and said why: a card is live
  DOM with async loads, so it needs canvas rasterisation or a Rust-side
  renderer. Not a wiring job.
- **The passage rule wants confirming.** Item 44 chose *longest passage, ties by
  lowest highlight id*, and stated the cost rather than hiding it: it selects
  for the longest **drag**, not the best passage, so a mis-drag outranks the
  sentence you loved. It refused a length cap because that is a magic number
  claiming how long a passage may be.

## Two process facts this session paid for

- **Four of six agent worktrees were created ~80 commits behind `main`**, with
  migrations stopping at `0010`. Every thread caught it because its prompt said
  to check, and one of them would otherwise have written a migration into a
  five-version gap. **Put the base check in the prompt, every time**:
  `git log --oneline -1` and `ls crates/engine/migrations/ | tail -2` before
  writing anything.
- **An engine item that changes a request shape can only fail on the frontend,
  and a worker cannot see it.** `ts-rs` emits a new field as **required** in
  TypeScript however `#[serde(default)]` the Rust is. Item 40 broke
  `gui/src/lib/api/client.ts` that way; its own gate passed, because a fresh
  worktree has no `gui/node_modules` and `web-check` prints `SKIPPED:`. Only
  `make ci` from the main checkout caught it. Prefer **adding** a request over
  changing one, and when you must change one, say so loudly.


## Running a wave as multiple workers

Three workers in parallel, then one, then one run by the orchestrator. What it
cost and what it taught:

- **A worker cannot gate on `make ci`.** A fresh worktree has no
  `gui/node_modules`, so `web-check` and `routes` print `SKIPPED:` and the worker
  "passes" them. Gate workers on **`make fmt lint build-check test ts-check`**.
  The orchestrator runs the full `make ci` from the main checkout after each
  merge.
- **Never read a piped report, not just a piped exit code.** The known rule is
  that `make test | tail -25` reports *tail's* status. The instance that actually
  bit this wave was `git merge | tail -3`, which showed the **last** `CONFLICT`
  line and hid four above it — four files were committed with markers in them,
  and `make ci` caught it two steps later. Same rule, different costume.
- **Cut the last worker's worktree from the *finished* main.** Item 36 collided
  with 34 on the API and with 37 on `koreader.rs`, was given a base containing
  both, and merged with **zero conflicts**. Reset the worktree onto finished main
  rather than rebasing afterwards — rebase-after produces semantic conflicts git
  cannot see.
- **Merge order is constrained by migration contiguity, not just by files.**
  `migration_versions_are_contiguous_from_one` fails on a **gap**, so the item
  holding `0016` cannot merge before the item holding `0015`, whatever the file
  collisions say. This wave's stated order had to be reversed for exactly that.
- **`docs/decisions.md` is the guaranteed conflict.** Every merge conflicted
  there, and three of them conflicted nowhere else. Tell every worker to *append*
  and restructure nothing; the resolution is then deleting three marker lines.
- **APFS-clone `target/debug` into each worktree** (`cp -Rc`), stripping
  `incremental/` — it was 29G of 59G. Measured this wave: **70 seconds** per
  worktree, total disk 56Gi → 55Gi free, and a cold `cargo check` in a cloned
  worktree finished in **15.8s**, so fingerprints survive the path change. Budget
  minutes, not the ten minutes the previous handoff guessed. `.claude/worktrees`
  is gitignored and is where they belong.
- **Workers push back, and they are usually right.** Four of five did here, and
  every overturned decision above came from one. Ask for it by name in the prompt
  file.

## Everything below here is unchanged

The GUI seam (`bindings.ts` is generated; `cover_path` is a whole path; the asset
protocol scope is set at runtime; `TauriClient` must never fall back to the
fake), the engine-internals rules (`MERGE_RULES` generates six things; a `user`
claim protects a field *pair*; absence is not zero, anywhere), the fixture rules
(`corpus` must never depend on `readingbuddy`; `notes_fts` has no triggers;
`reading_events` is not seeded), the two cloud-session constraints (gutenberg.org
is blocked by the sandbox proxy; Playwright needs
`pnpm exec playwright install webkit`), and the four agents and three skills.
`CLAUDE.md` and the per-crate files carry all of it, and this wave updated them
where it changed something.

## What is not play data

`dev-data/` is disposable and rebuilt by `make dev-db`; the user stated on
2026-08-06 that all data currently in use is play data that may be deleted.
**Re-check that before relying on it** — it expires the day a real library
exists, and with it the "renumbering a migration is legal" and "a shape change
needs no data migration" shortcuts this wave used.

What stays untouched regardless: `personal_data/`, the `real/` fixtures, and
anything on a mounted KOReader device.
