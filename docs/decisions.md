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
      **Fixed by the surfacing item below.**

33. **Surfacing 21/29/30/31/32 — the CLI and API the wave did not build.** Five
    engine items had landed with no way to see any of them: no CLI, no DTO, no
    request. This is that surface and nothing more — no new capability, no
    migration.
    - **`search::merge_into` is now generated**, closing the debt above.
      `Rule::federated` is the sixth thing `MERGE_RULES` produces, and it makes
      the arrangement total: a new column does not compile without saying how it
      merges in a federated search. The variants carry their own setter, so a
      column that is never merged (`sort_title`, `cover_path`) has no dead
      function pointer beside it — and `only_our_own_columns_sit_out_the_federated_merge`
      names those two, so `Local` cannot become the quiet way to skip the
      question. The merge itself moved to `books.rs` beside the table;
      `search::merge_into` is now the claim bookkeeping and nothing else.
    - **`subjects`/`series`/`series_index` cross the wire.** They were an
      asserted gap in `BookDto` and are now three fields, so timestamps are the
      only field the DTO round trip deliberately drops. `rb set` gained
      `--subject`, `--series` and `--series-index`.
    - **Half a pair is refused, at the frontend and only there.** `rb set
      --series-index` with no series — neither on the flag nor on the row — is
      an error naming the move, because a number under no name is the incoherent
      state `Rule::pair` exists to prevent. It is checked against the *stored*
      series too: renumbering a book already in a series is the ordinary use of
      the flag. The API deliberately does **not** enforce this — a rule
      implemented at the seam is a rule the in-process caller never meets, which
      is the argument `crates/api/CLAUDE.md` already makes about `dispatch`.
    - **`Book::series_label` is a derived fact, so it is the engine's.** `Dune
      #2` is one fact printed on one line; the *phrasing* is a frontend's but
      deciding what the pair means together is not, and `series_index` is a REAL
      so two frontends formatting it themselves would eventually disagree.
      `series_index_text` is public for the one case with no series beside it.
    - **`rb activity` says "not measured", never `0`.** A library that arrived as
      a Goodreads CSV has no minutes; the CLI prints `—` in a column and the
      words in a total, and `nothing_measured_is_never_printed_as_zero` pins it.
      Nothing counts what the user has not done — no streak, no "4 of 30" — and
      the test asserts those words are absent rather than trusting the author.
    - **An empty log names the move.** No importer fills `reading_events`, so a
      fresh library reports nothing and cannot tell that from "you did not
      read". `readingbuddy activity --refill` is named in the output rather than
      being a verb somebody has to find.
    - **`ko stats` is its own verb**, matching the engine: arrival is read-only,
      and a scan that quietly imported months of timing data would not be
      read-only in spirit. It prints `books_in_db` against `books_matched` even
      when nothing imported, because "no statistics database" and "none of these
      books are yours" need different next moves.
    - **`rb toc` has three answers, not two** — no readable file, an epub with no
      TOC, and the list — for the reason item 32 gave: collapsing the first two
      makes an ordinary EPUB3 book look like a missing file, with no move for
      either.

17. **The derived-facts layer.** No migration; projections and pure functions
    over columns that already existed. Its subject is a line the codebase had
    over-applied: *the engine does no terminal I/O* is right, and it had been
    read as *the engine does no derivation*, so each frontend independently
    wrote the sorting, the arithmetic, the state vocabulary and the row-state
    joins. A second frontend was not extending the app, it was re-deriving it.
    - **`Progress` is one value type and there were four implementations.** They
      disagreed. The CLI had no `total > 0` guard, so `make dev-db`'s
      zero-page-count book printed `[12/0]` — a **false denominator**, not a
      crash, which is why nothing caught it. `Progress` normalises a zero length
      to *absence*, so a caller cannot reach one; a `NULL` length is absence
      too, and neither is zero.
    - **Pages win where they can answer; the device fills in where they
      cannot.** The `ko_percent` fallback existed in one screen of one frontend
      (`tui/src/ui/home.rs`), which meant every list built off `Book` alone —
      every GUI list — showed nothing for the commonest row in a
      KOReader-sourced library. Deciding between the two is domain knowledge and
      is now decided once.
    - **`Book` gained a sixth reading projection, `ko_percent`.** Not a schema
      change — `readings.ko_percent` has existed since `0005`. It is what makes
      `Progress` computable from a `Book`, which is what a list needs.
    - **`percent` crosses the wire beside `fraction`, and is not redundant.**
      The page-based percentage is an *integer division*; `29/100` is
      `0.28999999999999998` in binary, so flooring the float says 28 where the
      division says 29. Two frontends already did it in integers.
    - **`ReadingState` is typed and `readings.status` stays a `String`.** The
      argument for the string was about *storage* — an importer can write a
      status this build does not know, and a parse that refused one would turn a
      foreign device's vocabulary into an error on the read path. It was never
      an argument for handing every frontend the same `switch` over three magic
      words. `Other(raw)` carries the unknown value whole, the shape `KoStatus`
      already had.
    - **There is no `NeverOpened` variant, and that is the ruling.** A book with
      no reading is absence, not a state. Naming it puts *unread* into the type
      system, and a variant is a thing a UI filters on, counts, and eventually
      puts a badge beside — which is the completion framing this document bans.
      Absence is also honest: the engine knows there is no reading, not that the
      book has not been read.
    - **Author names moved into `readingbuddy::names`.** `last_name` and its
      particle/suffix tables were the TUI's private knowledge; a GUI without them
      files *The Overstory* under nothing. `display_order` is the same reading of
      the comma run backwards, and the two share `comma_is_only_a_suffix` so the
      sort and the label can never name different surnames — asserted as a
      property, not by example. The **join** between names stays phrasing.
    - **`BookSort` gained `Author` and `Year`, and `limit` selects along the sort
      key in every arm.** That is what `LIMIT` means. The TUI's opposite policy —
      fetch one page by recency, reorder *that page* in Rust, so pressing `s`
      reorders the list rather than swapping its contents — is a decision about a
      fixed page and stays in the TUI. The two coexist; mistaking one for the
      other is the bug. `Author` cannot be an `ORDER BY` at all (SQLite has
      nothing to parse a human name with), so it reads the library, sorts, and
      truncates. A `sort_author` column is the follow-on and needs a migration,
      so it is not this item's.
    - **`CalibreReport::row_state` and `CalibreRowState::is_importable` moved
      down** (17d/17e). The state was a join across three fields of the report,
      done by a frontend; the predicate's twin, `DeviceState::is_syncable`, was
      already in the engine, which is what made its absence conspicuous.
      `Candidates` is still not importable: that row is a question, and a sweep
      answering it by creating a duplicate is what the candidate band exists to
      prevent.
    - **`ReadNumbering` moved down** (17c). It silently depended on
      `list_readings`' oldest-first ordering contract, so a second frontend
      numbering reads off a differently-ordered list would disagree with `rb
      show` about which read a highlight came from — with nothing on either
      screen looking wrong.
    - **Decided to stay in the frontend, explicitly.** *Dates*: nothing was
      moved. Relative time needs an answer to "what is today", the engine's day
      convention is UTC, and inventing a local-time answer is what item 31
      deliberately refused for reading minutes — the same refusal applies.
      *Absence wording*: the engine states the absence (`title` is `NULL`,
      `authors` is `[]`) and the frontend words it — *Untitled* is a word.
      *`ReadingDto.source`*: still a `String`. It is the name of a writer, it
      grows by one per importer, and nothing branches on it; an enum would be a
      second list of importers to keep in step with the first.
    - **A per-row summary of what is behind a book — highlights, notes, a file —
      is item 18's, not this one's.** It is a query shape, not a derivation: the
      detail screen makes four calls for one book, which for a list is eight
      hundred. Note the axiom line it sits against — a count of *your own
      highlights* is past tense and allowed; a count of what you have not done is
      not.
    - **Two behaviour changes worth knowing.** The TUI library list now shows a
      device percentage where it previously showed nothing, because the fallback
      is no longer home-screen-only. And `rb book list` prints `[p.12]` where it
      printed `[12/0]`.

20. **Covers a grid can actually use.** Migration `0014`. Its subject is a live
    bug that three handoffs carried, and it is the reason the item went first.
    - **Every Google Books cover collided on one filename**, and nothing in this
      repo could see it. `filename_from_url` named a file from the URL's last
      path segment; a Google Books thumbnail is `.../books/content?id=…`, so the
      last segment is the literal string `content` for every cover Google has
      ever served. Every GB-sourced jacket in a library wrote
      `images_dir/content` and the last import won — two books rendering each
      other's cover, permanently, with nothing on either screen looking wrong.
      Epub extraction collided the same way on `slugify(title)` (two editions of
      one book), and the `"cover.jpg"` fallback made a third reachable. Invisible
      in a single-provider library, and `make dev-db` generates its own covers,
      which is exactly how it survived.
    - **Named by content hash**, the pattern `book_files` set in `0010`, with the
      extension read from the bytes rather than from the URL or from the epub's
      declared mime type. The path-traversal guard is **kept and re-argued
      rather than deleted**: what changed is that both halves of the name are now
      closed sets, so the property it asserts is provable instead of true by
      accident of what `Url::parse` normalizes.
    - **Two things content addressing bought, and one it broke.** The write is
      idempotent and skip-if-present is free. But two books can now legitimately
      share one file, and both `delete_book` and `merge_books` used to hand the
      caller a path to unlink on the assumption that nothing else held it — which
      would take a surviving book's cover with it. Both now ask the database.
      `book_files` gets to skip that check because it is keyed on the sha alone
      and a second holder is unrepresentable there; `books` holds a plain column
      and a second holder is ordinary.
    - **`cover_width`/`cover_height`/`cover_accent`/`cover_thumb_path` are not
      `MERGE_RULES` rows**, and that is the decision this migration exists to
      record. `MERGE_RULES` governs what a *record* can carry, with a source to
      attribute; no provider publishes a width, and the only honest answer to
      "where did this come from" is "we decoded the file". A `Federated::Local`
      row would have said that in the table's vocabulary and would then have
      handed every record-shaped writer a way to move one of them **without**
      moving `cover_path` — a row whose stored dimensions describe a different
      image, which nothing downstream can tell from a correct one.
    - **`Rule::pair` is not that fix, and the reasons are worth keeping.** It is
      binary and asserted symmetric, where this is a star of four columns around
      a fifth; it guards against a *user claim* rather than against incoherence;
      and `merge_books`' fill — the one place the incoherence was genuinely
      reachable — runs with the user guard off, so the pair guard would never
      have fired there at all.
    - **One writer was not enough, and the test said so.** `Storage::set_cover`
      writes all five columns or none, but `cover_path` *is* a `MERGE_RULES`
      column with three other writers, and `Merge::Coalesce` under
      `Winner::Incoming` means an incoming record's path wins — so a calibre row
      or an `rb set` carrying a cover repointed the row and left the previous
      jacket's measurements behind it. `merge_set` now also generates
      `invalidate_cover_metrics`: a write that moves the path sets the four back
      to NULL, from the same expression that stores the path, so it cannot fall
      out of step with the merge rule, the user guard or the `dst`-wins
      inversion. Between the two halves the bad row is unrepresentable rather
      than guarded against.
    - **`set_cover` replaces where every other writer coalesces**, which fixed a
      latent bug rather than introducing an exception: `download_cover` went
      through the no-clobber merge, so a re-fetch producing a *different* file
      left the row pointing at the old one. Under URL naming the new file usually
      had the same name, so nothing ever looked wrong.
    - **The accent is stored unclamped.** `render3d` pushes it into a legible
      luma band so a white jacket still reads as a board; that band is a
      renderer's policy about its own lighting, not a fact about the file. The
      engine stores the measurement and each frontend decides what it can see —
      the same line item 17 drew. The border-median arithmetic is briefly
      **duplicated** between `images.rs` and `render3d/texture.rs`; item 19 owns
      the renderer and deletes its copy when it rewires onto the column.
      *(Superseded by entry 39. Item 19 shipped and did not delete it; the copy
      survived three handoffs on a second justification — that the two measure
      different images — which was also false. The sentence is left as written
      because this file is a build-order record rather than a live spec, and a
      claim quietly edited to be true is a claim nobody learns from.)*
    - **Both halves of 20c are one change.** OpenLibrary `-M` → `-L`, Google
      Books reads all six documented `imageLinks` sizes largest-first — and a
      shelf tier is generated locally, because asking for the large size
      *without* one makes every list strictly heavier than it was. `edge=curl`
      is stripped: it composites a page-curl graphic onto the jacket, which is
      somebody else's decoration baked into bytes we store for ever and take the
      border colour off.
    - **The back-fill is a command, not SQL**, because SQLite cannot decode a
      PNG. `Engine::measure_stored_covers` reads each file and writes through the
      same `set_cover` the download path uses, so a back-filled row and a fresh
      one are the same row. It does **not rename**: the stored `cover_path` is
      what a webview resolves, and renaming every image would be a destructive
      change dressed as a measurement. `cover_width IS NULL` therefore means *not
      measured yet*, never *no cover*.
    - **`sort_author` was considered and deliberately not carried**, though this
      item owned the wave's only migration and item 17 had deferred it for
      exactly that reason. It is not one migration: SQLite cannot compute the
      value, so the column is NULL for every existing row and `ORDER BY
      sort_author` is silently **wrong** until a back-fill command nobody has run
      yet runs — and the fallback for that window is `list_books_by_author`, the
      whole-table read the column exists to remove. Carrying it adds an arm
      beside the slow one instead of replacing it. The write side is now cheap
      (`invalidate_cover_metrics` is the pattern: a companion clause generated
      from another column's value expression), which makes it a smaller item than
      it was, not a reason to bolt it onto one whose subject is images.
      **`sort_title` is the warning**: it is a `MERGE_RULES` column,
      `Federated::Local`, on `Book` and on the DTO — and nothing in this repo has
      ever computed it. `BookSort::Title` orders by `books.title COLLATE
      NOCASE`. A sort-key column added without a writer looks answered and is
      not.
    - **A `covers` table keyed on the content hash was the other design, and it
      loses.** The spec's own argument — a shelf query needing a join per row —
      is the weakest one available, since a single `LEFT JOIN` against a primary
      key is not what makes a list slow. The real argument is ownership: a shared
      `covers` table makes the sharing explicit and then owes an answer for
      reference counting, garbage collection, and what `merge_books` does to a
      row two books point at. That is a lifecycle, in a codebase whose one rule
      about ownership is per-field provenance on `books`. Four columns beside the
      path they describe need no lifecycle at all — and the invalidation clause
      above is only expressible *because* they sit in the same statement as the
      path.
    - **The thumbnail tier is on disk, not in the database.** It is a sibling of
      the cover, named from the same hash, so `MergeReport` did not need a second
      orphan field and a caller holding a `cover_path` can always derive it.
      `Book::shelf_cover_path` is where "thumb, else the original" is decided —
      once, because a frontend reading `cover_thumb_path` directly shows nothing
      for every cover small enough not to have one.
22. **Reading here: the local source.** No migration. A PDF metadata reader, a
    fifth `reading_events` filler, and one word that was argued for and not
    added. Most of the item already existed — `book_files` takes any format,
    `import_file` copies bytes in content-addressed and already refuses to
    create over a candidate, `notes.page` already named the case.
    - **`source = 'local'` was NOT added to `readings`, and that is the item's
      main correction.** The vision doc's fourth ownership row (*readingbuddy
      owns what you read here*) and the schema disagreed, and the schema is
      right. `readings.source` names the **writer of the row**: `koreader` is
      the sidecar importer, `goodreads` is the CSV importer, `migrated` is
      migration `0005`, `manual` is a person typing a number here. Attaching a
      PDF opens no reading, and typing a page opens one through
      `update_progress`, whose writer is a person typing a number here — which
      is what `manual` already means. `local` on that column would have been a
      **synonym**, and a synonym is worse than nothing: `readings_from_source`
      is the query every importer's idempotency rests on, and it would then
      have had to know both words.
    - **The word it did earn is `reading_events.source`**, where the vocabulary
      is *claimants* rather than writers (`koreader` = the device said so,
      `vault` = a note said so). "The user typed a page here today" is a
      genuinely new claimant, and it is a claim `koreader` and `manual` cannot
      make on that user's behalf. The primary key `(book_id, day, source)` is
      what makes two claimants two rows rather than a fight over one, so a
      `koreader` reading whose page you corrected this afternoon carries both.
    - **Attach does not open a reading.** The spec said "a `source = 'local'`
      reading opened on attach"; opening one there would mark five newly
      attached PDFs as five books you are currently reading, which is
      fabricated reading state of exactly the kind `attribute_highlights` and
      `ko_statistics` refuse elsewhere. A read is earned by a typed page.
    - **A failed page-count extraction writes NULL, and lopdf makes that
      concrete rather than theoretical.** Its `extract_page_count` returns `0`
      from seven separate "could not tell" branches, and from a
      password-protected file, as well as from a document with no pages. `0` is
      normalised to absence once, at the boundary in `pdf.rs`, because a false
      denominator is the thing item 17 spent an item removing and nothing
      downstream can tell one from a real one.
    - **Two of three real PDFs return `Some("")` for `/Info /Title`.** Measured,
      not assumed. So emptiness folds into `None` at the same boundary — a
      `Some("")` survives every `Option` idiom and lands as a book with a blank
      name. A title that still carries an authoring-tool extension (*Microsoft
      Word - kant_final_v2.doc*) is also refused, and the filename stem, which
      `files.rs` already uses for every unreadable format, is the fallback. The
      rule is deliberately narrow: a general "looks like a filename" heuristic
      throws away books called *Sync* and *Java*.
    - **The page count merges as a partial record, not as an origin.**
      `fill_book` (*the stored row wins*) rather than the device's straight
      assignment, on `calibre.rs`'s rule that the pattern follows whether the
      record is complete. A page count already on the book is a claim about a
      specific edition; an attached PDF is one more partial record beside it.
      So it fills a gap and never overwrites an answer — including the user's,
      which `field_provenance`'s `user` rank holds against every source.
    - **`Source::Pdf` is beside `Source::Epub`, not folded into a `File`.** They
      answer different questions — an epub supplies a title, authors, a language
      and an ISBN and never a length; a PDF supplies a length and occasionally a
      title and never the rest — and "which file said 512 pages" is precisely
      what a reader of that table is asking.
    - **A day's `pages` accumulates; it does not replace.** `EVENT_MERGE` is
      `COALESCE`, so writing the evening's delta over the morning's would lose
      the morning silently. Three related refusals: the *first* typed page
      claims no pages at all (a delta needs two points, and "you are on page 42"
      is not "you read 42 pages today"), a correction downwards is not negative
      pages, and re-typing the same number touches no row — which is what keeps
      idempotency observable through `EVENT_DIFFERS`.
    - **The typed page is filed by `update_progress` itself**, not by a
      frontend. Two frontends each remembering to log is two frontends that
      eventually disagree about whether today counted, which is item 17's whole
      argument applied to a write.
    - **The licence gate ran before the crate was chosen, not after.** `lopdf`
      `=0.44.0`: MIT, whole transitive tree permissive, no new `deny.toml`
      exception. That matters more here than elsewhere — the engine already
      links GPL-3.0 `epub`, and a second copyleft reader would make that worse
      rather than merely unchanged. `pdf` 0.10 (pdf-rs, also MIT) lost on
      weight. Pinned exactly for `epub`'s reason: this is a *metadata* surface,
      and metadata is the surface `epub` broke in a patch release. The cost is
      visible and accepted — lopdf sits on the `digest` 0.11 line where the
      engine is on 0.10, so `cargo deny check bans` gains duplicate-version
      warnings for `sha2`, `md-5`, `digest`, `rand` and `syn`.
    - **Migration `0005`'s vocabulary comment cannot be corrected.** It has said
      `manual|koreader|migrated` since before item 15 added `goodreads`, and the
      `migrations` CI job refuses a modified migration — correctly. So the
      vocabulary now lives in `Reading::source`'s doc comment, beside the type
      every reader of the column goes through, and the SQL comment is a
      historical note about what the list was in `0005`. Any future item told to
      "extend the comment" should extend that one.
    - **Not built, deliberately.** No embedded PDF viewer (out of scope by the
      vision doc). No fabricated highlights — a locally-read PDF has none, and
      KOReader probably cannot supply them either, since `entry_to_highlight`
      requires a string `pos0` and PDF sidecars store a table there; that stays
      *unobserved* in `docs/koreader-format.md` and wants a real PDF sidecar in
      the corpus before anything is built on it. And no CLI or TUI surface,
      which `files.rs` has never had.
    - **The one gap item 22 opened and closed: per-reading progress on the
      wire.** `ReadingDto` had `current_page` and no `progress`, so a screen
      showing a *named* reading had only `BookDto::progress` — which is
      `Progress::of_book`, the **current** read's numbers, printed under an
      older read's heading on any reread. `Progress::of_reading` had existed
      since item 17 and was reachable only by a frontend that links the engine,
      so the GUI's only move was `current_page / page_count` above the API: the
      row-state derivation `gui/CLAUDE.md` bans, and all three hazards
      `Progress` was built to remove. The pairing is a derivation and now lives
      in the engine (`Engine::readings_with_progress` and its two siblings),
      because `readings` carries no length; `From<Reading> for ReadingDto` was
      **removed** rather than made to guess, since filling the field with
      `of_reading(&r, None)` would report "no percentage" for every book whose
      length is known.
    - **Two gaps found and deliberately left as later items.**
      `MatchCandidate` keeps only a title and a score though `koreader::band`
      is holding the whole `Book`, so the chooser a refusal leads to — the
      screen where *which Dune is this* is the entire question — cannot show an
      author without an N+1 `get_book`. And `reading_events` is per
      `(book, day, source)` with no per-book day aggregate, so a log that shows
      one row per day with the device's and your own claims summed would be
      doing arithmetic above the seam. Neither blocks item 22; both are narrow,
      migration-free engine items.
24. **Vault coherence.** No migration. `Engine::refresh_note_from_disk` had a
    facade method, a wire request and a test since item 7, and nothing had ever
    called it — no frontend issued one and nothing watched the vault, so a note
    edited in Obsidian was a note `notes_fts` could not find, indefinitely.
    Item 27's search box is what makes it visible, in the worst shape the
    failure takes: the box does not look broken, the note looks gone.
    - **The ruling: watch → refresh directly, not watch → notify → the frontend
      issues `RefreshNoteFromDisk`.** `VaultWatcher` holds a `Storage` and does
      the write, which departs from `watch.rs`'s *it may scan; it may not sync*.
      The argument is that the rule was never about watchers writing — it is
      about **consent**. A mounted reader is somebody else's disk and a cable is
      not permission to modify it; the vault is ours, in our own data directory,
      and the write is a *derived index* catching up with the file that was
      already the origin of its content. Re-deriving a cache from its source is
      not a sync. So the rule is preserved by being restated about the thing
      each watcher watches: **`MountWatcher` never writes to a device;
      `VaultWatcher` never writes to the vault** — asserted by
      `never_writes_to_the_vault` over a whole tree, and everything the watcher
      can do to the database is recomputable by `reconcile_vault`, which is what
      makes the departure cheap to be wrong about.
    - **Why notify-the-frontend is worse, in four parts.** The daemon has no
      push channel: every reply carries the id it answers, and a server-initiated
      frame has no id to carry — so that design's first cost is a wire-protocol
      change in a crate the item does not own. **The CLI could not participate at
      all**: every command is its own process and there is no loop for a
      notification to arrive in, so the CLI would have been permanently stale.
      The refresh is not "call a method" — it is *decide which note this path is,
      decide whether an absence is a deletion, decide whether to trust a file
      still being written* — and three frontends re-deriving those three
      decisions is precisely item 17's finding. And the failure mode is the worst
      available: a frontend that forgets to wire it has a stale index and **no
      symptom**. What the alternative buys — a storage-free watcher — buys
      nothing, because the write it avoids is not one anybody needs to consent to.
    - **No task is ever spawned, and that answers most of the objection.** Both
      watchers are pull-driven: nothing happens until the caller polls `next()`,
      and for the vault that includes the write. So there is no background writer
      racing a foreground import — the refresh runs on the caller's own task and
      takes the pool the ordinary way — and a `select!` arm whose handler is
      empty is still doing the whole job, because polling *is* the work.
    - **The push-back was considered and is half right.** Refresh-on-read plus
      refresh-on-search is cheaper, has no background task and no platform
      surface, and it is what the **CLI** actually does. It is not sufficient on
      its own, for a reason beyond "search reads the index": `note_links` is a
      *cross-note* index. Adding `[[B]]` to note A in Obsidian changes B's
      backlinks, and no read of B will ever notice, because B's file did not
      change. Refresh-on-read is structurally blind to it. So both were built,
      and each covers what the other cannot: **`reconcile_vault` is the sweep**
      (a `stat` per note, a read only where the file is newer than the index),
      because a watcher only ever sees the present and the ordinary case is a
      note edited on Tuesday and searched for on Thursday; the watcher is the
      liveness, for an edit made while the app is open.
    - **The three races.** *We wrote it*: not a loop and unable to become one,
      because the only thing written is the database and the database is not
      watched — the echo is one event deep however many notes are saved. It does
      not even cost a transaction: `reindex_from_body` compares the file against
      what is already indexed, which also absorbs the far commoner event of an
      editor rewriting a file on focus loss without a character changing.
      *A partial write*: the debounce first (`VAULT_QUIET` = 400ms, short
      because the thing waiting on it is a search box), and explicitly **not**
      claimed sufficient, since a write slower than the quiet period lands inside
      it — so `settled_read` stats either side of the read and re-arms when the
      stamps moved. The remaining hole is stated: a write changing neither length
      nor mtime between the two stats is indistinguishable from a file at rest.
      *Deleted and recreated*: **absence is never destructive**. `Vanished`
      writes nothing. Other tools move files (Obsidian's `.trash/`, a `git
      checkout`, a sync client resolving a conflict); deletion already has an
      explicit path that removes row and file together; the row holds more than
      the file ever did (`book_id`, `reading_id`, page, location, citations, and
      every *inbound* edge); and the asymmetry settles it — believe a deletion
      wrongly and something is gone, believe a persistence wrongly and the user
      sees a hit for a note whose file moved. Recreation then needs no case of
      its own, which is the ruling's real payoff.
    - **Unclaimed files are not adopted.** A markdown file under the vault that
      no note row claims is left alone. Adopting one would have to invent its
      book, its kind and its anchor out of whatever an editor left lying there.
    - **`notes_fts` cannot have triggers, and that settles the question item 24
      was told to ask.** A trigger copies between tables, and the note body is
      **not in the database** — `notes` has no body column, and `notes_fts` *is*
      the only copy. There is nothing for a trigger to read. Making triggers
      possible would mean storing every note body in a second column beside the
      index, i.e. making the vault a cache of the database rather than the other
      way round, which inverts the ownership `docs/decisions.md` states. Not an
      unallocated migration — a non-option.
    - **`notes.title` is still not unique, and was not made so.** Item 9 pinned
      that deliberately; a uniqueness constraint would have made this item's
      lookups easier and would change what the vault permits, so it stays.
      Relatedly and newly documented: a **derived title is the note's first six
      words and is indexed beside the body**, so an outside edit leaves the old
      words findable through the title. That is not staleness — the title is the
      `[[wikilink]]` target, and re-deriving it from an outside edit would
      silently repoint every backlink in the vault.
    - **Two pre-existing bugs the build surfaced.** `create_note` indexed the
      body it was *handed* rather than the one it *wrote* — they differ by a trim
      and a newline, so no note was ever byte-identical to its own index, and the
      first thing to compare the two would have re-indexed the entire vault while
      looking correct. And `refresh_note_from_disk`'s FTS write and link write
      were two transactions, so a cancelled refresh could leave a note whose
      searchable body and whose graph edges came from different versions of the
      file, with nothing on either side looking wrong; `Storage::reindex_note`
      is now one transaction.
    - **Nothing counts what is out of sync.** `VaultReconcile` is past tense and
      for the engine's own log and its tests. There is no "3 notes out of sync"
      anywhere, and the TUI's watcher arm shows nothing at all: an index quietly
      being right is not news, and announcing it would be a notification about a
      chore the user did not have.
19. **The shape of an edition.** No migration; four lines of arithmetic moved out
    of `crates/tui/src/render3d/mod.rs`'s `Model::new` into
    `readingbuddy::edition`. The second instance of item 17's rule, and the one
    that names its second half: *a `Progress` enum is not terminal I/O; `"p.42"`
    is* has a twin in *proportions are not rendering; a Bézier spine highlight
    is*. The question `Model::new` was answering — what shape is this edition —
    is asked by a WebGL shelf and a Unicode-glyph book alike, and two frontends
    answering it separately is a shelf that contradicts a book view about the
    same object with nothing on either screen looking wrong.
    - **The type is `EditionShape`, and it speaks in multiples of the book's own
      height.** *Edition*, not book or work: page count and cover art belong to a
      printing, and two printings of the same novel are different objects on a
      shelf. Not `Extent` — half-extents are a graphics word, and importing the
      renderer's vocabulary into the engine is the coupling this item exists to
      remove.
    - **`HALF_HEIGHT` deliberately did not travel, and that is the whole
      design.** A scene constant is one ratatui camera rig's idea of how big a
      book is; had the engine handed back a number that only meant something
      inside that rig, the arithmetic would have moved and the decision would
      not. Height is `1.0` and is not a field. Millimetres were the alternative
      and were rejected: we do not know an edition's real dimensions, and
      deriving "152mm" from a cover *image's* aspect ratio is inventing a
      measurement — a number wearing a unit it did not come by is worse than an
      honest ratio.
    - **The clamps are the engine's, and the line is proportions against look.**
      They are open to the charge of being aesthetic, and the charge is half
      right. The ruling: the object's *proportions* are the engine's, because
      they are what makes an edition that edition; colour, lighting, bevels,
      shadow, spine typography and whether it is drawn at all are the
      frontend's. The width clamp in particular is not taste — a cover image is
      cropped, scanned and jacketed at whatever aspect a provider felt like, so
      clamping corrects an unreliable proxy back onto a plausible physical
      object, which is a data judgement.
    - **`unwrap_or(320)` is honest here, and would not have been in item 17.** A
      renderer has no `None` to draw: a solid has to be *some* thickness, where a
      progress bar may legitimately draw nothing. So absence is filled — but it
      is never hidden. `ShapeSource::{Recorded, Assumed}` marks each number, the
      way `FractionSource` marks where a fraction came from, and the invented
      thickness is a middling paperback rather than an edge, so a book of unknown
      length does not masquerade as a remarkable one.
    - **`page_count = 0` was drawing as a pamphlet, and is now absence.** Moving
      the derivation found it: `unwrap_or(320).clamp(48, 1400)` sends `Some(0)`
      and every negative to 48, the thinnest book the model allows, and `make
      dev-db` has real zero rows. Unknown is not short. This is the one
      user-visible behaviour change in the item; everything else is asserted
      identical to the old arithmetic across six page counts, because the
      renderer is frozen and a move must not be a redesign.
    - **The GUI cannot reach this yet, and that is an API item rather than a
      frontend workaround.** `crates/api` was out of scope here (item 18 is
      editing DTOs in parallel), so `EditionShape` is engine-side only. A WebGL
      shelf gets it as a DTO field or it re-derives it in TypeScript, which is
      the exact failure this item was written to prevent — so the shelf item must
      not start before the DTO exists.
    - **The parameter, not a column.** `EditionShape::of_book` takes the cover
      aspect as `Option<f32>` because the engine stores no cover dimensions; item
      20 adds them. The TUI passes a decoded image's aspect, which is fine for
      the one book on screen and absurd for three hundred spines. When 20 lands,
      that call site becomes a division of two columns and no signature changes.

18. **List endpoints that survive a real library.** No migration; one new
    storage module, four new methods, three new requests and a filter type. Its
    subject is that `ListBooks{limit, sort}` was the whole list surface — no
    offset, no filter, nothing returning a count — and `list_notes` had no limit
    at all, which is a full table scan into a `Vec` for a screen showing twelve
    rows.
    - **Pagination is an offset, everywhere, and the spec's own alternative was
      the wrong shape.** The spec offered keyset for the sorts that can and
      offset for `Progress`, which cannot; item 17 then added `Author`, which has
      no `ORDER BY` at all. So **two of five sorts have no cursor key that exists
      in the database, and they are exactly the two whose pages are already
      expensive** — `Author` reads the whole library on page 1 and on page 40
      alike, so keyset saves it nothing. Paying a second pagination shape at
      every call site to speed up the three cheap sorts is paying in the wrong
      currency. Three further reasons: a **count composes with an offset and not
      with a cursor**, and a shelf that knows its total wants page numbers, which
      *are* offsets; the deep-page cost is a sort over a personal library, not a
      feed; and there is **no index on any sort key today**, so `ORDER BY title`
      sorts the whole table whatever the pagination shape is — keyset would not
      avoid that, which makes the index the real optimisation and the cursor a
      distraction. Named as a finding below rather than smuggled in.
    - **Offset pagination needs a total order, and the spec did not say so.**
      `LIMIT 20 OFFSET 20` is only the successor of `LIMIT 20` if both statements
      break ties the same way, and `publish_year DESC` over a library where four
      hundred books share a year does not say. Every arm now ends in `books.id`.
      **The behavioural test does not catch its absence** — measured, not
      assumed: removing the tie-breaks leaves
      `a_page_and_its_successor_partition_the_list` green, because SQLite's
      sorter is deterministic for one plan over one set of rows. That is a
      property of the current query plan and not of the schema, so the guard is
      `order_by_is_a_total_order`, which reads the SQL. A behavioural test that
      cannot fail is not the one holding the line, and saying so is worth more
      than a test that looks like it does.
    - **Where the TUI's policy would break if it ever adopted this.** The TUI
      fetches 200 books by recency and reorders *that page* in Rust, so `s`
      reorders the list rather than swapping its contents — correct, deliberate,
      and untouched here. It breaks the moment there are **two** pages: a
      Rust-side re-sort is only sound over a single page, because page 2 fetched
      by recency and then title-sorted in Rust does not concatenate with page 1
      into a title-sorted list. Fixing that means moving the sort into SQL, and
      moving it into SQL is precisely the membership change the 200-row fetch
      exists to avoid. So the two are compatible only while the TUI shows one
      page, which it does. A TUI that wants a second page has to choose, and this
      is the paragraph that says so.
    - **Filters and counts are one clause, written once.** `BookFilter` produces
      the `WHERE` and both `list_books` and `count_books` call the same function,
      because two hand-written clauses that agree today disagree the first time a
      filter is added to one of them; `the_count_agrees_with_the_page_for_every_filter`
      asserts it across ten filters and all five sorts. The **count is its own
      request and not a field beside the rows**: it is a property of the filter,
      a shelf asks it once and pages many times, and bundling would make every
      scroll pay for a scan of the whole matching set. Sharing the clause is what
      the spec's "build them together" actually buys; sharing the round trip is
      not.
    - **The status filter has four cases and `ReadingState` still has three.**
      "No reading" is `cur.id IS NULL` — the *join's* absence, deliberately not
      `cur.status IS NULL`, since a reading may exist with a null status and that
      book has been opened. It lives on `StatusFilter`, in the question, and not
      as a `ReadingState::NeverOpened`: a variant is a thing a UI filters on,
      counts, and eventually puts a badge beside, which is the framing this
      document bans. A filter case is a question somebody asked once.
    - **The per-row summary is counts, and the cheap-query argument for a boolean
      does not survive the implementation.** `book_summaries(&[id])` is three
      grouped aggregates over three existing `book_id` indexes for a whole page —
      the same three queries whether they end in `COUNT(*)` or `EXISTS`, because
      the alternative that would have been cheaper (a correlated subquery per
      row) is not how it is built. So the count costs nothing extra and carries
      strictly more; the mark is `> 0`, spelled once as `BookSummary::has` rather
      than in twelve components. Every number is **past tense** — highlights
      taken, notes written, files owned. Rows come back in the order asked, zeros
      included, so a caller zips it against the page it just fetched and
      "nothing behind this book" is an answer rather than a missing row.
    - **`list_notes` gets a limit and keeps its unlimited form, on purpose.** The
      obvious fix — a default cap — is wrong for the callers that are not
      screens: `resolve_note` walks every title in the vault so that `rb links
      "Reflection: Pachinko"` works, and a cap there would silently stop
      resolving the notes past it. A truncated correctness pass is worse than a
      slow one. The cap belongs to the caller with a viewport; `None` belongs to
      the caller with a graph to walk. `limit` selects along `created_at`, the
      same contract `BookSort` states.
    - **`EditionShapeDto` landed here rather than as its own item.** Item 19 left
      `EditionShape` engine-side and named the gap; the GUI links `crates/api`
      and not the engine, so item 26's shelf either gets the field or re-derives
      the arithmetic in TypeScript — the exact failure 19 exists to prevent. It
      is a derived read-only field beside `progress`, `series_label` and item
      20's cover readings, dropped on the way back in like all of them, and item
      20's stored dimensions make it a division of two columns rather than an
      image decode. A one-field item would have cost more to schedule than to
      write.
    - **The wire grew and `API_VERSION` did not move.** `ListBooks` carries
      `offset` and `filter` **flat** beside `limit` and `sort`, both
      `#[serde(default)]`, so `{"limit":20,"sort":"title"}` still means what it
      always did; `BookQueryDto` is the typed method's single argument, because
      `limit` and `offset` are both `i64` and adjacent and that is a swap no type
      checker catches. `Response::Count` is its own shape rather than reusing
      `Id` — an id identifies and a count measures.
    - **Three latent caps found by the signature change, all now honest.**
      `koreader::scores_for` read `list_books(10_000, …)` to decide whether a
      sidecar matches a book we already have, so a library past ten thousand
      would have started creating duplicates with nothing reporting it;
      `goodreads::export` read `i64::MAX`, a limit standing in for its own
      absence. Both are `BookQuery::default()` now, where a negative limit means
      no limit in both the SQL arm and the Rust one. And `rb book list` accepted
      three of the engine's five sorts: `author` and `year` landed with item 17
      and never reached a user, because the CLI's `match` was the only door and
      nobody widened it.
    - **Findings for a number, not built here.** (a) **A `highlights` FTS index**
      — `notes_fts` is still the only virtual table in the repo, so a GUI with
      one search box that finds notes and not highlights will be reported as a
      bug, correctly. It needs a migration, an `AFTER INSERT/UPDATE/DELETE`
      trigger trio or an explicit writer beside `insert_highlight`, and a
      `search_highlights` returning the `snippet()` shape `NoteSearchHit` already
      has; the search *surface* then wants one request answering both, since two
      lists a frontend interleaves is a relevance ordering invented above the
      seam. `find_books_by_title` is still a plain `LIKE` and belongs in the same
      item — as a `title` predicate on `BookFilter`, so search arrives as a
      filter rather than a seventh endpoint. (b) **Indexes on the sort keys**
      (`books.last_modified`, `books.title COLLATE NOCASE`, `books.publish_year`)
      — a migration, and the thing that would actually make a deep page cheap.
      (c) **`sort_author`**, still open and still argued against by item 20: a
      column SQLite cannot compute is NULL for every existing row until a
      back-fill nobody has run runs, and the fallback for that window is the
      whole-table read it exists to remove. It only pays as part of (b), where
      the back-fill and the index arrive together. (d) **`MatchCandidateDto`
      carries a title and a score but no author**, though `koreader::band`
      already holds the whole `Book` — so "which Dune is this" costs an N+1
      `get_book` per candidate. Same class as the per-row summary this item
      built, in a file this item did not own.

37. **A real PDF sidecar, and a `Diagnostic` instead of silence.** No migration;
    one `ErrorClass`-less diagnostic variant, one tier-1 fixture, one three-way
    split in a function that had two arms. Its subject is a comment that was
    true when it was written and false everywhere else: `entry_to_highlight`'s
    "a real highlight always carries a pos0 xpointer" holds on EPUB and not on
    PDF, where KOReader anchors to a page plus coordinates because a scanned page
    has no text stream to point into. `get_str` returned `None` on that table,
    `?` returned `None`, and the entry left no trace — so a user with a PDF
    library imported zero highlights and was told nothing, which is
    indistinguishable from a book nobody had highlighted.
    - **Skipped-with-a-diagnostic, and imported-with-a-different-anchor is a
      separate item that this one deliberately did not open.** `identity_hash =
      sha256(book_id | ko_datetime | pos0 | text)` is what makes import
      idempotent, so serialising a coordinate table into the `pos0` column fixes
      the identity of every PDF highlight to whichever serialisation is picked on
      day one — and coordinates are the *drifting* half of a sidecar, the same
      class as `pageno`, which already moves 29→30 on a re-render with no user
      action. An anchor built from a number the device rewrites is an anchor that
      re-inserts the same highlight after every re-render, which is precisely the
      failure the tier-2 layout split exists to make observable. `DeviceDigest`
      and `DEVICE_FIELDS_DIFFER` would both have to agree about it as well. It
      also cannot be designed here: the anchor's *shape* is settled and its
      *contents* are unobserved, so the item needs a real PDF sidecar before it
      needs code.
    - **One diagnostic per file, carrying a count — never one per entry.** A
      300-highlight PDF emitting 300 identical lines has replaced silence with
      noise, and noise is not the improvement that was wanted. `KoSidecar` grew a
      `usize`, not a `Vec`, so the shape of the type is the decision.
    - **No `ErrorClass` on it, and that is the correction the item forced.**
      `ErrorClass` is `From<&EngineError>`; it exists to classify something that
      *failed*. Nothing failed here — the file read, the chunk evaluated, the
      entry is well formed and we simply cannot represent it — so there is no
      error to classify and a class would be a field with no source. The engine's
      rule is "a partial failure returns a typed `Diagnostic`", not "every
      `Diagnostic` carries an `ErrorClass`", and half the existing variants
      already carry none.
    - **The change is scoped to one Lua value type.** `koreader::anchor` treats a
      **table** as unstorable and sends every other value through the same
      `get_str` as before — so a numeric `pos0` still coerces to its digits and an
      empty one is still filtered, exactly as they were. "No reflowable sidecar
      imports differently" is therefore a property of the code rather than a claim
      about which fixtures happen to be committed, and the goldens confirm it:
      every pre-existing fixture's imported-highlight count is byte-identical,
      and the only golden row added is a new fixture at zero.
    - **The fixture states one fact and illustrates the rest, and the
      distinction is written into it.** `Gen-Pdf-Anchors.sdr` asserts that `pos0`
      is a *table*; the `page`/`x`/`y`/`zoom`/`rotation` keys inside it are a
      reconstruction of a file nobody here has read. That is safe only because
      **nothing reads them** — the engine branches on table-ness and never on a
      key — so no golden can bless a key name, and a real device writing `rect`
      instead costs no change. `docs/koreader-format.md` §6 now separates
      "settled" from "do not treat the fixture as evidence" in those words,
      because the alternative was a synthetic fixture quietly promoting itself to
      an observation. The pre-existing `Gen-Pdf-Sidecar` was covering the sidecar
      *filename* and had been reading, by its name, as though it covered the
      format; it is untouched and now says so.
    - **The golden's `has_warnings` boolean became a list of named warnings**, and
      that was not tidying. It was `true` for an unknown device status and `true`
      for an unanchorable highlight, so no golden could tell one degradation from
      another — a fixture asserting "something warned" is green when the wrong
      thing warns. Naming the kind, with its count, is what makes the new goldens
      show the lost highlights *as a diagnostic* rather than as an absence, which
      is the only version of this item worth having: a golden that merely lacks
      the highlights is the original bug, written down and made permanent.

34. **One search surface.** Migration `0015`; a `highlights` FTS index, one
    request answering notes and highlights together, and `title` as a predicate
    on `BookFilter`. It builds finding (a) of item 18, and its subject is that
    `notes_fts` was the only virtual table in the repo — so a search box would
    have found the six notes a reader has written and none of the four hundred
    passages they kept, correctly reported as a bug.
    - **Triggers, and the argument is the inverse of `notes_fts`'.** `notes.rs`
      records "`notes_fts` cannot have triggers" as a settled answer rather than
      a deferral: a trigger copies between tables and a note's body is **not in
      the database** — `notes` has no body column, so there is nothing to read.
      `highlights.text` is a column, so the same sentence read the other way
      says a trigger *can*, and an **external-content** fts5 table can index it
      without storing a second copy of the reader's private passages at all.
      That buys the property the index exists for: **no write path can skip
      it.** `insert_highlight`, `refresh_device_fields`, `set_annotation`,
      `merge_books`' drop and `delete_book`'s cascade all maintain it without
      naming it — and so does `crates/corpus`' `gen-devdb`, which writes raw SQL
      and by design does not link the engine, so an explicit Rust writer would
      have had to be re-implemented there, exactly as `notes_fts` in fact is.
      Four things were rehearsed against sqlite3 3.51 rather than reasoned
      about, and two changed the file: **a cascade does fire the delete
      trigger** (without it, `MATCH` keeps returning a deleted book's rowids),
      and the update trigger is **`AFTER UPDATE OF text, ko_note, annotation`**,
      because `attribute_highlights` rewrites `reading_id` for every highlight
      of a book on every import and `merge_books` rewrites `book_id` per moved
      row — neither touches an indexed column, and an unrestricted trigger would
      make both pay a reindex per row for nothing. The back-fill is `'rebuild'`,
      one statement: unlike `0012` and `0014`, this migration *can* back-fill
      honestly, because the values are already in the column.
    - **One ranked list, and it is deliberately not ranked by bm25.** The item
      was right that two lists a frontend interleaves is a relevance ordering
      invented above the seam. It was wrong to imply the merge key is a score:
      fts5's rank comes from **that index's own corpus** — its document
      frequencies, its average document length — so a note's −8.2 and a
      highlight's −8.2 are not the same claim and no constant converts one into
      the other. Sorting a union on them would be arithmetic on incommensurable
      units, which is worse than source order because it *looks* like relevance.
      So each index is ordered by its own rank, which is the one thing bm25
      supports, and the two are merged **by within-source position** — the best
      note beside the best highlight, then the seconds, and so on. That is
      reciprocal rank fusion with one source per item, so no `k` has to be
      picked. Its one consequence is stated rather than discovered: a query
      matching fifty notes strongly and one highlight weakly puts that weak
      highlight at the top. For this app that is right — a single matching
      passage buried at position 47 is a passage nobody sees. Ties across the
      two sources break on **recency**, the one key genuinely comparable
      between them (both `created_at` columns are "when we stored it", unix
      seconds, one clock).
    - **`search_notes` was replaced, not joined, and `API_VERSION` is 2.** The
      recommendation was accepted. Keeping both doors would leave a client able
      to ask the two halves separately and merge two rankings it has no rule
      for — the interleaving problem with extra steps, and it would make the
      one-list guarantee optional. The narrowing survives as a **filter**
      (`source`, absent means both) in `BookFilter`'s idiom, because "search my
      notes" is a real question while the *ordering* is never the caller's;
      `rb notes --search` is that filter and shares every line of printing with
      `rb find`, so the two doors cannot come to disagree.
    - **`title` is `LIKE`, not FTS, and that is a refusal of the item's own
      suggestion.** Three reasons in the order they bind. It has to compose with
      `count_books`, all five sorts and the offset paging, and `BookFilter`
      writes **one** `WHERE` for the page and the count alike — fts5 would have
      to arrive as `books.id IN (SELECT rowid …)` plus a third index with a
      third maintenance discipline, for one short line of text. It has to behave
      like its five neighbours, or a struct whose fields match by different
      rules is a struct whose callers guess. And **infix is what a shelf filter
      means**: `possess` finds *The Dispossessed* under `LIKE` and finds nothing
      under fts5, whose tokens start at word boundaries. Relevance lives in the
      search, over what the reader wrote and kept; a book is neither.
    - **The defect the door exposed, and it was live.** `search_notes` bound the
      user's raw text straight into `MATCH`, and fts5 reads `-`, `+`, `*`, `:`,
      `(`, `NEAR`, `OR` and a bare `'` as syntax — so `rb notes --search "don't"`
      and `C++` failed with a raw sqlx error rather than a search, and had since
      item 7. Every token is now a quoted phrase, joined by a space (which fts5
      reads as AND), with a trailing `*` kept because prefix search is reached
      for on purpose. Measured against sqlite3 3.51, which is also how the
      empty case was settled: **`MATCH ''` raises**, so `Some("")` was never
      "returns nothing" for free. An empty or whitespace query is no hits and no
      error and issues no statement — an empty search box is not asking, and
      "everything" is not what it would mean if it were.
    - **Findings for a number, not built here.** (a) `highlights_fts` indexes
      `text`, `ko_note` and `annotation` but **`chapter` is left out** — every
      highlight in a book carries one, so it matches broadly and means nothing.
      If a "which chapter was that in" question ever arrives it is a filter, not
      a fourth indexed column. (b) **Books are not in the unified search** and
      `title` stays a filter, so "find" and "which book" remain two questions; a
      third arm would put an index over four hundred short strings beside two
      indexes over prose, where the position merge would give a title hit the
      top slot on every query. (c) The **query language stops at a trailing
      `*`** — no `OR`, no `NEAR`, no column syntax. A reader who wants one is
      asking for a query language, which is a surface somebody has to decide on,
      not an escape hatch to leave open by accident.

35. **The sort keys get an index, and a writer.** Migration `0016` — four
    indexes and one column. There was **no index on any sort key**: the seven
    that existed were all foreign keys, so every `ORDER BY` the library screen
    issued sorted the whole table and threw away all but a page of it, which is
    what made a deep page expensive and what turned item 18's `books.id`
    tie-break from insurance into the thing holding pagination together. Every
    sort but `Progress` now walks an index, asserted against
    `EXPLAIN QUERY PLAN` rather than behaviourally, because **a behavioural test
    cannot see an index** — each of these sorts returned the right rows the day
    before the migration and returns the same rows now. `Progress` is a computed
    ratio across a `LEFT JOIN` and is the one sort that cannot be indexed; its
    plan still says `USE TEMP B-TREE FOR ORDER BY`, and that is kept as the live
    control which proves the other four assertions are ones the suite can fail.
    - **`sort_title` had never been computed by anything**, and that is the trap
      this item was really about. It was in `0001_init.sql`, in `MERGE_RULES`, on
      `Book`, on `BookDto`, in the generated TypeScript, bound by the upsert —
      and NULL on every row of every database that has ever existed, which is why
      `BookSort::Title` ordered by `books.title` instead. **A sort-key column
      added without a writer looks answered and is not**, and that reads
      identically to answered from every call site. It is written now: the title
      with a leading English article dropped, so *The Overstory* files under O.
    - **Deleting it was the considered alternative and lost on one argument.**
      Nothing had read it in the life of the schema and `books.title` is what a
      user sees, so the case was real — but the delete reaches `BookDto` and
      `bindings.ts`, which item 34 owns this wave, and a half-delete (column
      gone, DTO field always null) is a *fourth* state and worse than the third.
      The wire shape is unchanged and `BookDto.sort_title` stops being always-null,
      which is a change of meaning without a change of shape.
    - **`sort_title` left `MERGE_RULES` in the same breath, and that is migration
      `0014`'s argument arriving a second time.** That table governs what a
      *record* can carry, with a source to attribute, and no provider publishes a
      filing key. Leaving it there would have handed every record-shaped writer a
      way to move a book's filing name without moving the title it is derived
      from — `0014`'s "the stored dimensions describe a different image", one
      column over — and it would have let a provider *claim* a value that the
      derivation had already replaced, i.e. a `field_provenance` row naming an
      origin that supplied nothing. Only `a_record_claims_exactly_the_fields_it_supplies`
      noticed, on the first run, exactly as it noticed item 32. So it takes the
      cover metrics' shape: one writer, no claim, ignored by every merge.
      `only_our_own_columns_sit_out_the_federated_merge` is down to one column.
      The cost, stated: there is no longer a door for a user to file a book under
      a name of their own. Nothing had ever opened it — `rb set` has no flag —
      and re-opening it is a `Rule` and a flag, not a migration.
    - **`sort_author` lands, after being refused twice.** Items 18 and 20 both
      declined it on one real ground: SQLite cannot parse a human name, so the
      column is NULL for every existing row and `ORDER BY sort_author` is
      silently wrong until a back-fill nobody has run runs. It only pays *inside*
      the item that adds the index, where the back-fill and the reader arrive
      together — and the wave runs against disposable data, so a shape change
      needs no data migration. `BookSort::Author` was the one arm with no
      `ORDER BY` at all: it read the whole library, sorted in Rust and *then*
      truncated. That arm is gone, not supplemented.
    - **The encoding is the hard part, and it is proved rather than eyeballed.**
      `names::sort_key` returns `(rank, last name, whole name)` and a column is
      one TEXT value, so the tuple is packed as
      `'0' SEP escape(last) SEP escape(whole)` with `SEP = \u{1}`. For a
      concatenation to compare like a tuple, **the separator must be strictly
      less than every byte a component can contain**; `escape` buys that by
      mapping `\u{0}\u{1}\u{2}` to two-byte sequences that are monotone and
      prefix-free, so nothing is reordered and nothing collides. SQLite's BINARY
      collation is `memcmp` and Rust's `Ord for String` is byte-wise, so the two
      ends compare the same way rather than similarly — and the property
      `the_stored_order_is_the_tuple_order` asserts it over arbitrary author
      lists including the control characters. **A column that nearly agrees with
      the function is worse than no column**, because both exist and only one is
      read; that is why the escape is there for names nobody will ever type.
      A `COLLATE NOCASE` index would have compared differently from the function
      it caches, so this one is deliberately BINARY.
    - **`sort_author` is deliberately not on `Book`.** It is the engine's filing
      key, not a fact about the book, and keeping it off the domain type keeps it
      off the wire — no DTO field to explain, no frontend that can render a
      string full of control characters, and no `dto.rs` edit in a wave where
      another item owns that file.
    - **One writer, reading the row back rather than the record.** Every write
      that can move a title or an author list calls `refresh_sort_keys` last
      inside its own transaction — the upsert, `enrich_book_attributed`,
      `fill_book`, `merge_books`' `dst`-wins fill, and `set_book_fields` through
      the second of those. It derives from **what was actually stored**, not from
      the incoming `Book`, and that is the decision worth carrying: a merge
      clause keeps the row's title when the record is silent, so a key bound from
      the record would describe a title the row does not have. Deriving from the
      stored value makes the two coherent through the merge rule, the user guard
      and the `dst`-wins inversion alike without re-spelling any of them — which
      is `invalidate_cover_metrics`' rule reached by a different route.
    - **The back-fill has a door, `rb sort-keys`, and `make dev-db` runs it.** A
      back-fill with no door is a function nothing ever calls, which is how
      `measure_stored_covers` sat unexercised for a wave — and this one has the
      sharper edge, because until it runs a book files at the *top* of an author
      sort rather than under its author. Its work list is `sort_author IS NULL`,
      which is *not computed yet* and never *no author*: an authorless book gets
      a real key whose rank component files it last. Idempotent, and its wording
      obeys both absence rules — `every book already files under a sort key`
      rather than `filed 0 books`, and no count of what has not been done.
    - **`BookSort::Title` orders by `COALESCE(sort_title, title)`, and the index
      is on that same expression.** Not tidiness: it is what let the migration
      ship with no SQL back-fill of `sort_title`, so the article-stripping rule
      exists in one dialect (Rust) instead of two. A row the writer has not
      reached degrades to its own raw title — the exact order it had before —
      rather than piling at the top. `a_sort_title_index_that_nearly_matches_is_not_used`
      pins both near misses: drop the `COLLATE NOCASE` or index the bare column
      and the planner walks straight past an index that reads correctly in the
      schema, which is `0008`'s lesson.
    - **The article rule is deliberately English-only.** `books.language` is NULL
      for most of a real library, so a table covering `der`/`el`/`la`/`le` would
      be applied on a guess and would file *Das Kapital* under "Kapital". Narrow
      for `pdf.rs`'s reason: a general rule that is wrong for some books is worse
      than a specific rule that is silent about them. Stripping never empties a
      title — "The" files under "The" — because a book filed under nothing is the
      failure `names.rs` names as the one that matters.
    - **The TUI's title sort was a second opinion and now is not.** `library.rs`
      sorted its fetched page by `b.display_title().to_lowercase()`, which agreed
      with the database only because `sort_title` was NULL everywhere. It calls
      `sort::title_key` now, the way its author arm already called
      `names::sort_key` — item 17's finding arriving a second time, for titles.
      The one honest mismatch left is stated rather than papered over: SQLite's
      `COLLATE NOCASE` folds ASCII only and Rust's `to_lowercase` is full
      Unicode, which predates this item.
    - **Two hand-maintained numbers beside generated SQL were removed on the
      way past.** The upsert's column list and its row of `?`s were written out
      by hand beside a clause generated from `MERGE_RULES`, and the three
      `UPDATE`s named `?20`/`?21` — all four decided by that table's length.
      Removing a column is what would have broken them, so they are computed now.
    - **Finding, not built here.** `crates/corpus`' `gen-devdb` still writes
      `sort_title` as literal NULL, and that is left deliberately: the door fills
      it on every `make dev-db`, which is what exercises the door, exactly as the
      seed's unmeasured covers exercise `rb covers`. A seed that stated the key
      would be a fixture agreeing with the engine by copying it.
36. **The chooser knows who wrote it.** No migration; two fields on one struct
    and its DTO. `koreader::band` has been holding the whole `Book` since item 3
    and keeping `book_id`, `title` and `score` off it, so the screen a refusal
    leads to — where *which Dune is this* is the entire question — could not
    answer it: two editions with one title and two authors read as the same row,
    and the only way to tell them apart was a `get_book` per candidate. Reported
    independently by items 22 and 18, which is what made it an item.
    - **`MatchCandidate` is a row to show, not a record to write**, and that one
      sentence decided every field on it. The only thing any caller does with one
      is hand `book_id` back to `link_sidecar`/`link_calibre_book`/
      `link_goodreads_row`, so nothing on it round-trips; what it owes is an
      answer to *which of these is the book in my hand*.
    - **`authors_display`, and deliberately no raw `authors` beside it.**
      `BookDto` carries both because it is a record that goes back in; a
      candidate is not, so a second spelling of the same names would be a second
      thing to keep in sync for a row nobody writes. The parsed form is the one
      that crosses because the **order within a name is a parse and is the
      engine's** (item 17, `names.rs`) and the GUI links `readingbuddy-api` and
      not the engine — a raw list here would be `names.rs` re-implemented in
      TypeScript, which is the re-derivation item 17 exists to prevent. Named for
      `BookDto::authors_display` on purpose: one idea spelled two ways is how two
      frontends end up disagreeing. The **join** between names stays a
      frontend's, so it is a list and not a sentence.
    - **`publish_year` earned its place; `cover_path` did not.** The year is what
      is left to choose by in the band's *ordinary* case — a variant title of a
      book already on the shelf is usually the same author, so title and author
      both tie and a 1965 Dune against a 2005 reissue is the whole distinction.
      It is an `Option<i64>`: eight bytes, no allocation. A cover is a different
      bargain. **No chooser draws a jacket** — the TUI's picker is a text list,
      the CLI prints lines, the GUI has no chooser at all — and the prompt's own
      test was *only if a chooser would actually show one*. When one does, the
      field it needs is not this one: item 20 made `cover_shelf_path` what a grid
      loads, beside `cover_aspect` and `cover_accent`, because a frontend reading
      `cover_path` directly shows nothing for every cover small enough to have no
      thumb. So the honest addition is a **cluster**, and it belongs to the screen
      that draws it. Both directions stay cheap: additive fields do not move
      `API_VERSION`.
    - **The whole `BookDto` was the real alternative, and it was measured rather
      than argued.** The case for it is strong — `band` is holding the entire
      `Book`, and *picking* is what created this bug. Two things beat it. A
      `BookDto` for a book with a real (681-character) description serializes to
      **1846 bytes against a candidate's 98**, ~19×, and still ~10× with
      `description`, `first_sentence` and `subjects` all stripped; candidates are
      produced **per row** of a calibre or Goodreads import, so a four-hundred-book
      library with a few near-misses a row is megabytes of prose nobody reads.
      And it answers the wrong question: `BookDto` carries `progress` and
      `reading_status`, which would invite a chooser to draw a progress bar on a
      book it is asking you to *identify*. The asymmetry settles it — picking
      wrong again costs one more additive field, paid by whoever draws the
      screen, while `BookDto` costs every import report for ever.
    - **`band` is the only constructor, and structurally rather than by
      convention.** All five candidate-producing paths were audited —
      `match_candidates`/`Engine::sidecar_candidates`, `files::identify` and
      `import_file`, `import_calibre_library`, `import_goodreads` — and none
      builds one by hand: `band`'s input `Scored` is `pub(crate)` and minted in
      exactly one place (`scores_for`), so the type system already forbids the
      second construction site. That is why no `Default` impl was added, which
      would have let a real call site forget the field silently. What guards it
      going forward is not the constructor but the tests: each of the four
      surfaces' existing candidate assertions now checks the author arrived,
      because a *new* path also has to pass them.
    - **Nothing above the seam was changed, on purpose.** No CLI output, no TUI
      picker row, no GUI. The item is about what a candidate *carries*; what a
      screen does with it is that screen's decision, and drawing an author in the
      TUI picker without the layout work its `DETAIL_MAX` regression asks for is
      how the "n if not" clip came back last time. The engine half is what could
      not be worked around.
    - **No caller does a `get_book` per candidate today, and that is the point.**
      The audit found none to delete: the cost was structural rather than
      committed — every frontend that has drawn a candidate row so far has drawn
      only a title, because a title is all it had. The N+1 was what the *next*
      screen would have had to pay, and the two items that reported it are the
      two that went looking.

38. **One fixture, two consumers.** No migration; one declaration file, two tests
    that read it, and the four cover fields the frontend's fixture had never
    carried. Its subject is the one class of defect this repo keeps finding the
    expensive way: **a fixture can disagree with the engine about a column and
    nothing notices**, because a fixture is what the tests are written against.
    - **The declaration is neither fixture.** `crates/corpus/edge-cases.json`
      states the hostile set once — id, title, authors, page count, cover, state,
      series, subject count — and `gen-devdb` and `gui/src/lib/api/fake.ts` are
      both asserted against it, from Rust and from vitest. **Nothing generates
      it**, deliberately: a declaration generated from one of the two things it
      governs would agree with that one however wrong it was, which is
      `crates/corpus`' own rule about oracles restated one level up. Hand-edited
      is the point, not a shortcut.
    - **It was mutation-tested rather than assumed.** The bar the item set was "a
      test fails when an edge case exists in one fixture and not the other", and
      a suite that merely enumerates both passes that bar while asserting
      nothing. So the guard was run against a case added to the declaration and
      absent from the generator (`declares 22 cases and gen-devdb builds 21`) and
      against a shape disagreement on a case both have
      (`case 3 (The Doorstop): page_count`) before being believed. Both halves
      compare **lengths first**, so a case *removed* fails as loudly as one added
      — the direction a per-case loop misses, and the direction two fixtures
      actually drift apart in.
    - **It found four real disagreements on the first run**, which is the
      argument for the item. `fake.ts` gave every book `subjects: []` where the
      seed gives each edge case one; `The Claw Of The Conciliator` was *unread*
      in one and *reading* in the other, so the case where a shelf shows a series
      label and progress together had never been rendered anywhere; the
      twenty-first case (`reading_state: other`) existed only in the frontend's
      fixture; and the untitled case was modelled as `null`.
    - **`books.title` is `TEXT NOT NULL DEFAULT ''`, so a stored title is never
      NULL.** The fixture had been modelling a state no library can produce. Both
      now use the empty string — and the engine's own `Book::display_title` is
      `self.title.as_deref().unwrap_or("(untitled)")`, which catches the
      unreachable absence and lets the reachable one through as a **blank**. That
      is a real defect with 76 call sites and three sibling copies for foreign
      records (`goodreads.rs`, `calibre.rs`, `ko_statistics.rs`) where `None` is
      genuinely reachable, so it is **recorded here rather than patched inside a
      fixture item**. `titleLabel` in the GUI already handles both.
    - **The cast was the mechanism.** `fake.ts`'s header claims a drifted or
      renamed DTO field is a `tsc` error there, and `book()` ended in
      `as StoredBook`, which makes that true for renamed fields and false for
      **added** ones. Three fields had been missing since item 20 landed —
      `cover_shelf_path`, `cover_aspect`, `cover_accent` — so every tile in
      layers 1 and 2 exercised the no-cover branch and the cover-bearing branch,
      the one item 26 is about to build on, was exercised nowhere. Removing the
      cast also surfaced a live hazard it had been hiding: `BookDto.id` is
      `number | null` while `StoredBook.id` is `number`, so an `over` naming an
      id could put a null in the field every route keys on. `id` is now applied
      **after** the spread and tsc proves the invariant a cast had been
      asserting.
    - **The shape crosses; only the bytes are withheld.** `FakeClient::coverSrc`
      still returns `null` — layer 2 has no asset protocol and a URL would render
      as a broken image, which was always the right call. What was wrong was
      inferring from it that the *fields* should be absent too. A component
      asking "what box does this tile reserve" now gets a real answer in a bare
      browser.
    - **The shelf-path fallback is asserted as a rule, never as a fixture.**
      `make dev-db` writes 240×360 covers, both dimensions under
      `images::THUMB_MAX` (400), so no tier is written and
      `Book::shelf_cover_path` yields the original — which means
      `cover_shelf_path == cover_path` for all 203 of them. A test asserting a
      thumb *exists* would be asserting the fixture and would go red the day the
      seed's covers changed size for an unrelated reason. `width_source` is tied
      to `cover_aspect` in the same spirit: `EditionShape` reads
      `Some(aspect) => Recorded`, `None => (PAPERBACK_ASPECT, Assumed)`, so the
      pair cannot disagree in the engine and is asserted not to here.
    - **`TauriClient::coverSrc` was reading `cover_path`**, though `gui/CLAUDE.md`
      has said since item 20c that a grid loads `cover_shelf_path`. It now does.
      No screenshot could have caught this: against a dev library the two paths
      name the identical file, and they diverge only for a cover large enough to
      have a tier — i.e. on a real library, on the shelf, at sixty hero shots a
      screen.
    - **A number in a Makefile echo is a claim nothing checks.** `make dev-db`
      printed a hand-written "220 books, 20 of them deliberate edge cases", which
      went stale the moment this item added a case. The generator is the only
      thing that knows, so it prints both counts and the Makefile states none —
      rather than adding `python3` or `jq` to a build that has neither, to parse
      a manifest for a line of prose.

26. **The shelf.** No migration; one home surface, a layout seam with two
    implementations, and a luma band this frontend had been missing. Specified
    as a WebGL spine shelf and **deliberately not built as one** — the user
    ruled the 3D renderer a cosmetic decision that can come later, and the item
    was rebuilt around that rather than half-built toward it.
    - **The arrangement is a value, not a shape of the page.** Deferring the
      spine shelf badly would mean welding a grid into `+page.svelte`, so that
      whoever builds spines later is unpicking a layout rather than adding one.
      `gui/src/lib/shelf/layouts.ts` is the only file that knows what
      arrangements exist; a layout is a component plus a name, and the spine
      shelf is a third entry that should need no change to it.
    - **Two implementations, because one is a guess.** `CoverGrid` is the
      default and `Rows` is not decoration: a seam nothing has ever been swapped
      through is this repo's standing complaint about a guard that cannot fail,
      one layer up. `Rows` wants a fixed box rather than a reserved aspect and
      puts the title where the grid puts a cover, which is what makes the
      one-field contract known to be sufficient rather than believed to be. The
      route suite screenshots **both** — a layout nobody renders is a layout
      nobody reviews.
    - **`currently_reading` is a request, not a filter spelled again.** "Pulled
      proud" is a selection predicate and those are the engine's under item 17.
      It also returns the *reading*, which `reading_state` on a book row cannot:
      a reread has two, and item 28's card is minted per reading. The strip's
      failure is caught separately from the shelf's, because a library that
      loaded must not be replaced by an error thrown by the ornament above it.
    - **The GUI owed a luma band and did not have one.** `images.rs` stores
      `cover_accent` unclamped and says why — the clamp is *a renderer's policy
      about its own lighting, not a fact about the file*. The TUI has such a
      policy; this frontend was painting the raw measurement, so a plate could
      be near-white or near-black and carry no inset panel at all. The hostile
      fixture is what exposed it: `fake.ts` invents full-gamut accents on
      purpose, and the defect was invisible against any library of real jackets.
      `src/lib/accent.ts` is the band, and **its constants deliberately differ
      from `texture.rs`'s** — a terminal's background is unknown and a webview's
      is its own `--bg`. Copying them across would be item 39's duplication in
      reverse.
    - **The band's floor is not always reachable, and that is asserted rather
      than papered over.** Scaling a colour toward the band preserves hue where
      clipping each channel would not, but a saturated blue is already at
      maximum on its one bright channel, so it cannot be lifted to the floor.
      The property asserts the ceiling unconditionally, asserts only that a dark
      colour never gets *darker*, and pins pure blue as the known limit — on the
      engine's own rule that asserting something false and weakening it later is
      worse than asserting less. Tolerances are one byte, because the output is
      three integers and a finer claim is about arithmetic that never reaches a
      screen.
    - **A book with no measurement gets no colour at all.** Not a grey: a
      fallback colour would make "never measured" and "this jacket is grey" the
      same picture. It gets the hatch, which is a composed empty state rather
      than an apology.
    - **No token was promoted to `app.css`.** A token is a promise that every
      screen will use it, and one screen exists. Item 27 is when what is
      genuinely shared becomes visible.
    - **The screenshot review found three axiom defects, not three taste
      defects**, which is the argument for rendering a screen rather than
      reasoning about it. `Rows` **hid the state label** below 420px — and the
      state is *what you did*, the one thing this surface exists to say and the
      only number it is allowed to carry, so a phone was shown a shelf that said
      what you own and nothing about your reading. It now moves to a third line.
      `Rows` also gave an unmeasured cover a **flat pale chip**, indistinguishable
      from a genuinely pale jacket, breaking the rule the grid honoured two files
      away. And the accent measured **2.78:1** on the light theme while carrying
      every state label, with the segmented switch's *selected* segment at 2.95:1
      against its unselected sibling's 5.61 — the control whose only job is
      showing which state is on had the on state as the harder one to read.
    - **`--accent-text` is a rendering of the palette, not a second palette.**
      `theme.rs` owns brass and still does; what is bounded here is its
      legibility against *this* app's background, which is the same argument
      `accent.ts` makes about a jacket and the same one `images.rs` makes about
      where a luma clamp lives. On the dark theme the two values are identical.
    - **Size alone does not carry hierarchy.** The Reading strip was the shelf's
      tile 13% larger, and at 720px the two sizing laws crossed and the band
      pulled *proud* rendered a third **smaller** than the shelf beneath it. It
      is now a different kind of object — its own ground, a rule beneath it, and
      a track clamped wider than the grid's column at every width. The
      duplication between the bands was kept: a shelf is your whole library, and
      pulling a book out of it while you read would be the app maintaining a
      queue with an outstanding state, which is the framing this product forbids.
    - **A step away from a colour cannot be a `color-mix`.** The plate's inset
      panel mixed toward white, which is crisp on a dark jacket and invisible on
      a pale one; mixing toward the ink instead did the same thing mirrored.
      Neither is wrong about one book and both are wrong about a shelf, which
      owes every jacket the same amount of design. `plateShades` steps toward
      whichever pole is near, in TypeScript, because a stylesheet can only mix
      toward a colour named at author time.
    - **One reported defect was declined.** The review flagged the lowercase
      `paused` — `ReadingStateDto`'s `other` arm — as an enum value leaking to
      the user. It is another application's word, kept verbatim by a decision
      item 17 made and `phrasing.test.ts` pins, and title-casing only flatters
      the single-word case: the fixture's real shape is
      `paused-by-some-other-app`. The rule that a frontend words a value covers
      *our* vocabulary, and this is the one label deliberately outside it.
39. **The border-median accent, read rather than re-measured.** No migration; one
    deleted function, one new parameter, one column read. Its subject is not the
    duplication — that was twenty-six lines and nobody's bottleneck. It is **how
    a duplicate stays alive**: this one survived item 19, item 20 and three
    handoffs behind two written justifications, *both false*, and neither was
    ever checked against the code it described.
    - **What was true.** `crates/engine/src/images.rs` and
      `crates/tui/src/render3d/texture.rs` each computed the median of a 2px
      border — the same frame, the same `border = 2.min(w/4).max(1)`, the same
      loop, differing only in that the engine packs `0xRRGGBB` unclamped and the
      renderer returned a `Vec3` already pushed into a luma band.
    - **The first false justification was a promise.** `images.rs`'s own comment
      said the copy was deliberate "for exactly one item" and that **item 19**
      would rewire the renderer onto the column and delete it. Item 19 shipped —
      it moved `EditionShape` into the engine — and did not touch the accent.
      A comment naming a future item is a TODO with a due date nothing enforces,
      and when that item ships for its *own* reasons the note reads as satisfied.
    - **The second was a plausible technical difference nobody re-read the code
      to confirm.** The handoff said the two "can legitimately differ" because
      `images.rs` measures the original file and `texture.rs` the *scaled
      texture*. It does not: `load_cover` called `accent_from_border(&img)` on
      the full-resolution decode and resized on the **next line**, and
      `Scene::cover` loads `book.cover_path` — the same file `store_cover`
      measured. Same bytes, same arithmetic, identical medians by construction.
      This is the more dangerous of the two, because it converts a task into a
      *decision*, and a decision with no owner is carried forward for ever.
    - **The ruling: the TUI reads the column and deletes its own loop.**
      `Book::cover_accent_rgb()` is the accessor and `images::accent_channels`
      stays the one place the packing comes apart. Nothing about what the
      renderer draws changed — the arithmetic on both sides was identical, which
      is exactly what made the cut safe to make against a frozen renderer.
    - **The luma clamp stays in the renderer**, and this is the line item 20 drew
      and item 17 drew before it. A legible band (0.14–0.62, now `ACCENT_LUMA`)
      is a policy about *this* renderer's lighting against *this* renderer's
      cream page edges; the stored median is a fact about the file. A WebGL shelf
      will want a different band and must not inherit ours.
    - **`load_cover` grew a parameter rather than a way to find a book.** The
      accent is passed in as a `Vec3`. `Scene::cover(book, texels)` is the only
      call site holding a `&Book`, so it is the only place that needs to know how
      an accent is obtained, and `texture.rs` stays free of the engine's domain
      types. The alternative — threading `&Book` down into the texture loader —
      would have put "how do I get an accent" in the module that no longer has
      any business answering it.
    - **`cover_accent` is now in both cache keys**, `Scene`'s `CoverKey` and
      `present::cover_hash`. This is the only part of the change that is not
      pure deletion, and it is not tidiness: `rb covers` fills the column without
      moving the path, the id or the title, so a key blind to it would serve a
      back-filled book its old spine indefinitely — and a *transmitted* frame
      that is never re-sent is a stale image with nothing on screen looking
      wrong, which is the failure `a_resize_retransmits_the_parked_image` exists
      to catch one door down.
    - **`cover_accent IS NULL` draws the title's hue, and that cost is stated
      rather than hidden.** NULL happens for an undecodable cover and for any row
      predating item 20 that `rb covers` has not reached. Those fall back to
      `procedural_cover`'s title-hashed accent — so on an un-back-filled library
      a book with a perfectly good jacket gets a spine coloured by its *name*
      instead of by its boards. The back-fill exists and is one command; the
      alternative was a grey nobody chose, or keeping the loop alive to
      re-measure a file the engine has already opened once. The two hues cannot
      drift apart, because `procedural_accent` is one function that both the
      plate and the fallback call.
    - **The three tests were re-pointed, not deleted.** The behaviour they pinned
      still exists; only its source moved. `accent_luma_is_clamped_into_the_
      visible_band` now clamps a *stored* accent and additionally asserts that
      one already inside the band arrives untouched — the half that proves the
      column is unclamped. `accent_follows_the_border_not_the_middle` keeps its
      name and inverts its evidence: the *measurement* is asserted in
      `images.rs`, so what is left here is that the stored median reaches the
      spine, tested by writing a **red-bordered** file to disk and handing
      `load_cover` a **blue** accent. Any implementation that re-measures fails
      it. A fourth test covers `None → procedural`, in both its forms — with no
      cover file, and with a readable one.
23. **Moments.** Migration `0017` — two tables, both of them state the app keeps
    about *itself*, and no column anywhere near the facts a moment is made of.
    Four kinds (a reading closed, the first mark on a book, a reflection that
    reached another book, a run of days that ended), derived on every ask.
    - **Nothing about a moment is stored, and that is the design.** The obvious
      shape is a `moments` row minted when one is detected; it was refused
      because a stored moment is a thing the app can accumulate, and a count of
      accumulated things is one product decision away from a badge. What cannot
      be derived is what has already been *shown*, so the table is a set of
      opaque ids and a `surfaced_at`, and nothing else. `book_id` was left out
      deliberately: it would earn an `ON DELETE CASCADE` and cost the rule that
      nothing derivable is duplicated, and a row that outlives its evidence is
      inert rather than wrong — its moment can never be re-derived.
    - **The import ruling needed no new column.** `books.created_at` has existed
      since `0001` and is exactly "when this book entered the library", so the
      guard is `evidence_at >= books.created_at` and a 400-book CSV whose dates
      are all older than this afternoon mints nothing. A `books.added_at` was
      designed and then found to be the column already there.
    - **The ruling does not cover an *upgrade*, and that is the correction the
      build forced.** A library that has been here for months has honest
      `created_at`s months old and reading that happened afterwards, so the
      first launch after this migration would replay a whole reading life as a
      ceremony — the same failure the CSV case names, from the other direction.
      Hence `moment_epoch`, one row, written by the migration itself with
      `strftime('%s','now')`: the instant the schema learned what a moment was.
      "When did this database learn about moments" is a thing a migration knows
      and nothing else does.
    - **Both guards err toward silence, and that is the property to keep.**
      `books.created_at` is an *upper* bound on when a book arrived
      (`merge_books` folds onto the older row); a reflection's reach is dated
      `max(reflection.created_at, target.created_at)`, a *lower* bound, because
      `note_links` carries no timestamp — item 21 recorded that and this item
      inherits it rather than fixing it. An approximation that can only suppress
      loses a ceremony; one that can only invent replays a library.
    - **A highlight is dated by the device's clock, never by ours.**
      `highlights.created_at` is when the sidecar was read, so using it would
      make every KOReader import mint a first-annotation moment for every book
      in it — the CSV failure with a different importer. `ko_datetime` decides,
      and a stamp SQLite cannot read as a time is no evidence at all, the rule
      `reading_events`' fillers already follow.
    - **The run is the one that had to be argued**, being a hair from the streak
      this document bans. Three things keep it honest and all three are
      load-bearing: it is recognised **only after it is over** (a run whose last
      day is yesterday is not a moment, because today might continue it), so
      nothing can be shown while it runs and nothing can put pressure on it;
      `RUN_MIN_DAYS` is **2 because that is what "consecutive" means**, not a bar
      somebody chose — a 3 or a 7 would be this module deciding what counts as
      enough reading; and `days` is past tense, the way item 17 permits a count
      of your own highlights. The gap-and-islands is a pure Rust function so the
      partition could be asserted as a property rather than by example.
    - **The per-book guard is applied to the evidence, not to the moment.** A
      day only counts toward a run if it is at or after the day its book
      arrived, so a 400-book import contributes no days and therefore no runs —
      without the run code knowing that importers exist.
    - **`reading_id` is on the moment**, named by item 28's audit before this was
      built and right: a card is minted per *reading*, a reread mints a second
      beside the first, and a moment identified by its book cannot select
      between them. It is absent where the evidence does not settle on one read
      — the call `attribute_highlights` and `reading_for_day` already make.
    - **Acknowledging is idempotent and does not re-derive.** `ON CONFLICT DO
      NOTHING`, so two clients cannot write two rows or move the first
      `surfaced_at`. It deliberately does *not* check that the moment is still
      derivable: `RunEnded` depends on the clock, so a moment that was pending
      when it was shown can legitimately stop being pending a moment later, and
      refusing the acknowledgement then would replay it for ever. A well-formed
      id for a moment that never existed costs one inert row; an id naming no
      kind this build knows is an `InvalidInput`.
    - **No count reaches the wire**, and `the_wire_states_no_number_of_moments`
      asserts the absence rather than trusting the author — including that no
      `count_moments` method parses. `limit` takes from the newest end and is the
      only lever a frontend gets.
    - **Engine and API only.** No CLI verb and no TUI surface, which is item 33's
      complaint in advance: five engine items once landed with no way to see any
      of them. Stated rather than hidden — `rb moments` is the obvious next
      thing and belongs with whoever builds the ceremony.
    - **`make dev-db` shows no moments, and that is the epoch working rather than
      a bug** — worth knowing before somebody debugs it. The seeder creates the
      database (so the epoch is *now*) and then writes a library whose reading
      history is in the past, which is precisely the "history arriving" the
      guards refuse. A seed that wants a live ceremony has to back-date the
      epoch through `Storage::set_moment_epoch`, which exists and is on
      `Storage` rather than `Engine` because nothing in the product asks for it
      yet.
    - **Engine and API only.** No CLI verb and no TUI surface, which is item 33's
      complaint in advance: five engine items once landed with no way to see any
      of them. Stated rather than hidden — `rb moments` is the obvious next
      thing and belongs with whoever builds the ceremony.
