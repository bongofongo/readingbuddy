---
title: Item 20 — covers a grid can actually use — migration `0014`
date: 2026-08-05
source: docs/gui/spec-gui-17-28.md item 20; docs/prompts/17-derived-facts.md's
        "G3" for the live bug; docs/decisions.md entry 17 for what landed under it
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Prompt — Item 20: covers a grid can actually use

Paste into a fresh session at the repo root, on branch `feat/engine-covers`,
branched from `main`. **Go first** in this wave: item 19 wants your stored
aspect, and item 18 shares `storage/books.rs` with you — see *Launch order* in
`docs/next-thread-handoff.md`.

Read `CLAUDE.md` (**Engine standards** is binding), then
**`crates/engine/migrations/CLAUDE.md` before you write the migration file**,
then `crates/engine/src/storage/CLAUDE.md`, then item 20 in
`docs/gui/spec-gui-17-28.md`.

**Engine only. Owns migration `0014`.** `0015` belongs to item 23; do not take
it. The contiguity test fails on a **gap** as well as a duplicate, so if you see
red because `0015` merged first, rebase — never renumber.

## You are fixing a live bug on `main`, and it is the reason to go first

**Every Google Books cover collides on one filename.** `filename_from_url`
(`crates/engine/src/images.rs`) names the file from the URL's last path segment,
and a Google Books thumbnail is `.../books/content?id=…` — so **every**
GB-sourced cover writes `images_dir/content` and the last import wins. Two books
render each other's cover. Epub extraction (`slugify(title)`) collides on two
editions of one title. The fallback to the literal `"cover.jpg"` when a URL has
no last segment makes a third collision reachable rather than merely possible.

It is invisible in a single-provider library, and `make dev-db` generates its own
covers, so **nothing in this repo will catch it for you**. The
`api-surface-auditor` agent found it before a line of Svelte was written; it has
been carried in three handoffs waiting for the item that rewrites cover storage
anyway. That is you.

**The fix is to name by content hash** — the sha256 pattern `files.rs` already
established for book bytes (migration `0010`). Two things fall out for free and
both are worth stating in the code: the write becomes **idempotent**, and
`FetchCover` gets a **skip-if-present**.

Keep the original name as a column. And **leave the path-traversal guard alone**:
it is careful, well-tested, and the reason it exists (`the name is
attacker-influenced… "safe because of what a dependency happens to do" is not a
property to leave unasserted on a path write`) does not go away just because the
name is now a hash. A hash cannot traverse; assert that rather than deleting the
assertion.

## The migration: three columns, and they are `MERGE_RULES` rows

**20b.** Add cover `width`, `height` and `accent` to `books`. Today the renderer
*decodes the image* to get them — `Cover::aspect` and `accent_from_border`
(`crates/tui/src/render3d/texture.rs`) — so a shelf of three hundred spines
decodes three hundred images to find out how wide to draw them. Compute both once
at download time.

The accent is what gives a spine a colour when there is no spine art, **which is
every book**: providers ship front covers only.

**Before you add a column to `books`, read `MERGE_RULES` in
`crates/engine/src/storage/books.rs`.** It generates six things — the upsert's
`ON CONFLICT`, `enrich_book`'s `UPDATE`, `merge_books`' `dst`-wins fill, the
`field_provenance` stamps, `Rule::show`, and `Rule::federated`
(`search::merge_provider_record`) — plus `PROBES` in `tests_support`, whose
column list is *asserted* equal to `MERGE_RULES`' **in order**. A new column
added anywhere else fails those sweeps with a message rather than going quietly
uncovered. That is the arrangement working; do not route around it.

Three questions it forces, and they are the interesting part of this item:

- **Are these provider-owned facts at all?** A width and an accent are not
  bibliographic — they are properties of *the file we downloaded*. If a user
  replaces the cover, whose claim is it? `Federated::Local` exists and names
  exactly two columns today (`sort_title`, `cover_path`), asserted by
  `only_our_own_columns_sit_out_the_federated_merge` so that `Local` cannot
  become the quiet way to skip the question. Answer the question, then decide.
- **`cover_path` + `width`/`height`/`accent` look like a group.** `Rule::pair`
  guards two halves off either half's claim; it exists because a user-owned
  ISBN-13 was being outvoted while the 10 landed from a different edition. Three
  columns whose values are only meaningful for *the same image* is the same
  shape. Decide whether it needs the guard or whether the write path makes it
  unrepresentable.
- **Back-fill.** `database/images/` has real files in it. The spec asks for a
  back-fill and migration `0005` is the repo's precedent for a destructive one
  (back-fill, *then* `DROP COLUMN`). But a migration cannot decode a PNG — so the
  back-fill is almost certainly a **command**, not SQL, and `tests/migrations.rs`
  applies the migrations in two halves precisely to exercise a back-fill a
  fully-migrated database can never reach. Say which you chose.

## 20c. Size tiers

OpenLibrary is pinned to `-M` (`providers/openlibrary.rs`) and Google Books to
`thumbnail` with a `smallThumbnail` fallback (`providers/googlebooks.rs`). Both
are too small for a cover-forward hero shot on a retina display. Ask for the
larger sizes and generate a thumbnail tier locally for the shelf.

Two constraints: **no network in tests, ever** — `wiremock` is only for real
status codes — and nothing that can carry an API key goes into a tracing field
without `googlebooks::scrub_key`.

## 20d. Not a gap — recorded so it is not re-investigated

EPUB cover extraction **already exists and is wired**: `epub::extract_cover`,
called from `Engine`. It is in scope only because it shares the filename
collision.

## What must not happen

- **Never edit an applied migration.** CI's `migrations` job refuses a modified,
  deleted or renamed one outright.
- **Do not touch the 3D renderer.** `docs/decisions.md` freezes it and freezing
  it is right. You are removing the *reason* it decodes images, not changing what
  it draws. Item 19 moves `Model::new`'s arithmetic down and will rewire onto
  your columns — coordinate through the columns, not through `render3d/`.
- **Never join `images_dir` with `cover_path`.** `cover_path` is a **whole
  path** (`images_dir.join(name)`), so joining doubles the prefix. If you change
  what is stored there, say so loudly: the GUI's `TauriClient.coverSrc` and the
  asset-protocol scope both depend on the current shape, and
  `gui/CLAUDE.md` documents it.
- **No prose.** Not one string a user reads; the accent is a value, not a colour
  name.

## Files you own, and the ones you share

Yours: `crates/engine/src/images.rs`, `crates/engine/migrations/0014_*.sql`,
`crates/engine/src/providers/openlibrary.rs`,
`crates/engine/src/providers/googlebooks.rs`.

Shared: **`crates/engine/src/storage/books.rs`** — you add columns, item 18
changes `list_books`. You go first. And `crates/engine/src/book.rs` +
`crates/api/src/dto.rs` for the new fields; item 17 just added `ko_percent`
there, so the shape of that change is fresh and worth copying.

## Push back rather than comply

Four of five threads in the last wave did, and each time they were right. The
line to argue with here: whether cover dimensions belong on `books` at all rather
than beside the image. A `covers` table keyed on the content hash is the other
design, it makes the back-fill and the idempotent write natural, and it is not
what the spec says. If it is better, say so — but the spec's answer has one
argument in its favour worth beating: a shelf query that needs a join per row is
the problem this item exists to remove.

## Done means

- `make ci` exit 0.
- `make ts` run and `bindings.ts` committed with any DTO edit.
- The `cargo-tester` agent before you call it done.
- **A test that would have caught the collision.** Two books, two Google Books
  URLs that differ only in query string, two covers on disk. That test is the
  deliverable as much as the fix is — the bug survived three handoffs because
  nothing could see it.
- **The corrections this build forced, written into `docs/decisions.md`.**
- A session log, via the `wrap-session` skill.
