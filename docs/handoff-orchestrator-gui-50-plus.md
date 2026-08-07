---
title: Orchestrator handoff — the requests are spent, and what the spending found
date: 2026-08-07
source: the wave that built items 47, 48 and 49 (this file's predecessor described
        that wave as still to come; it is done, and this is the rewrite it asked for)
for: an orchestrator thread running workers, one worktree each
---

# Orchestrator handoff — after items 47, 48, 49

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where the tree stands

- `main` at **`51e17cd`**, working tree clean, nothing pushed (this repo has no
  remote it uses).
- **`make ci` exit 0** on that commit — fmt, clippy `-D warnings`, plain
  `cargo check --workspace`, `ts-check`, whole-workspace tests, `web-check`, and
  **102** Playwright routes on WebKit (81 before this wave).
- Highest applied migration is **`0018`**. **Nothing is outstanding.**
- `API_VERSION` is **2** and must not move.
- Routes today: `/`, `/book/[id]`, `/book/[id]/cards`, **`/cards`**, `/life`.
- Next free item number is **50**. `grep '^[0-9]\+\. \*\*' docs/decisions.md` is
  the register; `docs/prompts/` is not and under-reports permanently.

## What the last wave did

Three requests that had no caller now have one. `ListReadingRows`/`CountReadings`
draw `/cards`, a paged wall with a year filter and three sorts;
`CitationsForNotes` draws the quoted mark in the book view; `CreateFlashcard` is
a capture control on the passage. `/book/[id]/cards` moved onto the same row
query and its N+1 is retired.

Two threads, sequential, both merged with **zero** conflicts — thread B was cut
from the finished `main` rather than rebased afterwards, which is the third wave
running that this has been measured as the thing that buys it.

**All three workers pushed back on a specified point and all three were right.**
Keep asking for it by name; the count is now nine over three waves and the
overturned points have been better than the specified ones every time.

## The register's own pathology, one level deeper

The last wave's premise was *an unconsumed request is a claim nothing can
check*. It spent three and **found a fourth on the way out**, which is the first
thing the next thread should look at:

**Item 40 landed and the GUI never called it.** `search_marks` carries
`book_id`, `source` and `limit` on the wire today — the engine item is done and
has a `decisions.md` entry. `gui/src/lib/api/client.ts` does not have the method;
the only occurrence of `search_marks` under `gui/` is in the generated
`bindings.ts`.

Worse than unconsumed: **`gui/CLAUDE.md` states a fact about the API that is now
false.** Under *The book view* it still says

> The **note search is deliberately absent**: `SearchMarks` has no `book_id` and
> its `limit` is over the global ranked list […] Item 40 is the engine change.

`SearchMarks` *has* `book_id`. That is a stale prohibition of exactly the kind
item 48 was sent to fix in two other texts, and the next thread will obey it —
which is how it survived a whole wave in a file every GUI thread is told to read
in full. `NotePane.svelte:212` carries the matching marker comment.

**Item 50 is the note search**, and it is small: one input, `SearchMarks`
narrowed to the book, the two texts corrected. Check whether *any other* text in
`gui/CLAUDE.md` names an item as owed that has since shipped before you start —
the file has now been wrong this way twice.

## The rest of the backlog, in the order I would take it

Every one of these is recorded in a `decisions.md` entry by the thread that
found it; none is a guess.

1. **Item 50 — the note search.** Above.
2. **A `ReadingYears` request.** `/cards`'s year picker has **no request behind
   it**: the years come from `ActivityByMonth`, which is a proxy and is honest
   about being one. Two consequences ship today — a library that has never run
   `rb activity --refill` offers no years at all (the switch is simply absent),
   and a year can be offered because a *note* was written in it while no read
   ended. The shape: group `readings` by the year of `finished_at` with
   `ReadingFilter::predicate` composed in, so the picker and the wall agree by
   construction rather than by coincidence.
3. **The word *yet*, in six places.** The axiom bans it by name and it was cut
   from two empty states two waves ago; six user-facing strings survive — one on
   **the shelf**, four from item 27, one from item 28. None are from the last
   wave, and both new routes carry comments forbidding it, so the rule is being
   taught correctly in new code while older violations sit under it. The
   assertion that would have caught them is one word short:
   `the library surface greets you with no numbers` checks the body for *unread*
   / *streak* / *goal* / *to-read* / *remaining*, and not *yet*. Fix the strings
   and the assertion together or the next one lands the same way.
4. **No hostile string has ever reached `/cards`.** The 220-character title, the
   null title, the RTL and the CJK books all carry `reading_state: null` in
   `fake.ts`, so they mint no reading and appear on no card. The wall's title box
   is the same composition the tile and the book hero exercise with those exact
   titles, which is a reason to expect it holds and **not** evidence that it
   does. Fixing it means changing a declared `state` in
   `crates/corpus/edge-cases.json` **and** `devdb.rs` — item 38's declaration is
   asserted from both sides, so this is one small coordinated change and not a
   frontend edit.
5. **There are no dark-theme screenshots anywhere in the suite.** Every contrast
   finding this repo has ever made — including the two the last wave fixed and
   the one it declined — is light-theme only. `gui/CLAUDE.md` requires a page to
   be theme-aware in three states; nothing renders the other two.
6. **`--line` on `--bg` measures 1.28:1.** An unselected pill's boundary is
   carried entirely by its dim label, and `/cards` added six more pills. The
   token borders everything in the app, so moving it is its own item and wants
   the shots from (5) first.
7. **`citations` ties no note's book to the highlight's**, so a note filed under
   no book can quote a passage and appear in no page of notes. The quoted mark
   therefore **under**-reports and cannot over-report, which is the safe
   direction. There is no reverse query. Recorded, not opened as an item.
8. **`/cards`'s paging control is in no screenshot.** `PAGE` is 24 and `fake.ts`
   holds ten readings, whose size is item 38's declaration and not a frontend
   choice. Its arithmetic is unit-tested; its markup wants `make dev-db` and a
   real library. Do not shrink the page size or inflate the fixture to make a
   control appear — that is picking a fixture to flatter a picture.
9. **The Cite button embeds a note title with no `max-width`** and no fixture has
   a long one. Item 27's control, item 27's fixture.

## Worker mechanics — do not re-derive these

Every one was paid for by a previous wave, and the two marked **new** were paid
for by this one.

- **Make every worker check its base before it writes a line.** `git log
  --oneline -1` and `ls crates/engine/migrations/ | tail -2`. Four of six
  worktrees in the first GUI wave were cut ~80 commits behind `main`.
- **A GUI worker's real gate is `make web-check` plus `make routes`**, which need
  `gui/node_modules`. A fresh worktree has none, so **run `pnpm install` in the
  worktree's `gui/` as part of cutting it**, or the worker prints `SKIPPED:` and
  passes unrun. `make fmt lint build-check test ts-check` besides — cheap, and it
  catches a `bindings.ts` touched by accident.
- **The orchestrator runs the full `make ci` from the main checkout after each
  merge.** That is the only place the whole gate is real. It takes ~10 minutes,
  mostly workspace tests; run it in the background and *read the exit code from a
  file*, because the harness's own report of a backgrounded wrapper is not the
  command's status.
- **APFS-clone `target/` into each worktree** (`cp -Rc target <wt>/target`, then
  `rm -rf <wt>/target/debug/incremental`). Measured: seconds, and zero net disk.
- **Cut the last worker's worktree from the finished main.** Measured three waves
  running. Rebasing afterwards produces semantic conflicts git cannot see.
- **Tell a worker what landed under it since its prompt file was written.** (New.)
  Thread B's base had a reshaped `Card.svelte`, a new route, changed fixture data
  and **60 regenerated PNGs** from an unrelated contrast fix. Without being told,
  a worker reads that shot churn as its own breakage.
- **Never read a piped report, only a piped exit code.** `make test | tail -25`
  reports *tail's* status, and `git merge | tail -3` once hid four `CONFLICT`
  lines above the one it showed.
- **A subagent with no `SendMessage` reports to the orchestrator, not to the
  worker that spawned it.** This bit again: thread B's first `screenshot-reviewer`
  sat completed-but-unfinished for ~6.5 minutes, the worker spawned a second, and
  then the first reported anyway. Both were used. If a thread goes quiet after
  its tests would have finished, relay it.
- **Remove the worktrees when the wave closes.** The permission classifier
  declines `git worktree remove` from the agent, so it is the user's command —
  hand it to them rather than leaving 70G+ per tree behind.

## The agents

- **`api-surface-auditor` first**, per item, before a line of Svelte. It is what
  turns *"I'll just add a field above the seam"* into an engine item.
- **`web-checker`** after touching anything under `gui/`.
- **`screenshot-reviewer`** before calling any screen done, and **not optional**.
  It is the only check here that can see. This wave it caught a sort control that
  read as a state filter — a lit *Finished* above cards saying `Reading` and `Put
  down`, the screen disproving its own control — a card loading the hero shot
  into an 84px box, a mark and a toggle separated by hue alone, two notes
  rendering as one, and the wordmark using `--accent` as text at 2.78:1 on a line
  no item had touched since the scaffold. Every one passed every assertion.
- **`gui-component`** skill for any new component or route.

## Two traps this wave found that are not in any other list

- **`\s` crosses newlines**, so an axiom guard spelled `/\d+\s+cards?\b/` matched
  `p. 44` on one line above a `Cards:` line on the next. A guard that fires on the
  wrong thing is a guard you delete. Use a literal space.
- **Headless WebKit does not word-select on `dblclick`** under Playwright — the
  selection stays collapsed and the app is fine. Drag instead. Probe before
  concluding the app is wrong; this cost a detour.

## What "done" looked like for the last wave, for calibration

Three `decisions.md` entries, each recording **the corrections building it
forced** rather than a summary of what was built — entry 47 is the model, and its
findings section is longer than its description. That paragraph is the most
valuable thing an item produces and it is the one that gets skipped when the
tests go green.
