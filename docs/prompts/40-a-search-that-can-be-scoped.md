# Prompt — Item 40: a search that can be scoped, and a note list that can be

Engine + API only. Runs in a worktree, in parallel with a `gui/` thread — **touch
nothing under `gui/` except the regenerated `bindings.ts`**.

Read the root `CLAUDE.md`, `crates/engine/src/storage/CLAUDE.md`, and
`docs/decisions.md` entries **34** (one search surface — the item this extends)
and **18** (list endpoints).

**No migration.** Every column this needs already exists.

## What

1. **`book_id: Option<i64>` on the search**, threaded `Storage::search_marks` →
   `Engine::search_marks` → `Api::search_marks` → `Request::SearchMarks`.
   Applied as a predicate **on both index queries, before the limit and before
   the position merge**. `None` is the whole library, exactly as today.
2. **A reading scope on the note list**, on `Engine::list_notes` and
   `Request::ListNotes`. `NoteDto` carries `reading_id` and a client is one
   filter away from doing it above the seam, which is the same truncation bug in
   a second place. Item 28's card is per *reading* and needs the notes of one.

## Why it bites now

An API audit found the GUI's book view cannot have a note-and-highlight search
box. `SearchMarks { query, source, limit }` searches the whole library, and
`limit` applies to the **global** ranked list — so filtering to one book above
the seam returns **zero rows** whenever the top `limit` hits live in other books.
`SearchHit::book_id()` already exists; it is just not a predicate.

## Must not

- Change how the merge orders things. Item 34 established that bm25 cannot rank a
  note against a highlight; the list is merged by within-source position with
  recency as the tie-break. This item narrows *what is in* the two lists, which
  is `SearchSourceDto`'s own idiom.
- Write a migration.
- Move `API_VERSION`. Both new params are `#[serde(default)]`, so this is
  additive; a request without the field must still parse, and that must be
  asserted.

## Done when

- The **truncation bug** has a behavioural test that can fail: enough marks
  seeded that the wanted book's hits rank *below* a small limit, and the scoped
  search still finds them. Three books and a limit of fifty proves nothing.
- The invariants that are actually true are asserted as properties, and the ones
  that are **not** true are stated rather than asserted. (See the correction
  below — the obvious one is false.)
- `make ts` run and `bindings.ts` committed.

## The correction this item forced

"Scoping to a book returns a subset of the unscoped result, **in the same
relative order**" was proposed as the property. It is **false across sources**
and the counterexample is small: notes `[n1(A), n2(A), n3(B)]` and highlights
`[h1(B)]` put `n3` at within-source position 2 and `h1` at position 0 unscoped,
so the filtered-to-B reading is `[h1, n3]`; scoped, both sit at position 0 and
the recency tie-break decides, which can be `[n3, h1]`. What *is* true is
membership, and within-source order. Assert those two and say why the third is
not there.

## Files

`crates/engine/src/storage/fts.rs`, `crates/engine/src/storage/notes.rs`,
`crates/engine/src/lib.rs`, `crates/api/src/lib.rs`,
`crates/api/src/protocol.rs`, the CLI/TUI call sites, and
`gui/src/lib/api/bindings.ts` (generated only).

## How it is gated

**`make fmt lint build-check test ts-check`** — not `make ci`. A worktree has no
`gui/node_modules`, so `web-check` and `routes` print `SKIPPED:` and would pass
unrun.

## `docs/decisions.md`

**Append.** The file is in build order, not numeric order, and every merge this
wave conflicted there.
