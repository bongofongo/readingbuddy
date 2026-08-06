# Prompt — Item 33: one search surface

Paste into a fresh session at the repo root, on branch `feat/engine-search-surface`,
branched from `main` at the head of the 2026-08-06 non-GUI wave.

---

Read `docs/decisions.md` (the entries for items 17 and 18 — item 18 settled how
every list in this app paginates, and this item is a list),
`crates/engine/src/storage/CLAUDE.md`, and `crates/api/CLAUDE.md`.
`CLAUDE.md`'s **Engine standards** section is binding, and
`crates/engine/migrations/CLAUDE.md` **before you write the migration**.

**Owns migration `0015`.** Engine + API + one CLI door. No TUI, no GUI beyond
the regenerated bindings.

## What

A `highlights` FTS index, and **one** request that answers notes and highlights
together, plus `title` as a predicate on `BookFilter`.

## Why one request, and why the title filter lives here

Two lists a frontend interleaves is a relevance ordering invented above the
seam — the frontend has no way to say a note scoring 0.9 outranks a highlight
scoring 0.7, so it will interleave by whatever it has, which is source order.
Rank once, below the seam, and hand back one ordered list.

And a title search is **not a seventh endpoint**. Item 18 already made every
list a filter + sort + page over one shared clause; `BookFilter` holds
`author` / `year` / `language` / `tag` / `has_cover` and `title` joins them.
Read the doc comments on those fields (`crates/engine/src/storage/query.rs:92`)
before you write yours — each one states exactly how it matches and why, and
yours must do the same. Note `like_escape` immediately below them: LIKE's own
wildcards are escaped so a search for `100%` is not a search for everything.

## The FTS decision: triggers or an explicit writer

`notes_fts` (`0001_init.sql:77`) has **no triggers** — the engine populates it
in application code (`storage/notes.rs:137`, `:141`, `:207`, `:331`), and
`crates/corpus/src/devdb.rs:184` writes the FTS row itself for that reason,
with a comment saying so.

Highlights arrive **in bulk from importers**, which is the difference: KOReader,
calibre and Goodreads all insert highlights, and a hand-written writer has to be
reached by every one of those paths. Whichever you choose, state the argument.
The failure mode is not subtle: a highlight that is in `highlights` and not in
the index is invisible to the one thing item 27 exists to do, and nothing will
tell you.

If you choose triggers, say what that means for `merge_books`' highlight moves
and for the `identity_hash` conflict path, both of which already move rows.
If you choose a writer, say which paths you audited and how a future insert path
is stopped from skipping it.

## `search_notes` on the wire

Nothing consumes it. `crates/cli/src/commands/note.rs:101` is the only caller
outside tests; `gui/src` has no reference (only the generated `bindings.ts`
mentions it). `crates/api/src/protocol.rs:42` holds `API_VERSION: u32 = 1`.

Adding a method does not bump `API_VERSION`; changing or removing one does.

**Recommendation, which you may refuse:** replace `search_notes` with the
unified request and bump `API_VERSION` to 2. It is free now and never again,
and leaving both doors up is the interleaving problem with extra steps. If you
keep both, defend it — and say what a frontend is supposed to do with two.

## The privacy rule is not negotiable

Highlight text, note bodies and **search queries** are the user's private
reading: never above `trace!`. A new query path is exactly where a helpful
`info!("searching for {q}")` gets added. `tests/tracing_redaction.rs` is the
one test that asserts on log output; everything else asserts on `Diagnostic`s.

## Absence is not zero

A search with no hits is not an error and not a zero-with-a-badge. `BookFilter`'s
`None` means *not asking*, `Some("")` is a decision you have to make. Say what
an empty query does before a caller discovers it.

## Files

- `crates/engine/migrations/0015_*.sql` — **read `migrations/CLAUDE.md` first**
- `crates/engine/src/storage/highlights.rs` (the writer)
- `crates/engine/src/storage/notes.rs` — `search_notes`, `NoteSearchHit`, and
  the `snippet(notes_fts, 1, '>>', '<<', '…', 12)` shape at `:420` your unified
  hit must match or deliberately diverge from
- `crates/engine/src/storage/query.rs` — `BookFilter`
- `crates/api/src/dto.rs`, `crates/api/src/protocol.rs`, `crates/api/src/lib.rs`
- `gui/src/lib/api/bindings.ts` — **generated. Never hand-edit. `make ts`.**
- A CLI door. `rb notes --search` exists; the unified one needs one too, or the
  surface is unreachable from a terminal the way `measure_stored_covers` was for
  a whole wave. `crates/cli/src/commands/covers.rs` (78 lines) is the shape of a
  door that costs nothing.

**Collides with:** item 35 (api + bindings) and nothing else in round 1. Item 35
runs **after** you merge, on a base that contains you, so you do not coordinate
with it.

## Done when

- A highlight imported by the KOReader path is findable.
- A note and a highlight come back in **one** ordered list, with snippets.
- `BookFilter { title }` composes with the existing filters, with the offset
  paging, and with `count_books`.
- `make ts` regenerated and committed; `make ts-check` clean.
- `docs/decisions.md` **appended** — see the conflict note below.

## Must not

- Add a seventh endpoint for titles.
- Return two lists.
- Introduce a second pagination shape. Item 18 settled on offset everywhere; the
  two sorts with no cursor key are exactly the two whose pages are whole-table
  reads, and a cursor would not fix them.
- Log a query, a note body or a highlight above `trace!`.

## How you are gated

**Not `make ci`** — a fresh worktree has no `gui/node_modules`, so `web-check`
and `routes` print `SKIPPED:` and you would "pass" them without running them.

Run **`make fmt lint build-check test ts-check`**, and read the exit code
properly: never `make test | tail -25`, which reports *tail's* status. Redirect
to a file and read `$?`.

The orchestrator runs the full `make ci` from the main checkout after merging
you.

## The one guaranteed conflict

`docs/decisions.md` — **append** your entry and restructure nothing. Three of
four merges last wave conflicted there and nowhere else; the resolution is then
deleting three marker lines. The file is in **build order, not numeric order**,
deliberately.

## Report the corrections this forced

In the shape `docs/decisions.md`'s existing entries use.

**Push back rather than comply.** Three places this prompt may be wrong: the
`API_VERSION` bump, whether one ranked list can honestly rank a note title
against a highlight body at all (they are different kinds of text and fts5's
bm25 does not know that), and whether `title` on `BookFilter` should be FTS or
LIKE — the other five predicates are LIKE, and matching them may matter more
than matching the notes index.

> **Note on `cargo-tester`.** If you are a subagent you cannot launch it —
> subagents cannot spawn subagents. Run its procedure directly:
> `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
> `cargo test --workspace`. Say which you ran.
