---
title: Orchestrator handoff — the search, the years, and the word that hid in an empty state
date: 2026-08-07
source: the wave that built items 50, 51 and 52 (this file's predecessor described
        that wave as still to come; it is done, and this is the rewrite it asked for)
for: an orchestrator thread running workers, one worktree each
---

# Orchestrator handoff — after items 50, 51, 52

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where the tree stands

- `main` clean, nothing pushed (this repo has no remote it uses).
- **`make ci` exit 0** — fmt, clippy `-D warnings`, plain `cargo check
  --workspace`, `ts-check`, whole-workspace tests, `web-check`, and **108**
  Playwright routes on WebKit (102 before this wave).
- Highest applied migration is **`0018`**. **Nothing is outstanding** — item 51
  wanted no migration, and `idx_readings_finished_at` covers its one statement.
- `API_VERSION` is **2** and must not move.
- Routes today: `/`, `/book/[id]`, `/book/[id]/cards`, `/cards`, `/life`.
- Next free item number is **53**. `grep '^[0-9]\+\. \*\*' docs/decisions.md` is
  the register; `docs/prompts/` is not and under-reports permanently — **items
  50–52 were minted from this file rather than from a spec row and have no
  prompt file**, which is item 33's shape and will keep recurring.

## What the last wave did

**Item 50** gave the book view a search over its own marks — the first caller
`SearchMarks`' `book_id` has ever had, two waves after item 40 built it. **Item
51** gave `/cards`' year picker a request (`ReadingYears`), retiring the
`ActivityByMonth` proxy. **Item 52** cut the word *yet* from eight strings and
added the guard that could actually have caught them.

**Every agent in this wave changed the design, and the two that mattered most
were the ones asked to look rather than to check.** The API auditor turned item
51 from "a request shaped like `CountReadings`" into a request carrying an
`open` bool, by finding that the years alone do not partition a wall that holds
open readings. The screenshot reviewer found the previous wave's own defect
reproduced — two lit brass chips saying opposite things — because a *mitigation's
premise* had expired while the mitigation still looked right. Keep running both,
and keep asking them to argue.

## The pathology this wave found, one level deeper again

Last time it was *an unconsumed request is a claim nothing can check*. This time
it is **a guard whose premise expired**, and there are two instances:

- `WallControls` labelled only its Order group, on the written argument that
  *All / 2025 / 2024* is self-evidently a filter. True while the group was
  years. Item 51 appended *Still reading* to it and the argument silently became
  false — nothing failed, and the screen went back to being ambiguous in exactly
  the way a previous review had fixed.
- `the library surface greets you with no numbers` would never have caught the
  *yet* strings: six of eight live in **empty states**, and the fixture is a
  library with things in it, so the markup is never on screen. `make routes`
  was green the whole time.

**When you add to a group, re-read the argument for how that group is drawn.**
And when a guard is proposed, ask which branch of the fixture it runs on.

## The rest of the backlog, in the order I would take it

Every one is recorded in a `decisions.md` entry by the thread that found it.

1. **There are still no dark-theme screenshots anywhere in the suite.** Every
   contrast finding this repo has made — including this wave's 2.35:1
   placeholder — is light-theme only, and `gui/CLAUDE.md` requires a page to be
   theme-aware in three states. Two of the three render nowhere. `/cards`' chips
   and item 50's `color-mix` mark ground are both unmeasured in them.
2. **`--line` on `--bg` measures 1.28:1.** An unselected pill's boundary is
   carried entirely by its dim label, and the wall now has seven pills. The
   token borders everything in the app, so moving it is its own item and wants
   the shots from (1) first.
3. **A card's only exit has no affordance.** `Card.svelte`'s title is an `<a>`
   drawn as plain bold ink with a hover-only accent — invisible to a keyboard or
   a touch screen — while `/book/[id]/cards` draws a brass link to the same
   place. Two links to one destination, drawn two ways. The *Still reading* wall
   exposed it: five of seven cards there carry one line of body and no other
   move.
4. **`search_marks` has no tie-break after `ORDER BY rank`**, unlike every other
   list in the repo. Harmless for one settled query and wrong the moment a *show
   more* re-asks with a larger limit — SQLite's sorter is deterministic per plan
   and not per schema, so the behavioural test stays green with the tie-break
   deleted (item 18's finding, now on its third table).
5. **A search hit does not say which field matched.** `snippet(…, -1, …)` picks
   the matched column and the DTO drops that, so nothing can label a hit *in
   your annotation*, and item 50 had to compare *text* to notice a note's
   snippet was its own title. A `field` on each arm of `SearchHitDto`, additive.
6. **The snippet's `>>`/`<<` markers are in-band and unescapable.** Real prose
   contains `>>`. `book/snippet.ts` degrades rather than guesses, and the honest
   fix is a structured snippet carrying offsets.
7. **No hostile string has ever reached `/cards`.** The 220-character title, the
   null title, the RTL and the CJK books all carry `reading_state: null` in
   `fake.ts`, so they mint no reading and appear on no card. Fixing it means
   changing a declared `state` in `crates/corpus/edge-cases.json` **and**
   `devdb.rs` — item 38's declaration is asserted from both sides.
8. **`/cards`'s paging control is in no screenshot.** `PAGE` is 24 and `fake.ts`
   holds ten readings, whose size is item 38's declaration. Its arithmetic is
   unit-tested; its markup wants `make dev-db` and a real library. Do not shrink
   the page size or inflate the fixture to make a control appear.
9. **`/book/[id]/cards` could take the year picker now** and does not.
   `ReadingYears` takes a `ReadingFilterDto`, so the same control narrows to one
   book — which the `ActivityByMonth` proxy could never do. Small, and the
   component is already written.
10. **`citations` ties no note's book to the highlight's**, so a note filed under
    no book can quote a passage and appear in no page of notes. The quoted mark
    therefore under-reports and cannot over-report. Recorded, not opened.
11. **The Cite button embeds a note title with no `max-width`** and no fixture
    has a long one. Item 27's control, item 27's fixture.

## Worker mechanics — do not re-derive these

Every one was paid for by a previous wave.

- **Make every worker check its base before it writes a line.** `git log
  --oneline -1` and `ls crates/engine/migrations/ | tail -2`.
- **A GUI worker's real gate is `make web-check` plus `make routes`**, which need
  `gui/node_modules`. A fresh worktree has none, so **run `pnpm install` in the
  worktree's `gui/` as part of cutting it**, or the worker prints `SKIPPED:` and
  passes unrun. `make fmt lint build-check test ts-check` besides.
- **The orchestrator runs the full `make ci` from the main checkout after each
  merge.** Run it in the background and *read the exit code from a file* — the
  harness's report of a backgrounded wrapper is not the command's status.
- **APFS-clone `target/` into each worktree** (`cp -Rc target <wt>/target`, then
  `rm -rf <wt>/target/debug/incremental`). Seconds, and zero net disk.
- **Cut the last worker's worktree from the finished main.** Measured four waves
  running. Rebasing afterwards produces semantic conflicts git cannot see.
- **Tell a worker what landed under it since its prompt file was written**,
  including regenerated PNGs — otherwise it reads shot churn as its own
  breakage.
- **Never read a piped report, only a piped exit code.** `make test | tail -25`
  reports *tail's* status.
- **A subagent with no `SendMessage` reports to the orchestrator, not to the
  worker that spawned it.** If a thread goes quiet after its tests would have
  finished, relay it.
- **Remove the worktrees when the wave closes.** The permission classifier
  declines `git worktree remove` from the agent, so hand it to the user.
- **Run `prettier --write` on every file you touched under `gui/`** before the
  gate. `web-check` fails on formatting and the failure arrives after everything
  slow has already run.

## The agents

- **`api-surface-auditor` first**, per item, before a line of Svelte. This wave
  it produced item 51's entire spec — the `open` bool, the `FROM readings` with
  no join, the covering-index measurement, and the frontend clamp that had to be
  deleted with it — and it confirmed item 50 needed **no** engine change, which
  is the answer that saves the most time.
- **`web-checker`** after touching anything under `gui/`.
- **`screenshot-reviewer`** before calling any screen done, and **not optional**.
  This wave it found a clipped placeholder at phone width whose clipped half was
  the only statement of what the search searched, a 2.35:1 browser-default
  placeholder colour nothing in the repo had ever set, a search box offered on
  books with nothing to search, and the two-lit-chips ambiguity above. Every one
  passed every assertion.
- **`cargo-tester`** for anything Rust. The workspace suite takes ~20 minutes;
  start it and do documentation while it runs.
- **`gui-component`** skill for any new component or route.

## What "done" looked like for the last wave, for calibration

Three `decisions.md` entries, each recording **the corrections building it
forced** rather than a summary of what was built — entry 51 is the model, and
the paragraph about the deleted `yearRange` clamp is the kind that matters: a
bug that could not exist under the old proxy and became reachable the moment the
data got better.
