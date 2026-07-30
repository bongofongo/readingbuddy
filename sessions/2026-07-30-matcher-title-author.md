# 2026-07-30 — the library matcher: title+author, not jaro-winkler on titles

Reported symptom: the calibre shelf offers "maybe *X* (7x%)" against rows that
have nothing to do with *X*.

## The bug

`koreader::title_scores_for` was the matcher for **all four** import paths
(calibre, Goodreads, KOReader sidecars, owned files):
`jaro_winkler(normalize(a), normalize(b))` on titles alone, band 0.60–0.85.

Three causes compound, and none of them are visible from one example:

- Jaro-Winkler is a character-transposition metric with a **4-char prefix
  bonus**, built for personal names. `search::normalize` drops a leading
  article, which makes a shared first word — and therefore that bonus — *more*
  likely.
- **0.60 is not a weak threshold for JW.** For two English titles it is roughly
  "both are English". Measured over 780 pairs of real titles: **10% land in the
  band**.
- The score is the **maximum over the whole library** (`list_books(10_000)`),
  so it is an extreme-value draw, then printed as `{:.0}%`. P(some book lands in
  band) = 65% at 10 books, **99.5% at 50, ~100% at 100+**.

Worst outcome was not the noise: `Dune`/`Dune Messiah` scored **0.867** and
**auto-linked**, silently, to the wrong book.

The repo already disagreed with itself — `search.rs::same_work` (provider dedup)
requires `title_sim > 0.93 && author_sim > 0.9`, while the library matcher used
0.85 title-only. `docs/decisions.md` says "do not invent a second matcher"; two
existed, and the loose one was the one making duplicates.

## Decisions locked

- **One module, `crates/engine/src/matching.rs`.** Pure functions, no `Storage`,
  so the rule is proptestable and lives in exactly one place. `koreader.rs` keeps
  the storage half (`scores_for`) and the band filter (`band`).
- **Two signals, and `compare` returns `Option`.** Returning `None` — rather
  than a low score — is the half that matters: the old matcher had no way to say
  *nothing here looks like it*, so it always named its best coincidence.
- **Title: shared content word required.** `score = 0.65·jaro_winkler +
  0.35·dice` over stopword-stripped tokens. Zero shared tokens ⇒ `None`, unless
  the raw strings clear `TYPO_ONLY = 0.90` (a misspelled one-word title, not a
  coincidence).
- **Author: a veto, and only a veto.** It is never asked "are these the same
  person" — that question has no answer here (see gotcha below). Empty on either
  side is **not** disagreement, which is the common case (sidecar-seeded book,
  file matched by its stem).
- **Band constants unchanged** (0.60 / 0.85). They now sit on a score that earns
  them.
- Chose author-gate + token-title over "raise the thresholds" — at 0.83,
  `Art of War`/`Art of Racing in the Rain` survives any threshold move that
  doesn't also kill real subtitle variants.

## Technical gotchas

- **Jaro-Winkler's floor is no lower on names than on titles.** This killed the
  first author rule. `Min Jin Lee` vs `Someone Else Entirely` = **0.64**, which
  is *above* `J.R.R. Tolkien` vs `John Ronald Reuel Tolkien` = **0.65**. No
  character threshold separates one person from two. The working rule is
  **shared name token ≥3 chars**, with JW ≥ 0.85 only as a fallback for
  `Dostoevsky`/`Dostoyevsky`. `NAME_TOKEN_MIN` is in **chars**, not bytes — a
  two-char CJK name is a whole name, caught by the JW fallback instead.
- **`Frank Herbert` vs `Brian Herbert` is 0.90 and is two people.** Deliberately
  not separated: the veto's cost is asymmetric (a wrong veto *hides a book you
  own*; a missing veto falls back to the title, which the token gate already
  made safe), so it fires only where names share almost nothing.
- **Author name forms differ per system.** calibre emits `Min Jin Lee`, an
  epub's `author_sort` emits `Lee, Min Jin`. Compared as written they score like
  different people. `author_key` sorts tokens alphabetically → both `jin lee
  min`. Uses its own tokenizer, *not* `search::normalize`, which would drop the
  leading `A.` from `A. N. Writer`.
- **`sidecar_seen` already cached `authors` verbatim**, so a cache hit still
  reaches the verdict a fresh parse would. Had it not, the device scan would
  have silently disagreed with itself. Now asserted, not assumed
  (`a_book_by_somebody_else_is_not_offered_and_the_cache_agrees`).
- **`calibre::match_book` ran the full library scan twice per row** —
  `title_scores_for` then `candidates_for_title`, which re-listed 10 000 books.
  400 calibre rows = 800 loads of the whole shelf. Now one `scores_for` feeds
  both answers via `band()`.
- **Two fixtures were themselves the bug**, and both went green under the old
  rule:
  - `tests/goodreads.rs` seeded **"Pacific Ocean Notes"** as the near-miss for
    CSV title **"Pachinko"** — a different book, in the band on the shared `Pac`
    prefix alone. Its own comment admitted the mechanism. Re-cut to
    `"Pachinko: A Novel of Korea and Japan"`.
  - `tests/workflows.rs` had one book arriving as `Min Jin Lee` from the sidecar
    and `A Test Author` from `write_isbnless_epub` — two authors for one book,
    which the veto correctly refused. Added `write_isbnless_epub_by` so a test
    can say the same author on both sides.
- Its sibling assertion needed loosening too: `resolve_books("Pachinko")` is no
  longer empty once the seeded near-miss is a real variant, so it now asserts
  *which* book is there rather than that none is.

## Verification

- `make ci` green — fmt, `clippy --workspace --all-targets -D warnings`,
  `cargo check --workspace --locked`, full workspace suite (254 engine unit
  tests + every integration target).
- 9 new tests in `matching.rs`: a verdict table (drop/band/auto) over the real
  pairs, author-veto cases, sort-form names, and three properties (self-match is
  exactly 1.0, score is a finite fraction, comparison is symmetric).
- **Live, against the real library** — 91 calibre rows vs 38 library books,
  replicating both rules in python off `calibredb list` + `books`:

  ```
  rows offered a candidate   BEFORE: 76    AFTER: 0

  BEFORE, worst:
    81.7%  The Tale of Genji      -> A Tale of Two Cities
    79.6%  Freakonomics           -> Soccernomics
    75.9%  Norwegian Wood         -> The Norman Conquest
    73.6%  Kafka on the Shore     -> The Sign of the Four
  ```

  All 35 genuine repeats still auto-link at 100%. One row hit the author veto
  (Genji vs Two Cities). `readingbuddy calibre import --dry-run` now reports
  "0 left for you to decide about".

## Deferred

- **Hoisting the library snapshot out of the per-row loop.** `scores_for` still
  calls `list_books(10_000)` once per row; killing the double scan halved it,
  but a `Prepared`-style library index built once per import run would make it
  O(1) loads. Not done — the double scan was the egregious half.
- **`search.rs::same_work` was left alone.** It is provider dedup with its own
  thresholds (0.93/0.9) and a different job; folding it into `matching.rs` is a
  separate change with its own risk.
- **`notes.title` is still not unique** — unrelated, but the other known
  matching-adjacent gap.
