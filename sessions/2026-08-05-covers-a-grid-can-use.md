---
title: Item 20 — covers a grid can use, and the invariant one writer could not hold
date: 2026-08-05
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Session log

Item 20, on `feat/engine-covers` in a worktree, first in the four-way engine
wave. Migration `0014`. Engine only; item 18 branches off this merge and item
19 rewires onto the columns it added.

## The bug, and why nothing in the repo could see it

`filename_from_url` named a cover from the URL's **last path segment**. A
Google Books thumbnail is `.../books/content?id=…`, so the last segment is the
literal string `content` — for every cover Google has ever served. Every
GB-sourced jacket in a library therefore wrote `images_dir/content` and the
last import won. Two books rendering each other's cover, permanently, with
nothing on either screen looking wrong.

It is invisible in a single-provider library, and `make dev-db` generates its
own covers, so no fixture in this repo touched it. It was found by the
`api-surface-auditor` agent before a line of Svelte existed and carried in
three handoffs. `two_google_books_covers_are_two_files` is now the thing that
would have caught it — offline by construction, because the collision was in
the *naming* and never in the fetch.

Two siblings of the same bug went with it: `slugify(title)` in
`epub::extract_cover` (two editions of one book), and the `"cover.jpg"`
fallback, which made a third reachable rather than merely possible.

## Decisions locked

- **Named by the sha256 of the content**, the pattern `files.rs` set in `0010`,
  with the extension read from the bytes (`guess_format`) rather than from the
  URL or from the epub's declared mime type. The **path-traversal guard is kept
  and re-argued rather than deleted**, as the prompt asked: what changed is that
  both halves of the name are now closed sets, so the property it asserts is
  provable instead of true by accident of what `Url::parse` normalizes.
- **The four measurements are deliberately *not* `MERGE_RULES` rows.** This is
  the opposite of what `0013` did with three columns, and it is the first time
  that has been the right answer. `MERGE_RULES` governs what a *record* can
  carry, with a source to attribute; no provider publishes a width, and the only
  honest answer to "where did this come from" is "we decoded the file". A
  `Federated::Local` row would say that in the table's vocabulary and would then
  hand every record-shaped writer a way to move one of them without moving
  `cover_path`.
- **`Rule::pair` is not that fix**, for three separate reasons and it is worth
  keeping all three: it is binary and asserted symmetric where this is a star of
  four columns around a fifth; it guards against a *user claim* rather than
  against incoherence; and `merge_books`' fill — the one place the incoherence
  was genuinely reachable — runs with the user guard **off**, so it would never
  have fired there at all.
- **The accent is stored unclamped.** `render3d` pushes it into a legible luma
  band so a white jacket still reads as a board; that band is a renderer's
  policy about its own lighting, not a fact about the file. Item 17's line,
  applied to a colour.
- **Both halves of 20c are one change.** OpenLibrary `-M` → `-L`, Google Books
  largest-first over all six documented `imageLinks` sizes — *and* a locally
  generated shelf tier, because asking for the large size without one makes
  every list strictly heavier than it was. `edge=curl` is stripped on the way
  past: it composites a page-curl graphic onto the jacket, which is somebody
  else's decoration baked into bytes we store for ever and take the border
  colour off.
- **The back-fill is a command.** SQLite cannot decode a PNG.
  `Engine::measure_stored_covers` writes through the same `set_cover` the
  download path uses, so a back-filled row and a fresh one are the same row. It
  does **not rename**: the stored `cover_path` is what a webview resolves.

## The correction this build forced

**One writer was not enough, and the test is what said so.**

The design was "`Storage::set_cover` is the only writer of the four
measurements, so a row describing an image it is not pointing at is
unrepresentable". `a_provider_write_never_touches_the_cover_metrics` failed on
its first run with `left: Some("database/images/9.jpg")`.

`cover_path` *is* a `MERGE_RULES` column with three other writers, and
`Merge::Coalesce` under `Winner::Incoming` means an **incoming record's path
wins**. So a calibre row or an `rb set` carrying a cover repointed the row and
left a width, a height and an accent belonging to the jacket before it — the
cover-shaped version of *Dune #2* under a different series' name.

The fix is the second half of the rule, and it is the more interesting half:
`merge_set` now also generates `invalidate_cover_metrics`, which sets the four
back to NULL whenever the path moves, **from the same expression that stores the
path** — so it cannot fall out of step with the merge rule, the user guard or
the `dst`-wins inversion. `IS` rather than `=`, because both sides are nullable
and the case where a book gains its *first* cover is `NULL = NULL`.

Between the two halves the bad row is genuinely unrepresentable. One writer
alone was a claim about who calls what, which is the kind of claim a later
thread breaks without noticing.

## Two smaller corrections, both consequences of content addressing

- **Two books can now share one file**, which is ordinary rather than exotic:
  the repo already says the ISBN-less insert path *guarantees* duplicates. Both
  `delete_book` and `merge_books` used to hand the caller a path to unlink on
  the assumption that nothing else held it, which would take a surviving book's
  cover with it. Both now ask the database. `book_files` gets to skip that check
  because it is keyed on the sha alone and a second holder is unrepresentable
  there; `books` holds a plain column, where a second holder is ordinary.
- **`set_cover` replaces where every other writer coalesces**, and that fixed a
  latent bug rather than adding an exception: `download_cover` went through the
  no-clobber merge, so a re-fetch producing a *different* file left the row on
  the old one. Invisible under URL naming, where the new file usually had the
  same name.

## Pushed back on the prompt

- **The prompt's own invitation — a `covers` table keyed on the content hash —
  was considered and refused, but not for the spec's reason.** "A shelf query
  that needs a join per row" is the weakest argument available: a single `LEFT
  JOIN` against a primary key is not what makes a list slow, and `BOOK_FROM`
  already carries a heavier one for readings. The real argument is **ownership**.
  A shared `covers` table makes the sharing explicit and then owes an answer for
  reference counting, garbage collection, and what `merge_books` does to a row
  two books point at — a lifecycle, in a codebase whose one rule about ownership
  is per-field provenance on `books`. Four columns beside the path they describe
  need no lifecycle, and the invalidation clause above is only expressible
  *because* they sit in the same statement as the path.
- **`cover_thumb_path` is a column; the thumbnail is not a table row or a
  `MergeReport` field.** It is a sibling of the cover named from the same hash,
  so `thumb_path_of` derives it and a caller holding only a `cover_path` can
  clean up both.
- **The provenance stamp moved.** `set_cover`'s `source` claims `cover_path` and
  nothing else — the file has an origin (whoever supplied the URL), the
  measurements do not.

## `sort_author` — the answer item 18 needs

**No. This item does not carry it**, though it owned the wave's only migration
and item 17 deferred it for exactly that reason ("it is a migration, and item 17
has none").

- It is not one migration. **SQLite cannot compute the value**, so the column is
  NULL for every existing row and `ORDER BY sort_author` is silently *wrong*
  until a back-fill command nobody has run yet runs. A column that gives the
  wrong answer until told otherwise is worse than a slow one that is always
  right.
- The fallback for that window is `list_books_by_author` — the whole-table read
  the column exists to remove. So carrying it **adds an arm beside the slow one
  instead of replacing it**, which is the opposite of what item 18 wants.
- The write side is now *cheap*: `invalidate_cover_metrics` is exactly the
  pattern (a companion clause generated from another column's value expression,
  bound from Rust because SQL cannot derive it). That makes `sort_author` a
  smaller item than it was this morning — a reason to do it deliberately with
  its own migration, not a reason to bolt it onto one whose subject is images.
- **`sort_title` is the warning.** It is a `MERGE_RULES` column,
  `Federated::Local`, on `Book` and on the DTO — and **nothing in this repo has
  ever computed it**. `BookSort::Title` orders by `books.title COLLATE NOCASE`.
  A sort-key column added without a writer looks answered and is not, and a
  hurried `sort_author` is the second one.

**For item 18:** design pagination against the four SQL arms and leave `Author`
as it is — read, sort in Rust, truncate — applying the window after the sort.
That is correct at any size. When it stops being fast enough, `sort_author` is
its own item: migration, companion clause in `merge_set`, back-fill command, and
*then* `BookSort::Author` becomes an ordinary `ORDER BY` and the Rust path is
deleted rather than duplicated.

## What was deliberately not built

- **No renaming of existing images.** The stored `cover_path` is what a webview
  resolves and `gui/CLAUDE.md` documents its shape; renaming every file would be
  a destructive change dressed as a measurement, and it buys nothing — the
  collision it would fix is in the *write* path, which is already fixed.
- **No CLI or API door for the back-fill.** The prompt scoped this to the
  engine, and `measure_stored_covers` is on the facade waiting for one.
- **No change to `render3d`.** `docs/decisions.md` freezes it and item 19 owns
  the rewire. The border-median arithmetic is briefly duplicated between
  `images.rs` and `render3d/texture.rs`; item 19 deletes the copy there.
- **No `Rule::pair` generalisation to n-ary groups.** It is the general answer to
  a question this item turned out not to be asking — see above.

## Verification

`make fmt`, `make lint`, `make build-check`, `make test`, `make ts-check` — all
green, plus the `cargo-tester` agent. `make web-check` and `make routes` were
**not** run: `gui/node_modules` is gitignored and absent in a worktree, so both
degrade to `SKIPPED:` and would be a false green. `make ts` was run and
`bindings.ts` is committed with the DTO change.
