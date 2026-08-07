---
title: Orchestrator handoff — the GUI wave the requests are waiting for (items 47–49)
date: 2026-08-07
source: docs/next-thread-handoff.md ("What is next, and all of it is a frontend");
        docs/decisions.md entries 43, 41, 45, 46 — the four requests this wave spends
for: an orchestrator thread running workers, one worktree each
---

# Orchestrator handoff — items 47, 48, 49

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation. **You are the orchestrator**: you settle the one open
decision with the user, write the prompt files, cut the worktrees, launch one
worker per thread, merge in a stated order, and run the real gate from the main
checkout after each merge. You do not build the items yourself.

## Where the tree stands

- `main` at **`ac1d644`**, working tree clean, nothing pushed (this repo has no
  remote it uses).
- **`make ci` exit 0** on that commit — fmt, clippy `-D warnings`, plain
  `cargo check --workspace`, `ts-check`, whole-workspace tests, `web-check`, and
  81 Playwright routes on WebKit.
- Highest applied migration is **`0018`**. **Nothing is outstanding**, and this
  wave takes none: every request it spends already exists.
- `API_VERSION` is **2** and must not move. If a thread believes it must, that
  is a conversation with the user, not a thing to do quietly.
- Routes today: `/`, `/book/[id]`, `/book/[id]/cards`, `/life`.

## What this wave is

The previous wave built four engine items and **drew nothing**. Three requests
now exist with no caller:

| request | shipped by | nothing calls it |
|---|---|---|
| `ListReadingRows` / `CountReadings` | item 43 | the wall of cards |
| `CitationsForNotes` | item 46 | the cited-passage mark |
| `CreateFlashcard` | item 45 | a capture control |

An unconsumed request is this repo's own recorded pathology one level up: *a
column that nothing renders is a claim nothing can check*. Three of them is a
wave.

## Read this first — the register, and what a number costs

**`grep '^[0-9]\+\. \*\*' docs/decisions.md` is the register of spent item
numbers.** `docs/prompts/` is not, and under-reports permanently. The next free
number is **47**, so this wave is **47, 48, 49**.

Allocating them here does **not** make them exist. The 2026-08-06 wave allocated
40–46 from an audit and built four; the register then held numbers with no entry
behind them for a wave. Only a `decisions.md` entry is evidence work was done.

A number is a fact scattered through the source — module headers, test section
comments, `CLAUDE.md` routing rows — so renumbering costs a commit touching
dozens of files. Check the register before you write a prompt file, not after.

## The one decision to settle with the user before anything is cut

**Does the wall of cards replace `/book/[id]/cards`, or sit beside it?**

This is genuinely the user's call and it changes what item 47 *is*. The
background:

- Item 28 shipped the **per-book** card at `/book/[id]/cards` and deliberately
  did not grow the reading-life page a wall, on the stated ground that a wall
  had **no request behind it**. That ground is now gone.
- Item 43 was built so one query serves both: `ReadingFilter::book_id` narrows
  the wall's question to a single book, and the row already carries the passage,
  `read_number` and `of_reads`. So `/book/[id]/cards` can stop making
  `ListReadings` plus N `CardPassage` calls **whatever** the user decides about
  the route.
- Item 44 recorded in advance that its one-call-per-card passage is right for a
  per-book card and **wrong for a wall of 400**.

The three shapes, with the recommendation first:

1. **A new `/cards` route, and `/book/[id]/cards` moves onto the same call and
   stays.** Recommended. The two screens answer different questions — *my
   reading life* and *this book's reads* — and a book's card page reached from
   the book is not a filtered view of a library wall in the user's head, whatever
   it is in the query. The N+1 dies either way.
2. **`/life` grows the wall.** Cheapest in routes, and it directly reopens what
   item 28 refused; the refusal's reason has expired, so this is legitimate
   rather than a reversal. Risk: `/life` is already a dense page and a wall of
   400 below the monthly figures is a different page wearing the old one's name.
3. **One route with a book filter, and `/book/[id]/cards` is deleted.** Fewest
   surfaces, and the one the query most naturally suggests. Refused unless the
   user asks for it: it makes a book's cards reachable only through a filter
   control, which is a dead end by way of a detour, and the axiom bans dead ends.

**Do not start item 47 before this is answered.** Everything else in the wave is
independent of it.

## The items

### Item 47 — the wall of cards

The substantive one. Over `ListReadingRows` + `CountReadings`.

- A page of cards, each carrying its book, its reading, the passage item 44
  chose, and **the read number** — `read_number` and `of_reads`, neither an
  `Option` (item 41). *Your second read* is `of_reads > 1`, the same test the
  TUI's gutter makes; a client must never re-derive it from array position.
- **The year filter** (`gui-vision.md:151`) through `ReadingFilter::finished_in`,
  which is a `DayRange` and is **fallible**: an inverted span is `InvalidInput`
  at both doors. A year picker cannot produce one, which is the point — the
  frontend must not carry a second validation dialect.
- **The count is its own request.** Ask it once per filter, not once per scroll.
  Item 18's ruling, and a wall is the case it was made for.
- **Paging is an offset.** Three sorts, all indexed by `0018`; there is
  deliberately **no title sort**, because that orders by a `books` column no
  index on `readings` can serve.
- **`/book/[id]/cards` moves onto the same call** regardless of the route
  decision. That is the item retiring a live N+1, not a nice-to-have.

Must not: invent a `CardDto`. Item 43's entry states the test — *a card would
grow the rating; this row will not*. The card is a **layout**, and a layout is
the frontend's composition of facts the API already serves.

### Item 48 — which passages are already cited

Small. Over `CitationsForNotes`.

- A mark on the passages some note already quotes, in the book view.
- `gui/CLAUDE.md` and the module doc at `gui/src/lib/book/Passages.svelte` both
  carry a standing instruction **not to build the N+1**. It is now satisfiable
  rather than binding, so **both of those texts must be updated by this item** —
  a stale prohibition is worse than none, because the next thread obeys it.
- One call for the page of notes the route already loads. The reply is
  **highlight ids**, not rows, and the page is already holding the highlights
  (`listHighlights` and `listNotes` are in the same `Promise.all`). A thread that
  finds itself fetching highlight text has taken the wrong call.
- `CitationsFor` (singular) stays and is not redundant: it feeds the pane that
  shows the passages themselves, where the words are the point.

### Item 49 — a card can be captured

Small. Over `CreateFlashcard`.

- A control that makes a flashcard from a passage. Until item 45 a card could be
  minted by the KOReader import and by **nothing else**.
- `FlashcardDto` now carries `book_id`/`highlight_id`, so a card can be shown
  beside its passage — which is the half of item 45 that makes the control worth
  having rather than a form.
- The reply is a **bool**: `true` created, `false` *you already had this card*
  (`UNIQUE(book_id, word)` dedupes and the existing card is left as it was).
  Those are different facts and the confirmation must tell them apart. A
  frontend that renders both as "saved" throws away the whole reason the write
  answers anything.
- No task-completion framing anywhere near it. No count of cards not yet made.

## The schedule, and why it is this one

**Two threads, sequential — not three in parallel.**

1. **Thread A — item 47**, alone, first.
2. **Thread B — items 48 and 49** together, cut from **finished** `main` after A
   merges.

Three reasons, and the first is the repo's own recorded rule:

- **`new-wave-item` step 4**: GUI items that share components must not run
  concurrently, because three agents on one app produce three dialects of it.
  48 and 49 are both controls in the book view and share `gui/src/lib/book/`
  outright, so they are one thread and not two.
- **Item 47 may reshape `gui/src/lib/card/`.** Item 28 built those components for
  one card on one page; a wall of 400 is where they either hold up or get
  refactored, and a thread editing the book view underneath that is a merge
  nobody wants to read.
- **Cut the last worker's worktree from the finished main.** Measured last wave:
  item 36 collided with two other items on paper and merged with **zero**
  conflicts because it was given a base containing both. Rebasing afterwards
  produces semantic conflicts git cannot see.

Running B in parallel with A is defensible — the file sets are close to disjoint
— and costs you the `docs/decisions.md` conflict plus the risk that both threads
independently decide what a "cited" mark looks like. Sequential is the
recommendation; if the user wants the wall clock back, say what it buys and what
it risks, and let them choose.

## Worker mechanics — do not re-derive these

Every one of these was paid for by a previous wave.

- **Make every worker check its base before it writes a line.** `git log
  --oneline -1` and `ls crates/engine/migrations/ | tail -2`. Four of six
  worktrees in the GUI wave were cut ~80 commits behind `main`.
- **Gate workers on `make fmt lint build-check test ts-check`** — *except* that
  this is a GUI wave, and those five do not check a screen. A GUI worker's real
  gate is **`make web-check`** plus **`make routes`**, which need
  `gui/node_modules`. A fresh worktree has none, so **run `pnpm install` in the
  worktree's `gui/` as part of cutting it**, or the worker will print `SKIPPED:`
  and pass unrun. This is the inverse of the last wave's trap and it bites
  harder here.
- **The orchestrator runs the full `make ci` from the main checkout after each
  merge.** That is the only place the whole gate is real.
- **APFS-clone `target/` into each worktree** (`cp -Rc target <wt>/target`, then
  `rm -rf <wt>/target/debug/incremental`). Measured: ~100 seconds and **zero**
  net disk, because a clone shares blocks.
- **Remove the worktrees when the wave closes.** The permission classifier
  declines `git worktree remove` from the agent, so it is the user's command to
  run — hand it to them rather than leaving 100G+ behind.
- **Never read a piped report, only a piped exit code.** `make test | tail -25`
  reports *tail's* status, and `git merge | tail -3` once hid four `CONFLICT`
  lines above the one it showed.
- **Two threads appending to one file can merge cleanly into nonsense.** Git
  aligns a shared tail, so two similar-shaped blocks get interleaved around it
  and deleting the markers **compiles**. When both sides only appended, rebuild
  the file from each side's own block (`git show <side>:<path>`, sliced past the
  merge-base's line count). Regenerate generated files rather than merging them.
- **A subagent with no `SendMessage` reports to the orchestrator, not to the
  worker that spawned it.** A worker's `cargo-tester`/`web-checker` result may
  arrive at your address, leaving the worker completed-but-unfinished with
  nothing looking wrong. If a thread goes quiet after its tests would have
  finished, relay it.
- **Ask for push-back by name in every prompt file.** Six specified points were
  overturned by workers last wave and every one of them was right.

## The agents, and the one that can see

- **`api-surface-auditor` first**, per item, before a line of Svelte. Every
  request this wave needs exists — but the auditor is what turns *"I'll just add
  a field above the seam"* into an engine item, and a GUI wave is exactly when
  that temptation arrives.
- **`web-checker`** after touching anything under `gui/`.
- **`screenshot-reviewer`** before calling any screen done, and it is not
  optional here. It is the only check in this repo that can **see**, and last
  wave it caught a band heading that broke the axiom word-for-word, a promotional-
  banner grammar, a 3.88:1 contrast failure on the one string that had to read as
  an absence, and a month table with no columns. Every one of those passed every
  assertion.
- **`gui-component`** skill for any new component or route, so twelve sessions
  produce one dialect.

## The axiom, in the three places this wave can break it

`docs/decisions.md` and `docs/ux-positioning.md` are the authority; these are the
edges this particular wave runs along.

- **No task-completion framing.** No badge counting cards not yet made, no
  "unread", no *yet* — that last word turns an absence into something
  outstanding, and it was cut from two empty states last wave for exactly that.
- **A number on the home surface may describe one book, never the collection.**
  The shelf ships `Reading · 35%` under a tile and that is the settled reading of
  the rule. A wall of cards is not the home surface, but a count of them near one
  is the collection.
- **Idle is not blank, and nothing is a dead end.** An empty wall says what a
  card is and links to a book; it does **not** name a CLI command. A GUI empty
  state's audience is a reader with no terminal in the window — the library's
  failure state may say `make dev-db`, because its audience is whoever mis-set
  the data dir. An ordinary empty state may not.

## The two guaranteed conflicts

- **`docs/decisions.md`.** Every merge conflicts there. Tell every worker to
  **append** and restructure nothing; the file is in build order, not numeric
  order. Resolution is deleting three marker lines — unless both sides appended
  similar-shaped blocks, in which case see the merge-splice trap above.
- **`gui/src/lib/api/bindings.ts`** if any thread touches a DTO. It is generated:
  regenerate with `make ts`, never resolve it as text. Nothing in this wave
  should touch a DTO at all — if a thread needs to, that is an engine item and a
  conversation, not a frontend workaround.

## What "done" looks like for the wave

- Items 47, 48 and 49 merged one at a time, `make ci` exit 0 on `main` after
  each.
- **Three requests that had no caller now have one**, and the two texts that
  told the frontend not to build the citations N+1 have been updated to say it
  is built.
- `/book/[id]/cards` no longer makes N `CardPassage` calls.
- `docs/decisions.md` carries three appended entries, each recording **the
  corrections building it forced** — not a summary of what was built. That
  paragraph is the most valuable thing an item produces and it is the one that
  gets skipped when the tests go green.
- `make shots` run and the PNGs **looked at**, at all three viewports, by
  `screenshot-reviewer` and by you.
- A session log via `wrap-session`, and **this file rewritten** rather than
  amended — a handoff describing the wave that just happened as the wave still to
  come is the specific failure the last two rewrites fixed.
