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
- **A reading with no `started_at` begins where the previous reading ended**, not
  at −∞. Taking an absent bound literally makes the newest reading's window
  contain every older one, so the last read silently collects the whole book's
  highlights — which is what *every* Goodreads `Read Count > 1` import did,
  since that CSV has no start date and we refuse to invent one. The derived
  bound is exclusive; an explicit one the user gave is inclusive.
- **`reading_id` is shown, not merely stored.** `rb highlights` groups by read
  once a book has more than one, the TUI's highlight list carries a one-cell read
  gutter on the same condition, and the unattributed ones are listed plainly and
  last. A column that nothing renders is a claim nothing can check.

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
- Matching order: **md5 mapping → title+author score ≥ 0.85 → candidate band
  (~0.60–0.85, offered as *Link*) → New.**
- The score is **not** a bare jaro-winkler on the titles, and was: two titles
  must share a **content word** and their authors must not **disagree**, before
  either is offered at all. On a whole title jaro-winkler's prefix bonus and its
  high floor for any two English strings put ~10% of arbitrary title pairs
  inside the band, and the band is reported as a maximum over the whole library
  — so at fifty books nearly every unmatched row grew a confident wrong
  candidate, and `Dune`/`Dune Messiah` linked itself. The author signal is a
  **veto only**: no threshold separates `J.R.R. Tolkien` from
  `John Ronald Reuel Tolkien` (one person) without also merging `Frank Herbert`
  with `Brian Herbert` (two), so it is not asked to.
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
   the device→book mapping key; settle the `datetime`-on-edit question. **Done;
   see `docs/koreader-format.md`.** Two corrections it produced: `datetime` is
   immutable (edits write `datetime_updated`), so item 2's design stands; and
   **`stats.md5` does not exist** on any 2024.11+ device — the mapping keys on
   the root `partial_md5_checksum`, so read "`stats.md5`" as that wherever it
   appears above.
   - **Follow-up:** the only realistic-scale KOReader coverage we have is the
     user's own library, which is personal and uncommittable, so it protects
     nobody else and vanishes on another machine. Add a generated
     realistic-device tier to `crates/corpus` — many books, hundreds of
     annotations, `datetime_updated`, notes, mixed statuses, PDF sidecars,
     `.lua.old` siblings — so the committed corpus reaches that scale without
     the private text. Tier 1 covers shape and tier 2 covers Gutenberg-derived
     text; neither covers a *device library*.
2. Highlight ownership seam — `ko_note`/`annotation`, device-field refresh,
   `updated` counter, `last_seen_ko_note`, new fixture, three-way test.
   **Done**; migration `0004_highlight_ownership.sql`. One correction it
   produced: `last_seen_ko_note` is seeded by the *insert* as well as the
   refresh — a row with a `ko_note` and a NULL `last_seen_ko_note` would tell
   the future two-way sync that the device had never said anything, which is
   the one thing that column exists to prevent.
3. `import_book_from_sidecar` + link/merge — offline, no enrichment. **Done.**
   Three things it settled: `UnmatchedSidecar` carries its candidates and its
   device key, so "unmatched" is a decision rather than a report; `link_sidecar`
   needs a *repointing* device-link write (`set_device_link`) that a library
   scan must never use; and `merge_books` lives in `storage/` rather than
   `koreader.rs`, since folding two books together is not the reader's business
   and all SQL belongs behind that boundary.
4. `readings` table + progress migration. Spec: `docs/spec-engine-04-07.md`.
   The progress columns **leave `books`** — no compat mirror — and `Book` keeps
   `current_page`/`finished`/`date_started`/`date_finished` as read-only
   projections of the active reading, which is what leaves every render call
   site untouched. `upsert_book`'s `finished`-merges-with-MAX clause is retired
   with them: it only ever existed because reading state lived on `books`.
5. `partialMD5`. **Erratum:** the device→book mapping table this item was
   written to include (`device_books`) landed with item 1b, and a sidecar
   already supplies its own `partial_md5_checksum`. What remains — and is now
   the whole of the item — is **computing the same hash ourselves over a local
   file**, so our file identity agrees with the device's. Owned files stay
   item 12.
6. Device screen — per-book state, single pull, then multi-select and sync-all.
   **Split in two:** 6a is the engine scan plus the sidecar mtime/size cache,
   6b is the TUI screen. The halves have no dependency on each other, so 6a runs
   beside item 4 instead of queueing behind it.
7. Reflection + Review. Both are **notes with a new `kind`** plus side tables for
   the rating, the Goodreads lookup and the citations — because the reflection
   is the graph hub and `note_links` is already the graph. `notes.kind = 'final'`
   is superseded by `'reflection'`. **Done** (engine + CLI); migration `0007`.
   Its **TUI half was deferred and never numbered** — see item 8.

   *Items 8–10 are specified in `docs/spec-08-10.md`, one prompt per thread in
   `docs/prompts/08a`, `08b`, `09a`, `09b`, `10`.*
8. Currently-reading home screen; action = open the reflection. **Split in two:**
   8a is the engine query (`list_open_readings`, plus a `NoteRecord`-returning
   way to open a reflection — `open_reflection` returns a `CreatedNote`, and
   every editor path in both frontends needs the record), 8b is the screen.
   - **Erratum: 8b absorbs item 7's deferred TUI half.** The action this item
     names does not exist — the TUI has no reflection or review surface at all —
     so item 8 is not implementable without building it, and splitting them means
     two passes over the same four exhaustive matches in `app.rs`.
   - **Done.** Two things the spec was silent about, decided while building it.
     Moving the front door means `Esc` on the **menu** can no longer quit the
     app — it returns home, because a key that leaves the app from a screen one
     keypress in is a trap; `q` still quits from anywhere. And the reflection
     and review keys are **`e` / `w`, not the mnemonic `r` / `v`**: those are
     spent on reset-pose and swap-renderer, where they read correctly, and both
     the home screen and the book view need the pair — so it is global rather
     than a `map_key_on` override two screens would install identically.
   - **Erratum: the front door moves back.** The **menu** is the front door and
     the screen the app opens on; the currently-reading screen stays a screen,
     reached by its menu row, by `--book`, or by opening a book. Single-step
     `Esc` is replaced by a **navigation trail**: `esc`/`b` pop the screen you
     came from, and the menu is the bottom of it. **`esc` never quits, from any
     screen** — `q`, the menu's Quit row and ctrl-c are the only ways out.
9. Backlinks pane. **Split:** 9a is the engine query plus migration `0008`
   (`note_links` has no index on `to_note` or `target_title`, so both a backlink
   query and `write_links`' back-resolution are full scans today); 9b is the
   pane, which attaches to the **note list** rather than adding a `BookTab`.
   - **Erratum, measured while building 9a:** the `target_title` index has to be
     declared `COLLATE NOCASE`. Back-resolution compares `COLLATE NOCASE`, and
     SQLite will not use an index whose collation differs — a bare
     `ON note_links(target_title)` exists, reads right, and is never used.
   - `backlinks` is a plain `WHERE to_note = ?`, with no dangling-by-title
     union: back-resolution keeps `to_note` complete, and the inbound and
     outbound views have to agree about which edges are still only text.
   - **Done.** One gap pinned rather than fixed: `notes.title` is not unique, so
     an edge resolved to one of two same-titled notes dangles again if *that*
     one is deleted. The fix is write-side (re-resolve on delete) and a read-side
     item must not paper over it — the pane would then claim an inbound link the
     linking note denies writing.
10. Goodreads CSV in/out. Migration `0009`.
    - **Trap worth recording before it is hit:** `active_rating_scale()` is
      `ORDER BY created_at DESC LIMIT 1`, so seeding a `goodreads` scale would
      silently hijack what `rating show` and `set_rating` default to. The item
      therefore also adds `rating_scales.is_default`.
    - `Bookshelves` → `book_tags` (inert provenance); Goodreads' `Book Id` →
      a general `external_ids` table, because Calibre (item 13) needs the same one.
    - **Done.** One correction it forced: `Read Count > 1` cannot give the earlier
      readings NULL end dates, because `finished_at IS NULL` *is* what open means
      and `idx_readings_one_open` permits one per book. `started_at` is NULL
      instead — that is what is genuinely unknown — and earlier readings close at
      the date the row carries, a true upper bound rather than an invented date.
    - Known limit: our export carries no `Exclusive Shelf` column, because
      Goodreads' importer does not read one. A round trip into a *fresh* library
      keeps the book and loses the shelf.
    - **TUI half done** (never separately numbered, like item 7's). Deliberately
      *not* a shelf: a CSV is matched against the library as a whole, so there is
      no per-row import to stand on. The screen is what the dry run found, the
      undecided rows last, and `s` applies the lot. `x` keeps its global `Export`
      meaning there, which is the one place a CSV is written.

*Items 11–16 are laid out in `docs/spec-11-16.md` — 11–13 to prompt-ready
detail, 14–16 as constraints to re-plan against.*
11. Wired device watcher.
    - **Done.** `watch.rs`: a debounce over an injected channel of stirs, with
      `notify` as a thin adapter that is the only untestable part. **Scans,
      never syncs** — the module holds no `Storage` at all, so "read-only on
      arrival" is a property of the code rather than a rule about it.
    - The watcher reports **transitions**; a reader already plugged in when the
      app starts is seeded, not announced, because `candidate_mounts` is what
      already answers that question.
12. `book_files` + owned files + three-level dedup.
13. Calibre (i) then (ii).
    - **Done.** No migration and no new dependency: `external_ids` was made
      general in `0009` for exactly this, and the matcher, `partial_md5` and
      `book_tags` were all already here.
    - Three corrections it forced, all from running calibre 7.26 rather than
      reading about it: `authors` is a **`&`-joined string** where every other
      multi-valued field is a JSON array; `pubdate` is **`0101-01-01`** when
      undated, which taken at face value publishes every such book in the year
      101; and `calibredb --with-library <typo> list` **creates a library there**
      and reports `[]` with exit 0, so the path is checked for `metadata.db`
      before the binary runs.
    - One engine correction underneath it: `upsert_book`'s no-ISBN branch is a
      plain insert and ignores `Book::id`, so enriching a book matched by uuid or
      by file hash needed `Storage::enrich_book`. Both statements are generated
      from one rule table, since two definitions of "merge a partial record" is
      how they drift.
    - **Calibre's rating is not imported**, and that is a decision rather than an
      omission: a rating anchors to a review, which anchors to a reading, and
      calibre has no readings. Importing one would mean inventing reading
      history. Series is dropped for the same shape of reason — no column, and
      `book_tags` is for shelves.
    - Tier (iii), device push, remains out of scope.
    - **TUI half done**, as a *shelf* — the device screen's shape, because calibre
      is another system that owns books and the way to meet one is to be shown its
      shelf. It forced four thin engine additions, all of them things the CLI
      never needed because it imports all-or-nothing: `CalibreBookReport` gained
      `calibre_id` (a report line otherwise names its row only by title, and two
      editions of one title tie to each other), `ImportOptions` gained `only` for
      the single-row import `Enter` is, and `link_calibre_book` /
      `link_goodreads_row` finally give the two CSV/library importers the
      `link_sidecar` they never had. Before those, `l` on an undecided row would
      have been a dead end: the only escape hatch was `--new` plus `merge_books`,
      which creates a duplicate on purpose in order to fold it back in and leaves
      the far side's id pointing at whichever of the two the merge deleted.
    - The **rating is still not imported** and the shelf does not offer to — the
      reason is structural (a rating anchors to a review, which anchors to a
      reading, which calibre knows nothing about) and a screen is not where that
      changes.
14. API crate + `readingbuddyd`.
    - **Done.** `crates/api` (`readingbuddy-api`) holds the surface; `crates/daemon`
      is the transport and holds no logic — it never names a method. Zero new
      third-party dependencies: a unix socket with newline-delimited JSON needs
      a runtime and nothing else, where HTTP would have added a server, a router
      and a middleware stack for a protocol with one endpoint.
    - **The domain types stay serde-free.** DTOs with `From<domain>` instead,
      because deriving `Serialize` on `Book` would pick a wire encoding for
      `OffsetDateTime` by accident and make every field name a public promise.
    - `Engine::storage` and `Engine::config` are **private**, with real facade
      methods for every frontend call site that used to reach through them —
      ratings-scale admin, readings and highlight listing had no facade method at
      all. `Engine::storage()` survives behind an `internals` feature for the
      engine's own `tests/`, and CI grew a plain `cargo check --workspace` to
      catch production code using it (clippy's `--all-targets` resolves
      dev-dependencies, so the feature is on there).
    - `set_google_api_key` had to take `&self`: a transport hands one engine to
      several connections through an `Arc`, and `&mut self` on a facade is a
      method no shared owner can call. That moved the live key off `EngineConfig`
      and behind `Engine::google_api_key()`, so nothing reads the seeded copy.
    - **Handles do not cross.** `update_note_body(&NoteRecord)`,
      `delete_note(&NoteRecord)` and `file_path(&BookFile)` become id-taking
      calls that re-read the row — a client echoing a stale `NoteRecord` back
      would otherwise write to a path that had moved.
    - **The mount watcher is deliberately not in the vocabulary**: it is a
      stream, request/response has no shape for one, and wrapping it as a poll
      would give the far side a different debounce from the one `watch.rs`
      guarantees. A subscription is its own design.
    - Known limit, stated rather than hidden: a path crosses as a `String` via
      `to_string_lossy`, so a filename that is not valid UTF-8 does not round
      trip. JSON is UTF-8 and the alternative is base64 on every path.
15. KOReader plugin + wireless push.
16. Everything under *Out of scope for now*, plus collections, the Tauri GUI and
    the Mac/iOS companions.

Items 17–28 are the GUI wave (`docs/gui/spec-gui-17-28.md`). Items 29–32 are
what the engine *keeps* rather than what it acquires
(`docs/spec-engine-29-32.md`), and item 21 was pulled forward into that wave
because item 31 needed somewhere to put reading time.

21. `reading_events`, the source-agnostic activity log — migration `0011`.
    - **Done**, ahead of the rest of its own wave. Three fillers that need no
      device (highlight days, vault days, reading endpoints) plus the period
      aggregates the engine had none of.
    - **The stated grain and the only idempotent key are not the same thing.**
      "A book, a read, a date" is the grain of an *occurrence*; the key is
      `(book_id, day, source)`, and `reading_id` is deliberately outside it. It
      is nullable `ON DELETE SET NULL`, so a key containing it needs a NULL
      sentinel — and then deleting a reading with two events on one day collides,
      which SQLite reports as a constraint error raised by a table nobody
      touched. The read is therefore an *attribution*, the call
      `attribute_highlights` already makes.
    - **`source` names the filler's evidence, not the upstream system**, and one
      token hosts several fillers only because the upsert is a no-clobber merge:
      `COALESCE` on what a filler has no opinion about, `confidence` ratcheting
      `inferred` → `measured` and never back. That is what makes "a later filler
      changes no query" true rather than aspirational, and it is binding on 31.
    - **`updated` had to mean *changed*, not *seen*** — an unconditional
      `DO UPDATE` reports every row on every refill, which makes idempotency
      literally unobservable. Same failure the tier-2 corpus recorded.
    - **"Links created per period" is not answerable as asked.** `note_links`
      carries no timestamp and `set_note_links` replaces a note's whole edge set
      on save, so a link's own creation date is recorded nowhere; a `created_at`
      there would record the date of the last edit to the *note*, dressed as the
      link's birthday. Edges are attributed to `from_note.created_at` and the
      doc comment says so.
29. Field provenance — migration `0012`.
    - **Done.** "Authority is per-field, provenance is recorded" has been in this
      document since item 10 and the second half was not true;
      `field_provenance(book_id, field, source, fetched_at)` makes it true, and
      **`MERGE_RULES` generates it** rather than a parallel list — the same table
      that already generated the upsert's `ON CONFLICT` and `enrich_book`'s
      `UPDATE` now generates `merge_books`' fill and the field set each stamps.
    - **`save_book` stamps nothing, deliberately.** It receives a record already
      flattened by `search::merge_provider_books`, which keeps the winning value
      per field and discards which provider supplied it; naming one would invent
      exactly what the table exists to record. Item 30 is where the provider half
      becomes answerable, because it merges provider by provider.
    - **`import_epub` had two origins in one row** — the file's metadata folded
      onto the provider's by a hand-written `is_none()` per column, which was
      this merge spelled a second time and which no single stamp could describe.
      Two writes now, through `fill_book`.
    - **`merge_books` was already a second hand-written copy of `MERGE_RULES`**,
      in the one statement where a forgotten column loses data rather than
      failing to merge it. Generated now.
    - **Provenance travels with the value, not the row.** The obvious
      `UPDATE … SET book_id = dst` would stamp `dst`'s kept value with the source
      of `src`'s discarded one, for every book older than the migration.
    - **No back-fill, and that is the decision.** Every signal that might
      attribute an existing row (`openlibrary_key`, `googlebooks_id`,
      `external_ids`, `device_books`) records who was *consulted*, not who
      supplied the field beside it. An absent row means unattributed, which every
      caller has to handle anyway.
    - Disagreement history — `(book_id, field, source, value, fetched_at)` — is a
      real feature and a **second table beside this one**, not a wider key here:
      a per-source row without a *value* column records who was asked, not what
      they said.
31. Reading time, from the device's own `statistics.sqlite3`.
    - **Done**, as one more filler of item 21's table — day, minutes, pages,
      `source = 'koreader'`, `confidence = 'measured'`. Its own module, not part
      of `device.rs`: it reads SQLite rather than Lua, joins `device_books`
      rather than matching, and **writes**, which is the thing `scan_device` is
      defined by not doing. Not on `sync_device`'s path.
    - **The database is WAL.** Copying only the main file reads the state as of
      the last checkpoint and can silently miss an entire recent session, with a
      plausible number where the right one belongs; the copy takes the `-wal` and
      `-shm` siblings, and the test holds a connection open across the import
      because a clean close deletes the WAL and would prove nothing.
    - **"`stats.md5` does not exist" is a fact about the *sidecar's* `stats`
      subtable, and it was read here as a blanket claim.** The statistics
      database carries `book.md5 = util.partialMD5(file)`, so this join is exact
      rather than fuzzy — the next reader of that line would otherwise have built
      a title matcher.
    - **The `page_stat` VIEW rescales pages onto the current page count** with
      integer division and multiplied rows. It looks like the convenient thing to
      read and is wrong on both columns; the raw table is read instead.
    - **A measured twenty-second session records `Some(0)`, not `None`.** The
      device is saying something; `None` is reserved for days nothing measured,
      which write no row at all.
    - **The day skew is left unfixed, on purpose.** A sidecar's `datetime` is
      zoneless local wall clock while `start_time` is a real epoch, so off UTC a
      session near midnight lands on adjacent days from the two fillers.
      Correcting by this machine's offset would make an import's result depend on
      where the laptop is, and re-importing after a flight would rewrite history.
      The cost is bounded to a stray `inferred` row, never a wrong minute count.
    - **The schema is source-derived and still unverified against hardware.**
      `KNOWN_SCHEMA_VERSION` gates it, so an unknown version imports nothing and
      says why rather than guessing. `docs/koreader-format.md` ranks the source
      above a fixture, which is what made building without a device legitimate
      rather than a shortcut.
30. Look a book up again — `rb enrich`, and `rb set` to answer it back.
    - **Done.** `Storage::enrich_book` existed and only calibre called it, so
      every book created without an ISBN — a sidecar pull, a file matched by its
      stem — had no cover, description or page count, permanently.
    - **Attribution is reported, not applied provider-by-provider**, and that is
      the item's central decision. Applying each provider in turn gets the
      attribution right and the *values* wrong: no-clobber means whichever
      provider runs first wins every field it speaks to, so OpenLibrary's
      description would beat Google Books' — a second dialect of `search.rs`'s
      tuned per-field preference. `search::FieldClaims` carries the per-field
      attribution the merge always computed and threw away.
    - **`download_cover` created a duplicate row on exactly this item's path.**
      It wrote back through `upsert_book`, whose no-ISBN branch is an
      unconditional insert that ignores `Book::id`, so a stored sidecar-seeded
      book got a *second row* instead of a cover. Reachable from `fetch_cover`
      before this item existed.
    - **A dead network read as a fact about the book.** `NoAnswer` separates
      "no provider knows it" from "no provider answered", and refuses the former
      when one of them never spoke.
    - **The override is `rb set`, not a `--new`-shaped flag.** `--new` means
      "create a book" in three other commands and there is no book to create
      here; `--accept 2` would index into a candidate list the next run cannot
      reproduce. `set` also makes `field_provenance`'s `user` reachable from
      outside a test, without which item 29's guard protected nothing in
      production.
    - **No ISBN → title fallback.** An ISBN is an identity, so "no provider knows
      this ISBN" is an answer about *this edition*; a title search answers a
      different question and writes another edition's page count over an
      edition-specific one.
    - **Disagreement history still does not earn its place** (agreeing with 29):
      re-asking makes the *value* half of the case, since `held` is useless
      without the offered value, but the report carries that in-band and nothing
      yet compares across runs.
32. Subjects, series, and the chapter list — migration `0013`.
    - **Done.** Three fields nothing captured. `subjects` merges as a set,
      `series`/`series_index` as a guarded pair, and the chapter list is derived.
    - **A claim protects a field *pair*, not a field**, and this is the item that
      settled the question item 30 left open. Owning either half of
      `series`/`series_index` or `isbn_13`/`isbn_10` holds both. The fix is a
      **guard rather than a new kind of claim**: the incoherence is only ever
      introduced by a write to the *unowned* half, so guarding the pair off
      either claim is strictly weaker than "a claim with no value" and
      sufficient. **The ISBN instance is fixed by this item.**
    - **A held value and a granted claim is a half-protected field.** The guard
      lives in two places from one rule — the merge clause and the stamp —
      because a column held by its *partner's* claim has no `user` row of its own
      for `stamp`'s `WHERE` to catch. And a held field must be *reported* held,
      or a held `series_index` is indistinguishable from a provider that had
      none.
    - **Sets merge by replacement, never union.** `field_provenance` holds one
      source per field, so a unioned value would name one of two origins for a
      value that came from both. Union is unrepresentable here, not untidy.
    - **A provider subject is not a collection**: it is a bibliographic fact with
      an origin and merges like `publisher`. `book_tags` stays minted shelves.
      Stored **raw** — a controlled vocabulary is not answerable from two
      examples, and storing raw is what makes it answerable later against real
      data.
    - **A chapter list is derived from the owned file and never stored**, and the
      deciding argument is not staleness: it has no *origin*. It is a pure
      function of a sha256, so a provenance stamp naming `epub` would attribute a
      value nobody claimed and `rb set` could not let the user own it. `None` is
      no readable file; `Some([])` is an epub with no TOC.
    - **Series is not read from Google Books** — `volumeInfo` has no documented
      series field and `seriesInfo` is absent from the API reference, so the only
      possible fixture is one written from memory. Calibre stopped dropping
      series; its `series_index` is `1.0` on a book with no series at all, so the
      index is read only where the name is.
    - **Recorded, unfixed:** `search::merge_into` is a fourth consumer of
      `MERGE_RULES` that is *not* generated from it, so a new column compiles
      cleanly and is silently lost in federated search. Only
      `every_claimed_field_is_a_merge_column` catches it — and it did.
