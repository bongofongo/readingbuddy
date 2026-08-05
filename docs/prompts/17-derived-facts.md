---
title: Item 17 — the derived-facts layer
date: 2026-08-05
source: docs/gui/spec-gui-17-28.md item 17 for the design; this file adds what
        building the GUI slice (item 25) discovered that the spec predates
follows: sessions/2026-08-05-gui-scaffold-and-the-seam.md
---

# Item 17 — the derived-facts layer

> **Landed 2026-08-05.** Kept as the record of what was asked and what was
> decided — including the four things deliberately *not* moved, which are the
> part a later thread is most likely to re-open by accident. The rulings are in
> `docs/decisions.md` entry 17; the session log is
> `sessions/2026-08-05-the-derived-facts-layer.md`. Everything below is the
> brief as written, unedited.

Paste this into a fresh session at the repo root. Read `CLAUDE.md`, then
`crates/engine/src/storage/CLAUDE.md`, then item 17 in
`docs/gui/spec-gui-17-28.md`. **No migration** — this item is projections and
pure functions over columns that already exist.

## Why this list is trustworthy now in a way the spec's was not

The spec's own closing rule says it: *"items 17 and 21 in particular are designs
made from an audit rather than from writing the code, and the thread that writes
them will know something this document does not."*

Item 25 was built first on purpose, so that item 17 would be designed against a
frontend that exists. **Every entry below is a derivation a real screen actually
wanted and was refused.** They are not a survey of what an engine might offer.
The refusals are recorded in `gui/src/lib/phrasing.ts`, `gui/src/lib/api/fake.ts`
and the two route files, each at the point where the temptation was.

**Push back rather than comply.** Four of five threads in the last wave did and
each time they were right. In particular: if one of these belongs in a frontend
after all, say so and say why — the line drawn here ("the engine does no
phrasing; the values being phrased are not the frontend's") is the one to argue
with.

## What must NOT happen

- **No prose.** Not one string a user reads. `ReadingState` is a value, not the
  word "Reading"; a percentage is a number, not `"43%"`. The engine does no
  terminal I/O and by extension no phrasing, and the GUI and TUI must be free to
  word the same value differently.
- **No new columns and no migration.** `0014` belongs to item 20 and `0015` to
  item 23. If something here seems to need storage, that is a finding to report,
  not a migration to write.
- **Nothing that counts what the user has not done.** No "books remaining", no
  "unread count", no streak, no goal. `docs/decisions.md` bans the framing by
  name and `gui/tests/routes.spec.ts` asserts against the words.
- **Do not change `Book::finished`.** It is load-bearing for `render.rs` and
  `progress_tag`. `reading_status` was added *beside* it for the same reason.

## Already landed, so do not rebuild it

**`Book::reading_status` crossed in the item 25 pass** (`reading` | `finished` |
`abandoned` | `None`), as a fifth read-only projection of the current reading
beside `current_page`/`finished`/`date_started`/`date_finished`. It is a `String`
because an importer can write a status this build does not know.

The question this item has to answer is whether that becomes a **typed
`ReadingState`** with the "never opened" case named — and if so, whether the raw
string stays. Two arguments were left open:

- The frontend currently maps the string to a word in a `switch` with a default
  arm for the unknown case. A typed enum with `#[serde(other)]` would make the
  default arm honest instead of dead.
- `None` ("no reading at all") is the commonest state in a real library and is
  currently absence rather than a variant. Naming it `NeverOpened` risks reading
  as *unread*, which is the banned framing wearing a type. `pins the word` is the
  frontend's job either way; deciding whether it is a *state* is this item's.

## The derivations the slice was refused, in the order it hit them

### 1. Progress — and the two false denominators *(highest value)*

`BookDto` carries `current_page` and `page_count`, and the book view shows the
page number alone because a percentage is not computable honestly here:

- **`page_count = 0`** — a real book in `make dev-db` (id 1). Any percentage is a
  divide by zero, and the honest answer is that there is no percentage.
- **`page_count = NULL`** — also real (id 2). Absence, not zero. A progress bar
  must not draw an empty track, which is what every `unwrap_or(0)` produces.
- **`ko_percent` exists on `ReadingDto`** (0.0..=1.0, device-owned) and is a
  *better* answer than pages for a book with no page count, but choosing between
  them is exactly the derivation that must not live in two frontends.

So: a `Progress` type whose absence is representable, with the `ko_percent`
fallback decided **once**. Note `readings.ko_percent` is device-owned and
refreshed by straight assignment, so it can disagree with `current_page`; which
wins is this item's call and needs writing down.

### 2. Author names

`authors` is `Vec<String>` exactly as an origin spelled it, and the dev library
holds `Borges, Jorge Luis` (calibre/Goodreads form), `Colette` (mononym), `[]`
(no author, real after a bare epub import) and three-author books.

The frontend joins with `", "` — phrasing, legitimately its. What it refuses to
do is **reorder `Surname, Given`**, because that is parsing and the TUI needs the
same answer. Wanted: the display order as a value, with the mononym and the
empty case answered rather than special-cased at each call site.

### 3. Sorting

`BookSort` is `LastModified | Title | Progress`. The GUI shows books in
`LastModified` order and does no client-side sorting at all, on the strength of
the comment at `crates/tui/src/app.rs:1034`: a SQL `LIMIT` makes the sort key
decide **membership**, not just order, so a client-side re-sort of a limited page
is a different set of books quietly relabelled.

`FakeClient.listBooks` therefore **ignores its `sort` argument on purpose**, so
the fake cannot be the place that rule gets broken and still looks tested.

Wanted: by author and by year at minimum (neither exists), and `sort_title`'s
role settled — it is one of only two columns that sit out the federated merge, so
it is ours, and nothing currently derives it.

### 4. The series pair

`series` + `series_index`, where the index is a `REAL` so a naive render gives
`#2.0`. The engine already owns `series_index_text` and `Book::series_label` for
precisely this — the session log for the surfacing wave says *"deciding what the
pair means together"* is the engine's.

But **`series_label` is not on `BookDto`**, so the frontend reconstructs the pair
from two fields and formats the index itself. It happens to agree because JS
prints whole floats without a decimal point — which is agreement by coincidence,
not by construction, and is the exact drift `series_label` exists to prevent.
Either surface it or say why not.

### 5. Dates

`created_at`, `last_modified`, `date_started`, `date_finished`, `ReadingDto`'s
timestamps and `reading_events.day` (TEXT `YYYY-MM-DD`, UTC). The slice shows
**none** of them, because "3 days ago" is arithmetic and the engine's own day
convention is UTC while a user reads in local time — the item 31 day-skew note is
the warning here.

Wanted: whatever a frontend needs to render a date without deciding what "today"
means. Note that inventing a local-time answer is what item 31 deliberately
refused for reading minutes; the same refusal may apply.

### 6. Row state for a list

The library grid shows a state chip per row and needed exactly one thing beyond
`reading_status`: whether a book has anything *behind* it — highlights, notes, a
file — so a tile can indicate it without one request per row.

The detail screen makes **four calls** for one book (`get_book`,
`list_readings`, `list_highlights`, `list_notes`) and that is recorded in the
route file rather than worked around. For a *list* it is 800 calls, so no list
can show it. Whether that is item 17 (a derived per-row summary) or item 18 (a
list endpoint that carries counts) is a real question — but note the axiom: a
count of *your own highlights* is past tense and allowed; a count of what you
have not done is not.

### 7. Empty and absent, as a vocabulary

Four different absences reach a screen and each needs a different rendering:
`title = NULL` (untitled), `authors = []` (nobody), `cover_path = NULL` (no
cover), `page_count = NULL` (unknown length). The frontend answers each with a
literal today (`titleLabel`, `authorsLabel`, a hatched box). At least the first
two are arguably phrasing and should stay — worth **explicitly deciding**, since
"the engine states the absence, the frontend words it" and "the frontend detects
the absence" are different rules and only one can be true.

## Two engine items this pass turned up that are NOT item 17

Both came from the `api-surface-auditor` run before the slice was built. Record
them where they belong; do not fix them here.

- **G3, and it is a live bug on `main`.** `images::filename_from_url`
  (`crates/engine/src/images.rs:17`) names a cover file after the URL's last path
  segment. A Google Books thumbnail is `.../books/content?id=…`, so **every**
  GB-sourced cover writes `images_dir/content` and the last import wins — two
  books show each other's cover. Epub extraction (`slugify(title)`) collides on
  two editions of one title. The fix is to content-address the filename, which is
  the sha256 pattern `files.rs` already established and which also makes the write
  idempotent and gives `FetchCover` a free skip-if-present. **Assigned to item
  20**, which rewrites cover storage anyway. Invisible in a single-provider
  library, and `make dev-db` generates its own covers, so nothing here will catch
  it.
- **G4.** `highlights.color` has existed since migration `0001` and the KOReader
  importer writes it, but it is not on the `Highlight` domain struct, not in
  `HIGHLIGHT_COLUMNS` and not on `HighlightDto`. A highlight list cannot show
  what the reader marked in yellow versus blue. One column, one field, no
  migration — item 27's business, named now so the highlight list is not built
  twice.

Also open, and smaller: **`ReadingDto.status`/`source` and `NoteDto.kind` cross as
bare `String`s** while `NoteKindDto` and `KoStatusDto` are already exported enums
the DTOs do not use. Mirroring them turns a stringly comparison in Svelte into an
exhaustive `switch`. `DiagnosticKindDto` is the precedent.

## Done means

- `make ci` exit 0 — which now includes `ts-check`, `web-check` and `routes`.
- `make ts` run and `gui/src/lib/api/bindings.ts` committed in the same change as
  any DTO edit. CI fails on a stale copy.
- Every derivation this item lands is **deleted from `gui/src/lib/phrasing.ts`**
  or from the route that was doing it, and the frontend now reads the engine's
  answer. A derivation that lands in the engine and is left duplicated in Svelte
  is worse than either alone.
- The corrections this build forced are written into `docs/decisions.md`, as
  every landed item's are.
- A session log, via the `wrap-session` skill.
