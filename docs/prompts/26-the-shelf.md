# Item 26 — the shelf

**Landed.** No migration. GUI only: `gui/src/lib/shelf/`, `gui/src/lib/accent.ts`,
`gui/src/lib/components/BookTile.svelte`, `gui/src/routes/+page.svelte`, plus
`currentlyReading` on the client seam and its fake.

Written after the fact rather than before it, which `docs/next-thread-handoff.md`
already notes is the shape any item minted from a conversation has. The rulings
are in `docs/decisions.md` entry 26; this file is the brief they came from.

## What was asked for, and what changed

The spec (`docs/gui/spec-gui-17-28.md:474`) asked for a **WebGL spine shelf**:
spine-out, thickness from item 19, colour from item 20b, selection sliding a book
out and turning it cover-forward.

The user ruled the 3D renderer **cosmetic and deferrable**, and separately that
the shelf need not be the centrepiece of the app — the ask became "make the grid
look good, and keep its layout isolated so it can be toggled and other layouts
added later."

That second clause is the item. A deferral is cheap; a deferral that leaves the
next person unpicking a welded-in grid is not.

## The three things a later thread should not re-open

1. **The layout registry is the deliverable.** `layouts.ts` is the only file that
   knows what arrangements exist. Two ship, because a seam with one
   implementation is a guess about what varies. The spine shelf is a third entry.
2. **`currently_reading` is a request.** Not `books.filter(b => b.reading_state
   === 'reading')`. It returns the reading, which a book row cannot — and item 28
   needs exactly that.
3. **`src/lib/accent.ts`'s constants are this renderer's**, and must not be
   unified with `render3d/texture.rs`'s. Two renderers with different known
   backgrounds owe different bands. See entry 26.

## What was found on the way

- The GUI had **no luma policy at all** while the engine deliberately stores
  `cover_accent` unclamped. Found by looking at a screenshot, not by reading
  code, and only visible because `fake.ts` invents hostile full-gamut accents —
  against a library of real jackets it would have looked fine indefinitely.
- `BookTile`'s comment claimed *"nothing stores cover dimensions yet (item
  20b)"*. Item 20 had landed. A stale comment in the one file that most needed
  the field.

## Left open

- **Duplication between the bands.** A book being read appears in the Reading
  strip and again on the shelf below. Argued as correct — the shelf is your whole
  library and a book you are reading is still in it, and excluding it would make
  books appear and vanish from the shelf as you start and stop them. Flagged for
  the user rather than settled by a thread.
- **The Reading strip uses the same tile at a larger size.** Whether that reads
  as *pulled proud* or as a second grid is a design question item 27 will be in a
  better position to answer, once a second surface exists to be consistent with.
