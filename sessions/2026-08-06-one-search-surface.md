# 2026-08-06 — one search surface

Item 34 of the non-GUI wave, in a worktree on `feat/engine-search-surface`.
Migration `0015`, engine + API + one CLI door. Built finding (a) of item 18:
`notes_fts` was the only virtual table in the repo, so a search box would have
found the six notes a reader has written and none of the four hundred passages
they kept.

The prompt named three places it might be wrong. It was wrong in one of them,
right in one, and half-right in the third.

## Decisions locked

- **Triggers, not a writer — and the argument is the inverse of `notes_fts`'.**
  `notes.rs` records "`notes_fts` cannot have triggers" as a *settled answer*:
  a trigger copies between tables and a note's body is not in the database.
  Read the other way that sentence says a trigger *can* index highlights,
  because `highlights.text` is a column — and an **external-content** fts5 table
  can index it without storing the reader's private passages a second time.
  What that buys is the property the index exists for: **no write path can skip
  it.** `insert_highlight`, `refresh_device_fields`, `set_annotation`,
  `merge_books`' collision drop and `delete_book`'s cascade all maintain it
  without naming it — and so does `crates/corpus`' `gen-devdb`, which writes raw
  SQL and by design does not link the engine. An explicit Rust writer would have
  had to be re-implemented there, which is exactly what `notes_fts` in fact
  needs and says so in a comment.
- **One ranked list, ordered by within-source position rather than bm25.** The
  prompt's *why* is right — two lists a frontend interleaves is a relevance
  ordering invented above the seam. Its implicit *how* is not: fts5's rank comes
  from that index's own corpus (document frequencies, average document length),
  so a note's −8.2 and a highlight's −8.2 are not the same claim and no constant
  converts one into the other. Sorting a union on them is arithmetic on
  incommensurable units, which is worse than source order because it *looks*
  like relevance. So each index is ordered by its own rank — the one thing bm25
  supports — and the two are merged by position: best note beside best
  highlight, then the seconds. That is reciprocal rank fusion with one source
  per item, so no `k` has to be picked, and it claims nothing bm25 cannot
  support. Ties break on **recency**, the one key genuinely comparable between
  the two (`notes.created_at` and `highlights.created_at` are both "when we
  stored it", unix seconds, one clock), then kind, then id.
- **`search_notes` removed, `API_VERSION` → 2.** Recommendation accepted.
  Keeping both doors would leave a client able to fetch the halves separately
  and merge two rankings it has no rule for, which makes the one-list guarantee
  optional. The narrowing survives as a **filter** (`source`, absent means both)
  rather than a second method, because "search my notes" is a real question
  while the ordering is never the caller's. `rb notes --search` is that filter
  and shares every line of printing with `rb find`.
- **`title` on `BookFilter` is `LIKE`, refusing the FTS suggestion.** Three
  reasons in the order they bind: it must compose with `count_books`, all five
  sorts and the offset paging out of the one shared `WHERE`; it must behave like
  the five fields beside it, or callers guess; and **infix is what a shelf
  filter means** — `possess` finds *The Dispossessed* under `LIKE` and nothing
  under fts5, whose tokens start at word boundaries.
- **`rb find <query> [--notes|--highlights] [--limit]`** is the door, plus
  `notes --search` routed through it. Built because the alternative is
  `measure_stored_covers`: an engine that can do it and nothing that can ask.

## The defect the item exposed, and it was live

`search_notes` bound the user's raw text straight into `MATCH`. fts5 reads `-`,
`+`, `*`, `:`, `(`, `NEAR`, `OR` and a bare `'` as query syntax, so
`rb notes --search "don't"` and `C++` failed with a **raw sqlx error** rather
than a search — and had since item 7. Every token is now a quoted phrase joined
by a space (which fts5 reads as AND); a trailing `*` survives because prefix
search is reached for on purpose, and nothing else does.

The same measurement settled the empty case the prompt asked about: **`MATCH ''`
raises**, so `Some("")` was never "returns nothing" for free. An empty or
whitespace query is no hits, no error, and no statement issued.

## Measured, not reasoned about

Everything below was rehearsed in `sqlite3` 3.51 before the migration was
written, and two of the four changed the file.

- **A foreign-key cascade fires the child table's delete trigger.** Run both
  ways: with the trigger, `DELETE FROM books` clears the highlights' index rows;
  without it, `MATCH` keeps returning the deleted book's rowids and
  `integrity-check` still passes. That is the "a highlight in the index and not
  in the table" failure inverted, and nothing would have said so.
- **`AFTER UPDATE OF text, ko_note, annotation`, not `AFTER UPDATE`.**
  `attribute_highlights` rewrites `reading_id` for every highlight of a book on
  every import and `merge_books` rewrites `book_id` per moved row. Neither
  touches an indexed column, and an unrestricted trigger makes both pay a
  delete-and-reinsert of the index per row for nothing.
- `INSERT INTO highlights_fts(highlights_fts) VALUES('rebuild')` is the whole
  back-fill. Unlike `0012` and `0014` — the repo's two deliberate
  non-back-fills — this one *can* back-fill honestly, because the values are
  already in the column.
- `snippet(tbl, -1, …)` picks the column the match is in, which is what lets
  `highlights_fts` index three columns and still answer in one string.

## Gotchas

- **The redaction test was proved able to fail.** A search query is exactly
  where a helpful `info!("searching for {q}")` gets added, so
  `a_search_query_never_reaches_a_log_above_trace` was written and then checked
  by promoting the `trace!` to `info!` — it failed with
  `query="Hansu"`, which is the point. It covers the typed query *and* the text
  of the highlight that came back. Only the hit **count** is at `debug!`; a
  number is not the query.
- **`chapter` is not indexed.** Every highlight in a book carries one, so it
  matches broadly and means nothing. `annotation` *is*, even though it is ours
  and the other two columns are the device's — the ownership seam is about who
  may overwrite what, and a search box asked "where did I read that" is served
  by all three.
- The one-list consequence worth stating rather than discovering: a query
  matching fifty notes strongly and one highlight weakly puts that weak
  highlight at the top. For this app that is right — a single matching passage
  buried at position 47 is a passage nobody sees.

## Gate

`make fmt lint build-check test ts-check`, all green; plus `cargo fmt --all
--check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and
`cargo test --workspace` run directly, since a subagent cannot launch
`cargo-tester`. Not `make ci` — a fresh worktree has no `gui/node_modules`, so
`web-check` and `routes` would print `SKIPPED:` and pass without running.
