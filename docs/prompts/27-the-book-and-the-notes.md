# Item 27 — the book, and the notes

**Landed.** No migration, no engine or API change. GUI only:
`gui/src/lib/book/` (four components, two modules, two suites),
`gui/src/lib/components/Jacket.svelte`, `gui/src/routes/book/[id]/+page.svelte`,
plus fifteen methods on the client seam and the fixtures behind them.

The rulings are in `docs/decisions.md` entry 27; this file is the brief they came
from, and the two things it refused.

## What was asked for

`docs/gui/spec-gui-17-28.md:490`: *the book view — info, notes, highlights, and
the links pane*, with `crates/tui/src/ui/book.rs` as the reference for what
belongs on it. Two things the TUI never built and this had to:

- **Note search.** Out of scope on arrival — see below.
- **Citations.** `Cite` / `Uncite` / `CitationsFor` have existed since item 7
  with `rb cite` as their only surface.

## The two refusals

**Note search was excluded before the item started, and correctly.**
`SearchMarks` takes no `book_id`, and its `limit` applies to the **global**
ranked list — so narrowing its results to one book above the seam returns
nothing whenever the top hits live in other books. A box that looks like it
works and is silently wrong is worse than no box, and item 40 is the engine
change. `NotePane.svelte` carries a comment where it goes and no input.

**The read-number gutter was excluded and stays excluded.** `ReadNumbering` is
the engine's (item 17c) and crosses no DTO; `readings.indexOf(id) + 1` in
TypeScript would re-implement a domain rule *and* silently depend on
`list_readings`' ordering contract — which is the exact failure item 17c moved
it down to prevent, with nothing on either screen looking wrong.

## The three things a later thread should not re-open

1. **The pane has three depths and no dialog.** List → one note → that note's
   links, each replacing the last in place with the way back on screen. This is
   the TUI's own arrangement and it is the axiom's *nothing is modal-by-default*
   doing real work rather than being agreed with.
2. **A cited mark on a passage is per *open* note, and that is not the N+1.**
   `CitationsFor` is one call for the note the pane has open. Marking which
   passages *any* note cites needs one call per note in the book, has no request
   behind it, and is a later item.
3. **`heroSrc` is `cover_path` and `coverSrc` is `cover_shelf_path`.** Two
   methods rather than a flag, so a call site says which it meant. A grid must
   not fetch sixty hero shots and a detail view must not show a thumbnail.

## What was found on the way

- **Three `as` casts in `fake.ts` were hiding live drift**, in both directions
  the file's header claims `tsc` catches. `highlight()` stated `pos0`, `pos1`
  and `identity_hash`, none of which are on `HighlightDto`; the note literal
  stated a `last_modified` `NoteDto` does not have; and `reading()` **omitted
  `progress`**, which item 22 added — so every per-reading progress line in the
  app would have rendered `undefined` and no test could have said so. Item 38
  removed the cast from `book()` for exactly this reason and the other three
  survived it.
- **`Engine::open_reflection_record` and `open_review_record` are not on the
  wire.** They exist because *"everything that then edits the note wants a
  `NoteRecord`"*, which is precisely what a GUI that opens the reflection needs;
  the API exposes only the `CreatedNote` half, so the screen pays a `get_note`
  after every open. One extra request rather than a re-derivation — the right
  side of the seam, and a cheap additive fix for whoever owns `crates/api`.
- **A top-level `const state` in a rune file breaks `svelte-check`**, reporting
  two dozen errors on the *other* lines about `$state` being used before its
  declaration. `BookTile` gets away with the name only because it declares no
  `$state`.
- **`toContainText` on a `<textarea>` passes on an empty box.** The text node is
  the initial markup and `bind:value` sets the property, so a body assertion
  written that way asserts nothing. `toHaveValue`.

## Left open

- **A per-book `SearchMarks`** — item 40, in flight.
- **A cited mark that does not depend on which note is open**, which is a
  request shape (`citations_for_book`, or a count on `HighlightDto`) rather than
  a frontend problem.
- **The read-number gutter**, once `ReadNumbering` crosses.
- **Creating a flashcard.** `Storage::insert_flashcard` has no `Engine` wrapper,
  so there is no way to make one from any frontend; `list_flashcards_for_book`
  is served and deliberately not surfaced here, because a band whose contents
  can only arrive from a KOReader import is a fourth band saying nothing on
  every book that has not had one.
- **Eight calls for one book.** Still no request that returns a book with its
  children; item 18 answered the list half with `BookSummaries` and the detail
  half is open. Recorded rather than worked around, so the next audit sees it.
