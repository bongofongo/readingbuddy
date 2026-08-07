# Prompt — Item 47: the wall of cards

GUI only. Runs in a worktree, **alone** — nothing else in this wave runs beside
it, because this item may reshape `gui/src/lib/card/` and the other thread lives
in `gui/src/lib/book/` underneath that.

No engine change, no API change, no migration, and **`API_VERSION` stays 2**.
Every request this item needs already exists. If you become convinced one does
not, that is a conversation with the orchestrator and an engine item — never a
field added above the seam and never a second call in a loop.

## Before you write a line

```
git log --oneline -1                    # must be the tip of main
ls crates/engine/migrations/ | tail -2  # must end at 0018_reading_sort_indexes.sql
ls gui/node_modules >/dev/null && echo deps-ok
```

If the first two are wrong, `git reset --hard main`. Four of six worktrees in the
GUI wave were cut ~80 commits behind. If `deps-ok` does not print, run
`pnpm install` in `gui/` **before** anything else: without it `make web-check`
and `make routes` print `SKIPPED:` and you will pass unrun, which is the worst
way for a check to stop working.

Read `gui/CLAUDE.md` in full, then `docs/gui/gui-vision.md` (especially the year
filter at `:151` and the card at `:112`), `docs/gui/testing.md`, and
`docs/decisions.md` entries **43** (the row you are drawing — it states what this
must *not* become), **41** (the read number), **44** (the passage, and why one
call per card is right for a book and wrong for a wall), **18** (why the count is
its own request), **28** (the card as it shipped) and **17** (derived facts).

## The route decision, already settled

The user has ruled: **a new `/cards` route, and `/book/[id]/cards` stays.** The
two screens answer different questions — *my reading life* and *this book's
reads* — and a book's card page reached from the book is not a filtered view of
a library wall in the reader's head, whatever it is in the query.

Do not delete `/book/[id]/cards`. Do not put the wall on `/life`.

## What

### 1. `/cards` — the wall

A page of cards over `ListReadingRows`, with `CountReadings` beside it.

- **One row is one card's facts.** `ReadingRowDto` carries `book`, `reading`,
  `read_number`, `of_reads` and `passage`. That is one round trip for a page.
- **The read number, at last.** `read_number` and `of_reads` are `i64`, neither
  an `Option` — item 41 states why. *Your second read* is the test
  `of_reads > 1`, which is the same test the TUI's gutter makes. **Never**
  `readings.indexOf(id) + 1`: that re-acquires a dependency on an ordering the
  wire does not state, and nothing on the screen would look wrong.
  `Card.svelte`'s header currently says the card carries no ordinal *because the
  request did not exist*. It exists. Update that comment rather than leaving a
  stale prohibition — a stale prohibition is worse than none, because the next
  thread obeys it.
- **The year filter** (`gui-vision.md:151`) through
  `ReadingFilterDto.finished_in`, which is a `DayRangeDto` with two **required**
  day strings. It is **fallible at both doors**: an inverted span is
  `InvalidInput` and never a confident empty answer. A year picker cannot
  produce an inverted span, which is exactly the point — **do not carry a second
  validation dialect in TypeScript.** Build the span from the year, send it, and
  let the one refusal live where it already lives.
- **The count is its own request, asked once per filter** — not once per scroll.
  Item 18's ruling and this is the case it was made for.
- **Paging is an offset**, not a cursor. `limit` is required and **negative
  means no limit**; `0` is a real limit meaning a page of nothing, which is why
  the field has no serde default. A wall sends a real page size.
- **Three sorts**: `finished` (default — most recently finished first, open
  readings last), `started`, `last_modified`. All three are indexed by `0018`.
  **There is deliberately no title sort** and you must not add one above the
  seam: it would order by a `books` column no index on `readings` can serve, and
  sorting is the engine's anyway (item 17).

### 2. `/book/[id]/cards` moves onto the same call

Not a nice-to-have — this is the item retiring a live N+1. That page today calls
`listReadings` and then `Card.svelte` makes **four** calls per card. Point it at
`ListReadingRows` with `ReadingFilterDto.book_id` set. Item 43 was built so one
query serves both surfaces; that is stated in `ReadingFilterDto.book_id`'s own
doc comment.

The page keeps its route, its heading, its empty state and its side-by-side
track. Only where the facts come from changes.

### 3. The thing to decide, and I want your reading of it

`Card.svelte` currently fetches **four** things per card in an `$effect`:
`cardPassage`, `highlightsForReading`, `notesForReading` and `noteForReading`
(plus `reviewRating` when the last finds a review). The row kills the first.
**The other three are still one-per-card**, so a page of 24 cards is 72–96
requests and a wall of 400 is unaffordable.

Two shapes, and you are asked to argue for one rather than assume:

- **Hand the card what the row already knows, and make the rest opt-in.** `Card`
  takes `passage`, `read_number`/`of_reads` as props; the marks/notes/rating
  fetch sits behind a prop the per-book page sets and the wall does not. The
  wall's card is then the row and nothing else. Costs: two card densities to
  look at, and `screenshot-reviewer` has to see both.
- **Let paging bound it.** The card keeps its own fetch, and the page size is
  what makes it affordable. Simplest, one card everywhere, and honest about the
  cost — but a page of 24 is still ~72 requests over a unix socket for one
  screen, and "it is bounded" is the argument the N+1 always makes.

I lean toward the first. **Push back if you disagree** — six specified points
were overturned by workers last wave and every one of them was right. Whichever
you take, say in `docs/decisions.md` which and why, with the number of calls per
page written down.

## Must not

- **Do not invent a `CardDto`.** Item 43's entry states the test: *a card would
  grow the rating; this row will not*. A card is a **layout**, and a layout is
  the frontend's composition of facts the API already serves.
- **Do not fetch highlight text you were handed.** `passage` is on the row.
- **Do not divide `current_page` by `page_count`** — one book in `make dev-db`
  has a `page_count` of 0 and another has `NULL`, and `percent` is an integer
  division the float cannot reproduce. `reading.progress` is this read's;
  `book.progress` is the **current** read's and on a reread they differ, which
  is what item 22 caught a frontend getting wrong.
- **Do not rebuild `series_label` or join `authors`** — `authors_display` is the
  parse, `authors` is the record.
- **Do not touch `gui/src/lib/book/`.** Thread B is there.
- **Do not hand-edit `gui/src/lib/api/bindings.ts`.** Nothing here should touch a
  DTO at all.

## The axiom, on this screen specifically

`docs/decisions.md` and `docs/ux-positioning.md` are the authority. Three edges
this screen runs along:

- **No task-completion framing.** No badge counting cards not yet made, no
  "unread", and **no *yet*** — that word turns an absence into something
  outstanding and was cut from two empty states last wave for exactly that.
- **A number may describe one book, never the collection — on a home surface.**
  `/cards` is not the home surface, so a count of the readings a filter matched
  is legitimate *there* and would not be under a tile on `/`. Do not put one on
  `/`. Say plainly in your entry where you decided the line is.
- **Abandoning a book is not failure.** `readingStateLabel` says "Put down". No
  *fail* / *did not finish* / *DNF* / *gave up*, and no failure styling.
- **Idle is not blank, and nothing is a dead end.** An empty wall says what a
  card is and links to a book. It does **not** name a CLI command: this screen's
  audience is a reader with no terminal in the window. (The *library's failure*
  state may say `make dev-db`, because its audience is whoever mis-set the data
  dir. An ordinary empty state may not.) Every screen needs at least one link
  out — `tests/routes.spec.ts` asserts it.

## Agents

- **`api-surface-auditor` first**, before a line of Svelte. Every request exists,
  but the auditor is what turns *"I'll just add a field above the seam"* into an
  engine item, and a GUI wave is when that temptation arrives.
- **`gui-component` skill** for the new route and any new component, so this
  session produces the one dialect.
- **`web-checker`** after touching anything under `gui/`.
- **`screenshot-reviewer`** before calling the screen done, and it is **not
  optional**. It is the only check in this repo that can see. Last wave it caught
  a band heading that broke the axiom word-for-word, a 3.88:1 contrast failure on
  the one string that had to read as an absence, and a month table with no
  columns. Every one of those passed every assertion.

## Svelte 5, runes only

`export let` / `$:` / `writable()` / `createEventDispatcher` / `<slot />` are all
banned and `eslint.config.js` fires on every one of them. This is the most likely
defect in agent-written code here, because the training mass is Svelte 4.

## The client, and the fake

`gui/src/lib/api/client.ts` has **no** `listReadingRows` and no `countReadings`.
Add them to the `LibraryClient` interface, to `TauriClient`, and to `FakeClient`
in `gui/src/lib/api/fake.ts`.

The fake is the frontend's fixture and its books are **the hostile set on
purpose** — a null title, a 220-character one, RTL, CJK, no author, a
`page_count` of zero, an abandoned reading, a reread. `crates/corpus/edge-cases.json`
declares that set once and both fixtures are asserted against it
(`src/lib/api/fake.test.ts` from this side). **No `as` cast in `book()`** — the
file's header claims a drifted DTO field is a `tsc` error here, and a cast makes
that true for renamed fields and false for added ones.

Your fake `listReadingRows` must honour the filter, the sort and the offset, and
`countReadings` must agree with it under the same filter. A fake that ignores the
filter makes the year picker untestable at layers 1 and 2, which is where it is
tested.

## Your gate

```
make web-check     # svelte-check + tsc + eslint + vitest + build
make routes        # every route, three viewports, WebKit — fails on a diff
make shots         # then LOOK at the PNGs, and have screenshot-reviewer look
```

Add the new route to `ROUTES` in `gui/tests/routes.spec.ts` — the file says *"Add
one here when you add one there."* Add the cases that would fail: a wall with a
reread in it (`of_reads > 1`), a filter matching nothing, and the per-book page
still rendering after its rewiring.

**Never read a piped report, only a piped exit code.** `make routes | tail -25`
reports *tail's* status.

Also run `make fmt lint build-check test ts-check` — cheap, and it catches a
`bindings.ts` you touched by accident.

## When you are done

Append an entry to `docs/decisions.md`. **Append; restructure nothing** — the
file is in build order, not numeric order, and every merge conflicts there.

The entry records **the corrections building it forced**, not a summary of what
was built. That paragraph is the most valuable thing an item produces and it is
the one that gets skipped when the tests go green. Name the call-count decision
from §3 and the number you settled on.

Then report to the orchestrator: what you built, what you overturned, what you
left, and whether anything you did could break another thread. **If you spawned a
`web-checker` or `screenshot-reviewer` and it went quiet, say so** — a subagent
with no `SendMessage` reports to the orchestrator rather than to you, and a
worker can sit completed-but-unfinished with nothing looking wrong.
