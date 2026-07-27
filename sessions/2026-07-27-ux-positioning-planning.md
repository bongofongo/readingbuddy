# 2026-07-27 — UX positioning and feature planning

Seven-round Q&A session, no code. Produced `docs/ux-positioning.md` (the
archive of reasoning), `docs/decisions.md` (settled decisions, no reasoning),
`docs/spec-engine-01-03.md`, and three paste-ready thread prompts under
`docs/prompts/`.

## Decisions locked

- **Positioning**: "the desk you sit at after you put the book down." Never a
  reader; everything is downstream of KOReader/Kindle. The unit of value is the
  *connection between highlights*, not the highlight.
- **Axiom: "a place, not a tool."** Binding consequences: state persists and is
  visible; nothing modal-by-default; idle is not blank; **no task-completion
  framing** — this is what killed the orphan queue as a home screen.
- **Ownership is per-field, provenance recorded.** Earlier "we own everything we
  copy" was overstated and corrected by the user: KOReader is the legitimate
  origin of highlights and reading state. Copy-in buys durability and
  cross-book query, not authority.
- **Home screen** = currently-reading, with progress. Its action is "open the
  reflection".
- **Highlight text frozen, annotation editable.** New `annotation` column
  (ours); `note` renames to `ko_note` (theirs).
- **`readings` table now**, not retrofitted — rereads first-class. Progress
  moves off `books`.
- **Reflection (private, the graph hub) and Review (public, rated) are two
  objects, written separately.** User explicitly scrapped the shared-body idea:
  a public review is a rewrite for a different audience, not a subset.
  Book-to-book connection runs reflection-to-reflection.
- **Ratings numeric-only v1**; user-defined scale + explicit user-editable
  lookup table to Goodreads' 0–5. Never store only the mapped value.
- **Frontends**: TUI focus until the engine has definite shape, then
  **Tauri + Svelte** (no FFI, one language, Linux ships). SwiftUI not gatekept.
  Daemon yes — but **the boundary is the API, not the process**.
- **Calibre**: all three tiers in importance order, feature-detected, never a
  hard dependency.
- **Device**: one-way for now; wired first; device screen with per-book state;
  plugin installed by us onto the reader, reversibly.
- **Collections deferred** (three systems minting collections = merge hell), but
  Goodreads shelf data preserved as inert provenance so a later design has real
  data.
- **3D renderer frozen** as-is; `raster.rs` already emits RGBA so any frontend
  displays a Rust-rendered image.

## Bugs found (all pre-existing, none fixed — docs-only session)

- **Device note edits are silently discarded.** `storage/highlights.rs:55` is
  `ON CONFLICT(book_id, identity_hash) DO NOTHING`, and `note` is not among the
  hash's four inputs (`highlights.rs:22`). Edit a note on the device, re-import,
  nothing updates — and it reports as `skipped`, indistinguishable from
  "identical". Contradicts the ownership rule. Fix specced as build item 2.
- **ISBN-less books duplicate unconditionally.** `storage/books.rs:90` branches
  isbn_10 → isbn_13 → *plain `INSERT`*. Any book without an ISBN (which is every
  sidecar-seeded book) takes branch three every time. Pull from device, then
  search+add the same book → two rows, one with highlights, one with the cover,
  nothing joining them. This is why `merge_books` is mandatory, not optional.
- **Unmatched sidecars are a dead end.** `koreader.rs:392` pushes to
  `report.unmatched` and `continue`s. Import only ever attaches highlights to
  books that already exist, so "pull this book in from the reader" has no engine
  support at all.
- **`partial_md5` is parsed and thrown away.** `koreader.rs:78` reads the root
  `partial_md5_checksum` into `KoSidecar.partial_md5`; nothing uses it. Device
  matching can use it immediately — computing the algorithm ourselves is only
  needed for owning our own files.

## Technical gotchas

- **`DO NOTHING` → `DO UPDATE` breaks insert-detection.** `RETURNING id` then
  yields a row on the conflict path too, so `Some(id)` stops meaning "newly
  inserted" and the `inserted`/`skipped` counts the goldens assert collapse.
  Fix: leave the insert alone, add a separate targeted-update call.
- **`COALESCE` no-clobber is right for providers and wrong for the device.**
  A provider returns a *partial* record (missing = "don't know"); a sidecar is
  the *complete* state of an annotation (missing note = user deleted it).
  Copying the books-upsert pattern to device fields makes note deletion
  impossible to sync, permanently.
- **`reimport_is_strictly_idempotent` isn't wrong — it's under-specified.** It
  re-imports an *identical* fixture, so it can't distinguish "nothing should
  have changed" from "we discard device updates". The missing thing is a
  fixture (`Pachinko-NoteEdited.sdr`), not a weaker assertion.
- **`highlights.id` stability is load-bearing and unasserted.**
  `notes.highlight_id` and `flashcards.highlight_id` are FKs
  (`0001_init.sql:48`, `:67`) — a delete-and-reinsert refresh would silently
  null note anchors and cascade flashcards away.
- **`book_id` is an input to `identity_hash`**, so `merge_books` must recompute
  every moved highlight's hash against the destination and drop collisions.
- **Open question that decides a schema**: does KOReader bump an annotation's
  `datetime` when its note is edited? If yes, the same highlight re-imports as a
  *duplicate row* and the identity hash must drop `ko_datetime` for
  `pos0`+`text`. Item 1a must answer this with evidence before item 2 starts.
- **Goodreads' API is dead** (shut down Dec 2020, no new keys). CSV both
  directions is the only interface. `My Rating` is **integer 0–5**, no halves
  (StoryGraph takes quarters). Its `Exclusive Shelf` is reading *status*, not a
  collection — only `Bookshelves` has the merge problem.
- **WidgetKit extensions must be SwiftUI**, are always sandboxed, and can read
  only their App Group container — but need **not** live in the same app. Two
  apps from one team can share a group, and a *non-sandboxed* app can write
  `~/Library/Group Containers/<id>/` as an ordinary directory. So a Tauri app
  can feed a tiny SwiftUI widget companion without embedding an Xcode extension
  in its bundle (which is an entitlement/notarization minefield).
- **iOS has no user daemons.** A daemon-shaped architecture locks iOS out unless
  the logic lives in an API crate the daemon merely wraps.
- **KOReader's sidecar is per-file and a reread appends to it**, so the device
  cannot attribute highlights to a particular reading. `reading_id` must be
  nullable and best-effort by date window.
- **`pos0` is a cre-engine xpointer** — resolving it means reimplementing enough
  of that engine to agree with it. The excerpt view should search the epub for
  the highlight's *text* instead.
- **`dry_run` is not free**: it evaluates every sidecar's Lua in mlua plus book
  matching. A device screen must pre-filter on sidecar `mtime` + size.
- **Calibre 7.26.0 is installed** at `/opt/homebrew/bin`. `calibredb list
  --for-machine` emits JSON; `ebook-convert` has no JSON output;
  `calibre-debug -e script.py` is the full-API escape hatch. GPL-3, but
  **shelling out to a binary is not linking** — cleaner than the in-process
  `epub =2.1.4`.
- **No curated public `.sdr` corpus exists.** Mint real ones with desktop
  KOReader (Linux AppImage / macOS) against `epubs/` — which also satisfies the
  standing rule that fixtures must not be generated by our own parser, since
  here the generator is the reference implementation.

## Verification

Docs-only session; no Rust touched. `cargo test --workspace` and
`cargo clippy --workspace --all-targets` run as the wrap-up gate.

## Deferred

Collections (until real imported data exists). Provider enrichment on device
pull (v2). Two-way device sync (needs the plugin infra solid first). Excerpt
view, orphan queue, graph view, author/corpus view, shelf view, publishing the
public review, non-numeric rating scales. GUI, widget and iOS companions. The
fate of `notes.kind = 'final'` — explicitly punted to a later iteration.
