# Prompt — Item 3: pull a book in from the reader

Paste into a fresh Claude Code thread at the repo root.

---

Read `docs/spec-engine-01-03.md` (item 3) and `docs/decisions.md` before
starting. `CLAUDE.md`'s **Engine standards** section is binding.

**Depends on item 1b** — the `device_books` table and `MatchMethod::Md5` must
exist. Independent of item 2; either order is fine.

## The problem

When a sidecar's book isn't in the library, it is reported and **dropped**
(`crates/engine/src/koreader.rs:392`):

```rust
let Some((book, matched_by)) = match_book(storage, &sidecar_path, &sc).await? else {
    report.unmatched.push(UnmatchedSidecar { path: sidecar_path, title: sc.title });
    continue;
};
```

Import only ever attaches highlights to books that already exist. So "pull this
book in from the reader" — the primary verb of the planned device screen — has
no engine support. Today you must add the book by title or ISBN first, then
import.

## Scope

**Fully offline.** No provider enrichment: deferred by decision. That also means
no mock provider is needed and the "no network in tests" rule is satisfied
trivially. A pulled book gets title, authors, page count and language from the
sidecar's `stats` table (falling back to `doc_props`) and has no ISBN, cover or
description.

### `import_book_from_sidecar`

Creates the book, then imports its highlights.

**Idempotency cannot come from `upsert_book`.** `storage/books.rs:90` branches
isbn_10 → isbn_13 → *plain unconditional insert*, and a sidecar-seeded book has
neither ISBN — so it takes the third branch every time and a second call creates
a second book.

Guard on `device_books` by `partial_md5` instead: known → reuse that `book_id`;
unknown → create, then record the mapping with `linked_by = 'auto'`.

If a sidecar has no `partial_md5`, still import, but emit a **typed
`Diagnostic`** (not a bare string) saying the book will be re-created on a
second pull.

### Candidate matching

`match_book` auto-links at jaro-winkler ≥ 0.85 and everything below is silently
"New" — so a variant title quietly becomes a duplicate. Add
`match_candidates(storage, &KoSidecar) -> Result<Vec<MatchCandidate>>` returning
the ambiguous band, best first. `AUTO_MATCH = 0.85` (existing) and
`CANDIDATE_MIN = 0.60`, both `const` — not config knobs, same reasoning as
`PROVIDER_TIMEOUT`.

Full matching order: **`device_books` md5 → sibling-epub ISBN → title ≥ 0.85 →
candidate band → New.**

### Link and merge

- `link_sidecar(storage, partial_md5, book_id)` — record a sidecar↔book decision
  so it is never re-guessed.
- `merge_books(storage, src, dst)` — fold one book into another: move
  highlights, notes, flashcards and `device_books` rows; merge book fields
  `COALESCE`-style with `dst` winning; delete `src`.

`merge_books` exists because the ISBN-less insert path guarantees duplicates
will occur no matter how good matching gets. Two requirements:

1. **One transaction.** A half-merged library is worse than a duplicate.
2. **Handle the identity collision.** `book_id` is an input to
   `identity_hash` (`storage/highlights.rs:22`), so every moved highlight's hash
   must be recomputed against `dst`. A highlight that then collides with one
   already on `dst` is **dropped, not duplicated**.

## Tests

All `sqlite::memory:`, all offline.

- Unmatched sidecar → book created with sidecar metadata, highlights attached,
  `device_books` row written.
- Pull twice → second is a no-op. One book, not two.
- No `partial_md5` → still imports, emits the diagnostic.
- `match_candidates` returns a near-miss title and excludes both an exact match
  and unrelated noise.
- `link_sidecar` then re-pull → matched by `Md5`, no duplicate.
- `merge_books` moves everything, deletes `src`, and drops an
  identity-colliding highlight rather than duplicating it.
- Property: merge is idempotent — `src` into `dst` twice equals once.

## Constraints

- Engine only, plus a CLI surface if one is natural. No TUI — the device screen
  is build item 6 and out of scope here.
- No network anywhere, tests or otherwise.
- Typed `Diagnostic`s, never pre-formatted strings.
- `EngineError::Other` is last-resort; if a caller might branch on it, add a
  variant.
- Never edit an applied migration.
- Every ISBN entering the system goes through `normalize_isbn`.

## Done when

`unmatched` is an actionable state instead of a dead end; pulling the same
device twice creates one book; `make ci` is green. Run the `cargo-tester` agent
before committing.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
