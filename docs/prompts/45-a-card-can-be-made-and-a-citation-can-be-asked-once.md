# Prompt — Items 45 and 46: a flashcard can be made, and citations asked once

Engine + API only. Runs in a worktree, in parallel with the item 43/41 thread —
**touch nothing under `gui/` except the regenerated `bindings.ts`**, and touch
nothing in `crates/engine/src/storage/readings.rs` or `query.rs`.

## Before you write a line

```
git log --oneline -1                    # must be the tip of main
ls crates/engine/migrations/ | tail -2  # must end at 0017_moments.sql
```

If either is wrong, `git reset --hard main`. Four of six worktrees last wave were
cut ~80 commits behind and one nearly wrote a migration into a five-version gap.

Read the root `CLAUDE.md`, `crates/engine/src/storage/CLAUDE.md`,
`crates/api/CLAUDE.md`, and `docs/decisions.md` entry **18** (list endpoints, and
the anti-N+1 rule item 46 exists to satisfy).

**No migration.** `flashcards` has carried `book_id` and `highlight_id` since
`0001_init.sql`; `citations` has `PRIMARY KEY (note_id, highlight_id)`, whose
leading column is what a batch query keys on. Migration `0018` belongs to the
other thread — do not write one.

## What

### Item 45 — a flashcard can be made

The gap is structural and starts one layer below the wire. `flashcards` has had
`book_id` and `highlight_id` since the first migration; `list_flashcards`'
**SQL never selects them**, so `FlashcardRow` has no fields for them and
`FlashcardDto` cannot carry them. A card therefore cannot be shown beside the
passage it came from — it knows only its book's *title*.

And `Storage::insert_flashcard` has no `Engine` wrapper and no request. Its only
production caller in the whole repo is the KOReader import's auto-capture of
single-word highlights (`koreader.rs:1024`); everything else calling it is a
test fixture, one of them reaching through the `internals` feature. So a card
can be *minted by an import* and by nothing else.

Close both halves:

1. `book_id` and `highlight_id` selected, carried on `FlashcardRow`, and on
   `FlashcardDto` as `#[serde(default)]` additions.
2. An `Engine::create_flashcard` wrapper and one request behind it, following
   `SetAnnotation` (`protocol.rs:236` → `lib.rs:925` → `engine/lib.rs:886` →
   `storage/highlights.rs:328`) — the crate's template for a single-row write.

`insert_flashcard` returns `bool` because `UNIQUE(book_id, word)` dedupes. That
answer must survive to the wire: *"you already had this card"* and *"a card now
exists"* are different facts and a caller drawing a confirmation needs to tell
them apart. Prefer an existing `Response` shape over minting a new one.

### Item 46 — which passages are already cited

`CitationsFor { note_id }` is one call per note. Nothing loops it yet, and that
is deliberate — `gui/CLAUDE.md` says outright *"Marking which passages any note
cites is one call per note in the book and is a later item — do not build the
N+1"*, and `gui/src/lib/book/Passages.svelte:14-27` records the same refusal in
its module doc. This item is what makes that mark buildable.

Copy `book_summaries` (`crates/engine/src/storage/books.rs:1265`) exactly — it
is this repo's one precedent for binding a variable-length id list, and it
carries two decisions worth inheriting:

- **chunked at 500**, because SQLite's parameter ceiling is 999 on older builds
  and *"a limit stated in a doc comment is a limit somebody exceeds"*;
- **one reply row per requested id, in the order asked, empties included** — so
  a caller zips it against the page it already has, and *"nothing behind this
  one"* is an answer rather than a missing row.

## Two decisions this item has to make

State your answer and the argument in `docs/decisions.md`. **Push back rather
than comply** if the reasoning below is wrong — four of five threads did last
wave and every one of them was right.

1. **What does the batch return — highlights, or highlight ids?** The recommended
   answer is **ids**, and the argument is that the page asking this question
   already holds the highlights: `gui/src/routes/book/[id]/+page.svelte:113`
   loads `listHighlights(bookId)` in the same `Promise.all` as `listNotes`. A
   `Vec<HighlightDto>` per note re-sends the reader's private highlight **text**
   once per citing note, for a screen that only wants to draw a mark. The
   single-note `CitationsFor` stays exactly as it is — it feeds a pane that
   shows the passages themselves. If you disagree, say why; a shape that made
   the single-note call redundant would be a better answer than two shapes, but
   only if it does not put the text on the wire N times.

2. **Does `CreateFlashcard` validate the highlight against the book?** A card
   carries `book_id` and an optional `highlight_id`, and nothing stops a caller
   passing a highlight belonging to a different book — after which the card sits
   beside a passage from somewhere else. `crates/api/CLAUDE.md`'s rule is
   *"handles do not cross"*: the write path takes ids and re-reads server-side
   rather than trusting a client. Decide whether that check belongs here, and if
   you refuse it, say what makes the pairing safe without it.

## Must not

- **Touch `crates/engine/src/storage/readings.rs` or `query.rs`** — the other
  thread owns both.
- Write a migration.
- Build the GUI mark, or any screen. This item mints the request; the surface
  that draws it is a later item, and `gui/CLAUDE.md`'s standing instruction not
  to build the N+1 stays true until that item is taken.
- Change `citations_for`'s existing behaviour or its ordering
  (`ORDER BY h.page ASC, h.ko_datetime ASC` — the order a book reads).
- Move `API_VERSION`. Everything here is additive.
- **Change the shape of an existing `Request` variant if a new one will do.**
  `ts-rs` emits a new field as **required** in TypeScript however
  `#[serde(default)]` the Rust is, so a field added to an existing request
  breaks `gui/src/lib/api/client.ts` — and your own gate cannot see it, because
  a fresh worktree has no `gui/node_modules`. If you must change one, **say so
  loudly in your report**.

## Done when

- The batch and the single-note call **cannot disagree**: a property or a test
  asserting that for any set of note ids, the batch's answer for each equals
  what `citations_for` returns for that note alone. That agreement is the only
  thing making the batch safe to prefer.
- The batch is asserted to return **one row per requested id, in the order
  asked**, including for ids that cite nothing and for ids that do not exist.
- Chunking is asserted with more than 500 ids, not documented. `book_summaries`
  chunks and nothing proves it does.
- Creating the same `(book_id, word)` twice reports the second as **not new**,
  and the first card's `context`/`highlight_id` are not silently rewritten by
  the second attempt.
- A flashcard round-trips its `book_id`/`highlight_id` from `create` to `list`
  to the DTO — the whole point is that a card can be shown beside its passage.
- A payload written before this wave still parses into every request you touched.
- `make ts` run and `bindings.ts` committed.

## Files

`crates/engine/src/storage/flashcards.rs`,
`crates/engine/src/storage/notes.rs`, `crates/engine/src/lib.rs`,
`crates/api/src/{dto,protocol,lib}.rs`, any CLI/TUI call sites the signatures
break, and `gui/src/lib/api/bindings.ts` (generated only).

Note that `FlashcardRow` is re-exported from `crates/engine/src/lib.rs:89` and
rendered by `crates/tui/src/ui/book.rs:464` — adding fields to it is a
workspace-wide change, and `cargo check --workspace` is the build that says so.

## How it is gated

**`make fmt lint build-check test ts-check`** — not `make ci`. A worktree has no
`gui/node_modules`, so `web-check` and `routes` print `SKIPPED:` and would pass
unrun. The orchestrator runs the full `make ci` from the main checkout after the
merge.

Run the `cargo-tester` agent before you call it done.

## `docs/decisions.md`

**Append** two entries, 45 and 46, in build order. Restructure nothing: the file
is in build order rather than numeric order and it is the guaranteed conflict of
every merge. Each entry records **the corrections building it forced** — that
paragraph is the most valuable thing the item produces and it is the one that
gets skipped when the tests go green.

## Report back

What you overturned, what the two decisions above resolved to, and whether you
changed the shape of any existing `Request`.
