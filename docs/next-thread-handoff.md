---
title: Handoff — the non-GUI wave is finished; the GUI half is what is left
date: 2026-08-06
source: sessions/2026-08-06-one-search-surface.md (34),
        sessions/2026-08-06-the-sort-keys-get-an-index.md (35),
        sessions/2026-08-06-a-real-pdf-sidecar.md (37),
        sessions/2026-08-06-the-chooser-knows-who-wrote-it.md (36),
        sessions/2026-08-06-one-fixture-two-consumers.md (38);
        docs/decisions.md entries 34–38 for the rulings
supersedes: docs/handoff-orchestrator-non-gui-wave.md, all five of whose items
            have landed — and with it the engine handoff before that, seven of
            whose eight open items are now closed
---

# Handoff

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where things stand

**`make ci` exit 0 on `main` at `b21a0ed`.** Working tree clean, committed
straight to `main` (nothing is pushed; this repo has no remote it uses). The gate
is the full one: fmt, lint, build-check, ts-check, whole-workspace test,
web-check, and 30 Playwright routes on WebKit.

**Items 34, 35, 36, 37 and 38 are merged** — the whole non-GUI wave. Migrations
`0015` (highlight FTS) and `0016` (sort-key indexes) are applied. **`0017` now
belongs to item 23**, which moved down from `0015` so this wave could land ahead
of it.

**`API_VERSION` is 2.** Item 34 removed `SearchNotes` and `Response::NoteHits`.

`docs/decisions.md` is in **build order, not numeric order** — this wave appended
37, 34, 35, 36, 38 in that sequence. Read those five before touching any of it.

## Read this first: item numbers are registered in `docs/decisions.md`

This wave allocated itself 33–37 by reading `docs/prompts/` for the highest
number, and shipped **two items numbered 33** before anyone noticed. The one it
collided with — "Surfacing 21/29/30/31/32" — was minted mid-session rather than
from a spec, so it has a `decisions.md` entry and a session log and **no prompt
file**. The prompts directory under-reports permanently, and any item that starts
life as a handoff's open work has that shape.

**`grep '^[0-9]\+\. \*\*' docs/decisions.md` is the register.** The next free
number is **39**.

Renumbering cost one commit touching 32 files, because a worker writes its item
number into module headers, migration headers, test section comments and
`CLAUDE.md` routing rows. A number is a fact scattered through the source, not a
label on a document. `new-wave-item`'s step 2a now says so.

## What this wave overturned, in one line each

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

## Traps this wave found, all still live

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
`grep '^[0-9]\+\. \*\*' docs/decisions.md`, and it is the register because
`docs/prompts/` under-reports permanently — an item minted mid-session gets a
`decisions.md` entry and no prompt file. Items **40–46** below are *allocated
from an audit but mostly unbuilt*, which is a state this file has not had
before: check `decisions.md` for an entry before assuming one is open.

### Landed this session

Items **26** (the shelf), **39** (the accent) and **23** (moments, migration
`0017`) are merged, and `make ci` is exit 0 on `main`. Items **40** (scoped
search) and **27** (the book and the notes) are **in flight** in worktrees as
this is written — check `git branch` before starting either.

The WebGL spine shelf was **deferred as cosmetic** by the user and item 26 was
rebuilt around that: the shelf's arrangement is now a seam
(`gui/src/lib/shelf/layouts.ts`) with two implementations, and the spine shelf is
a third entry in it. `docs/decisions.md` entry 26 has the argument.

### The remaining GUI work

1. **Item 28 — the chain, and the reading-life page.** The last GUI item.
   Item 23 gave it `PendingMoments`/`AcknowledgeMoment` with `reading_id` on the
   moment, so the first link of the chain exists.

   **The card is per-book, reached by selecting a book** — decided by the user
   this session. `ListReadings { book_id }` already serves that. **The wall of
   cards across the whole library, with a year filter** (`gui-vision.md:151`) is
   deliberately *later*, and it needs item 43 first: nothing in the engine can
   list finished readings across books at any layer, and `BookFilterDto.year` is
   the *publish* year, not the year finished.

   Two traps an audit named in advance. `ActivitySummaryDto.activity_days` **must
   not become a streak** — it is a count of days in a range you asked for, past
   tense, and a "current streak" rendered from it is a threshold in a costume.
   And bucketing `ActivityByDay` to months in TypeScript **collapses `null` to
   `0`** on the first `reduce`, which is exactly the lie the page exists to
   avoid; that is what item 42 is for.

### Items minted by the audit, and not yet built

Numbers are allocated. None has a prompt file; all are additive with
`#[serde(default)]`, so **`API_VERSION` stays at 2** and none needs a migration.

- **40 — a search that can be scoped.** *In flight.* `book_id` on `SearchMarks`,
  `reading_id` on `ListNotes`. Blocks item 27's search box.
- **41 — the read number crosses.** `ReadNumbering` is the engine's (item 17c)
  and crosses no DTO, so a frontend showing which read a passage came from would
  compute `readings.indexOf(id) + 1` and silently depend on list ordering.
  Prefer `ReadingDto.ordinal` over a field on every highlight.
- **42 — the month is a period too.** `activity_by_month`, `GROUP BY
  substr(day,1,7)`, only months carrying an event. See the trap above.
- **43 — readings across the library.** The rows behind
  `ActivitySummary::books_finished`. **This is the wall's blocker.**
- **44 — the card's passage, chosen once.** "One passage pulled from the
  highlights" is a selection predicate and those are the engine's; `highlights[0]`
  in TypeScript is a frontend inventing one, and the TUI would then disagree.
  Which rule — longest, first, most-annotated, cited — is a product decision.
- **45 — a flashcard can be made.** `Storage::insert_flashcard` has had no
  `Engine` wrapper and no request since it was written. `FlashcardDto` also
  carries no `book_id`/`highlight_id`, so a card cannot be shown beside its
  passage.
- **46 — which passages are already cited.** Today that is one `CitationsFor`
  per note — an N+1 on a list, which is the pathology item 18 exists to remove.

2. ~~**The duplicated border-median accent arithmetic**~~ — **settled by item
   39.** The renderer reads `books.cover_accent` and its own loop is gone.

   Kept here because the *reason* it survived three handoffs is worth not
   repeating. This entry used to say the two "can legitimately differ" because
   `images.rs` measures the original file and `texture.rs` the *scaled texture*.
   That was not true and had never been: `texture.rs` called
   `accent_from_border(&img)` on the full-resolution decode and resized on the
   next line, against the same file `images.rs` measured — same bytes, same
   arithmetic, identical medians by construction. A plausible-sounding
   difference nobody re-read the code to check outlived the item that was
   actually supposed to do the work (item 20's comment named **item 19**; item
   19 shipped and did not).

3. **`Book::display_title` renders a stored blank as a blank.** It is
   `self.title.as_deref().unwrap_or("(untitled)")`, and `books.title` is
   `TEXT NOT NULL DEFAULT ''` — so the branch it guards is unreachable for any
   book that came out of the database, and the reachable case falls straight
   through. 76 call sites, and three sibling copies in `goodreads.rs`,
   `calibre.rs` and `ko_statistics.rs` where `None` genuinely *is* reachable (a
   CSV row, a calibre row, a statistics row). Found by item 38 and deliberately
   not patched inside a fixture item. The GUI is unaffected — `titleLabel`
   handles both — so this is the TUI, the CLI, and `MatchCandidate.title`.

4. **A table-shaped `pos0` is storable, and this is what it would take** (item
   37's deferral). Storing a PDF anchor rather than counting it means deciding
   what a coordinate table serialises to in the `pos0` column, and `pos0` is
   `identity_hash` material — coordinates drift on re-render exactly as `pageno`
   does, so a coordinate-bearing `pos0` re-inserts the same highlight after every
   re-render. `DeviceDigest` and `DEVICE_FIELDS_DIFFER` would have to agree too.
   **It needs a real PDF sidecar before it needs code**: `docs/koreader-format.md`
   §6 states that table-ness is settled from KOReader's source while the *key
   names* in the fixture are a reconstruction nobody has observed. Do not blur
   that line.

5. **`scan_device` still says nothing about unstorable anchors** (item 37,
   recorded not built). `DeviceState`'s four-name vocabulary — New / Unchanged /
   Updated / Unreadable — has no "readable, and partly unstorable", so the
   silence item 37 removed from import survives one layer over in scan.

6. **A generator that emits `fake.ts` from the declaration** (item 38's
   deferral). Item 38 made the drift loud rather than removing the second
   fixture, and the second fixture is not going away — layer 2 must run in a bare
   browser with no IPC. What *could* be generated is `fake.ts` itself. Worth doing
   when `edge-cases.json` has earned more fields than it has now.

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
