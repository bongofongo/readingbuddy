---
title: Settled Decisions
date: 2026-07-27
source: docs/ux-positioning.md (rounds 1–7) — the reasoning lives there
---

# Settled Decisions

What was decided. Not why. For the argument behind any line, see
`docs/ux-positioning.md`.

## Positioning

- readingbuddy is **the desk you sit at after you put the book down**, with the
  library on the shelf behind it. It is not a reader and never competes with one.
- KOReader/Kindle is where reading and light note-taking happen. Everything
  readingbuddy does is downstream of that.
- The unit of value is **the connection between highlights**, not the highlight.
- "Self-hosted Readwise" is a thing it does, never its identity.
- Built for one user. Obsidian's community is the likely channel if it ever
  needs one.

## Design axiom

> **A place, not a tool.**

Consequences, binding:

- State persists and is visible; the app remembers where you were.
- Nothing is modal-by-default; nothing is a dead end.
- Idle is not blank — the ambient layer and the turning book earn their cost here.
- **No task-completion framing.** No inbox, no badge counting unwritten
  highlights. Places you can go, never numbers that greet you.

## Data ownership

**Authority is per-field. Provenance is recorded. readingbuddy keeps a durable,
queryable local copy of everything but does not claim to be the origin of what
it copies.**

| Origin | Data |
|---|---|
| KOReader | highlight text/position/colour, the note attached at capture, reading position and percent, per-page read time, on-device status and rating |
| Calibre | the file, its format, curated metadata |
| Providers | ISBN, publisher, page count, cover, description |
| readingbuddy | vault, links, reflections, reviews, cross-book structure, flashcards, annotations |

- Conflicts resolve **toward the origin** for origin-owned fields.
- **Provider merges use `COALESCE` no-clobber** (partial records; missing means
  "don't know"). **Device merges use straight assignment** (a sidecar is the
  complete state; missing means the user deleted it). Do not copy one pattern to
  the other.

## Highlights

- **Highlight text is frozen.** Never editable in readingbuddy — it is part of
  the identity hash.
- **Your annotation is editable.** New `highlights.annotation` column, ours,
  never touched by import.
- **Rename `highlights.note` → `ko_note`** so ownership is visible in the schema.
- Device-owned payload fields (`ko_note`, `color`, `chapter`, `page`) **refresh
  from the device on re-import**.
- Idempotency invariant becomes: *no field we own mutates; device-owned fields
  track the device; `highlights.id` is stable.*
- `notes.highlight_id` stays for full vault notes. "Promote annotation to note"
  is the affordance when a reaction grows.
- Store `last_seen_ko_note` for future two-way sync.

## Readings

- **`readings` table now**, not retrofitted. Rereads are first-class.
- `books.current_page` / `finished` / `date_started` / `date_finished` move to
  the active reading.
- Reflections and reviews anchor to a **reading**, not a book.
- Highlights keep `book_id` authoritative plus a **nullable `reading_id`**
  assigned by matching `ko_datetime` into a reading's date window. KOReader's
  sidecar is per-file and a reread appends to it, so the device cannot supply
  this attribution — unattributed is correct, not a gap to paper over.

## Reflection and Review — two objects

| | Reflection | Review |
|---|---|---|
| Audience | private | public |
| Purpose | personal agglomeration; ties books together | rating + prose for others |
| Graph role | **the hub** — cites highlights, links notes and other reflections | exportable artifact |
| Rating | none | yes |
| Timing | **openable mid-book, accretes** | written at the end |

- Both markdown in the vault, both with a DB record, both anchored to a reading.
- **No shared body.** They are written separately. A public review is a rewrite
  for a different audience, not a subset of private thinking.
- Book-to-book connection runs **reflection-to-reflection**.

## Ratings

- **Numeric only for v1**: user-defined `min`, `max`, `step`.
- Export mapping to Goodreads is an **explicit user-editable lookup table**, not
  a formula. Formulas are always wrong at the ends.
- Store the raw value plus the scale id; never store only the mapped value.
- Goodreads CSV `My Rating` is **integer 0–5** (0 = unrated), no halves. Say so
  rather than silently rounding. StoryGraph accepts quarters.

## Goodreads

- **The API is dead** (shut down December 2020). CSV both directions is the
  interface, and it is better for us.
- `Exclusive Shelf` (read / currently-reading / to-read) → **reading status**,
  ours, maps onto `readings`.
- `Bookshelves` → free tags, **stored as inert provenance** (raw value + source),
  no UI, no merge semantics, until collections are designed.
- CSV import brings full history including `Read Count`.

## Collections

- **Deferred.** Three systems minting collections is a merge problem with no
  good default.
- Data is preserved meanwhile as inert provenance so a later design can be made
  against real data.
- Unattributed highlights need no staging bucket — `reading_id = NULL`, reached
  from their book.

## Device linking

- **One-way (device → app) for now.** Two-way is a later goal, gated on the sync
  and plugin infrastructure being solid. Store `last_seen_*` device values now so
  it stays possible.
- **Wired first**: mount → scan → import, nothing typed. macOS `/Volumes/…`,
  Linux `/run/media/$USER/…` and `/media/$USER/…`.
- **A device screen** listing per-book state — New / Unchanged / Updated /
  Unreadable. Single-book pull first, then multi-select and "sync everything".
- **Pull-from-device creates the book** from sidecar metadata. No provider
  enrichment in v1 — the path is fully offline.
- Matching order: **md5 mapping → title jaro-winkler ≥ 0.85 → candidate band
  (~0.60–0.85, offered as *Link*) → New.**
- Once linked, the decision is **recorded against `stats.md5` and never
  re-guessed.**
- Scan must pre-filter on sidecar `mtime` + size; re-parsing every sidecar in
  mlua on every screen open does not scale to a full device.

## KOReader plugin

- Plugin lives **on the reader** (Lua, `koreader/plugins/readingbuddy.koplugin/`).
- readingbuddy installs it itself, over the wired path, reversibly — never by
  asking for a config edit.
- Safety, non-negotiable: verify the mount is really a KOReader install; write
  only inside our own plugin directory; create-only, upgrade replaces only our
  directory; refuse to overwrite a newer version; **never automatic** (mount →
  import is automatic and read-only, mount → install is explicit and shows the
  path); uninstall is exact; the plugin **fails closed** and never blocks or
  slows the reader UI.

## Files

- **readingbuddy owns its files.** Content-addressed storage
  (`database/files/<ab>/<sha256>.<ext>`); original filename is a column, not the
  path.
- `book_files(book_id, sha256, format, original_name, size)` — many files, one
  book (epub + azw3 = two files, one book).
- Dedup has **three distinct levels**, and conflating them is the usual mistake:
  1. same bytes → sha256
  2. same book, different file → many-to-one via `book_id`
  3. same book, unknown → ISBN → **KOReader `partialMD5`** → the existing
     `search.rs` jaro-winkler fingerprint. Do not invent a second matcher.
- `partialMD5` does three jobs with one value: dedup, sidecar↔book match, and
  the join into the device's `statistics.sqlite3`.

## Calibre

- **All three tiers, in importance order**: (i) `ebook-convert` conversion,
  (ii) library import via `calibredb list --for-machine`, (iii) device push.
- **Feature-detected, never a hard dependency.** Present → the features work;
  absent → they aren't there. Never ask the user to install or configure it.
- Shelling out to a GPL-3 binary is **not linking** — no license contamination,
  unlike the in-process `epub =2.1.4`.

## Vault

- readingbuddy is the **home and primary place** for every datum it stores.
- Obsidian compatibility is a courtesy, never a constraint. Keep `[[wikilinks]]`
  and plain markdown; do not make Obsidian-only constructs load-bearing.

## Frontends

- **TUI is the focus** until the engine has a definite shape. Then the GUI.
- **Tauri + Svelte** for the main desktop app — no FFI (its backend is Rust),
  one language across the stack, Linux ships.
- SwiftUI is not gatekept for later; its advantages are a real `MenuBarExtra`
  and native Mac feel.
- **`readingbuddyd` daemon** — but **the boundary is the API, not the process.**
  A versioned API crate holds the whole surface; the daemon is a thin transport
  wrapper with no logic. That's what keeps iOS (which has no daemons) able to
  link the same crate in-process.
- **Widget and iOS are display pieces**, read-only, not requirements.
- A widget is always sandboxed and reads only its **App Group container** — but
  needn't live in the same app. Non-sandboxed Tauri app writes a snapshot to
  `~/Library/Group Containers/<id>/`; a tiny SwiftUI companion owns the widget
  and reads it. **The widget never touches the database.**
- Do not embed an Xcode extension into the Tauri bundle and re-sign — possible,
  but an entitlement and notarization minefield for no gain.
- The 3D renderer is **frozen as-is**. `raster.rs` already emits RGBA, so any
  frontend displays a Rust-rendered image; the renderer survives the frontend
  change intact.

## Out of scope for now

Excerpt view (and when it lands: **search the epub for the highlight's text**,
not `pos0` resolution — a `pos0` is a cre-engine xpointer and resolving it means
reimplementing enough of that engine to agree with it). Orphan queue. Graph view.
Author/corpus view. Shelf view. Publishing the public review. Two-way sync.
Provider enrichment on device pull. Non-numeric rating scales.

## Build order

1. KOReader format work — source, real sidecars, `summary`, `percent_finished`,
   `stats.md5`; settle the `datetime`-on-edit question.
2. Highlight ownership seam — `ko_note`/`annotation`, device-field refresh,
   `updated` counter, `last_seen_ko_note`, new fixture, three-way test.
3. `import_book_from_sidecar` + link/merge — offline, no enrichment.
4. `readings` table + progress migration.
5. `partialMD5` + the device→book mapping table.
6. Device screen — per-book state, single pull, then multi-select and sync-all.
7. Reflection + Review.
8. Currently-reading home screen; action = open the reflection.
9. Backlinks pane.
10. Goodreads CSV in/out.
11. Wired device watcher.
12. `book_files` + owned files + three-level dedup.
13. Calibre (i) then (ii).
14. API crate + `readingbuddyd`.
15. KOReader plugin + wireless push.
16. Everything under *Out of scope for now*, plus collections, the Tauri GUI and
    the Mac/iOS companions.
