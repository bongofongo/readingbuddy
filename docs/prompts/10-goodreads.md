# Prompt — Item 10: Goodreads CSV, in and out

Paste into a fresh session at the repo root, on branch `feat/engine-goodreads`.

---

Read `docs/spec-08-10.md` (item 10) and `docs/decisions.md` (the **Goodreads**,
**Ratings** and **Collections** sections) before starting. `CLAUDE.md`'s
**Engine standards** section is binding.

**Engine + CLI + corpus. Owns migration `0009`, and merges after `0008` (T9a).**
No TUI.

This is the item `docs/decisions.md` says "needs 3, 5, 7 to import losslessly".
All three are in, which is why it is possible now and was not before.

## Migration `0009_goodreads.sql`

- `book_tags(book_id, tag, source, raw, PRIMARY KEY(book_id, tag, source))` —
  `Bookshelves` as **inert provenance**: raw value plus source, no UI, no merge
  semantics, until collections are designed. Three systems minting collections is
  a merge problem with no good default, and the data is preserved meanwhile so a
  later design can be made against real data.
- `external_ids(source, external_id, book_id, PRIMARY KEY(source, external_id))`
  — Goodreads' own `Book Id`, and what makes re-importing the same CSV
  idempotent. **General rather than a `goodreads_id` column**, because Calibre
  (item 13) needs exactly the same table and two of them would be one too many.
- `rating_scales.is_default`, back-filled to mark the current newest scale, with
  `active_rating_scale()` reading the flag instead of the ordering.
- Seed a `goodreads` scale (min 0, max 5, step 1) with an identity `rating_map`.

**`is_default` is load-bearing, not tidying, and it is the trap in this item.**
`active_rating_scale()` is `ORDER BY created_at DESC, id DESC LIMIT 1`
(`crates/engine/src/storage/ratings.rs:153-159`). Seeding a `goodreads` scale
therefore makes it the *active* one, and `rb rating show` and `set_rating` would
silently start defaulting to Goodreads' integers. Without the flag, this item
changes the meaning of a command it has nothing to do with. Add a test that the
default survives seeding.

## Import — `Engine::import_goodreads(&Path, dry_run) -> GoodreadsReport`

Dependency: `csv = "1"` (Unlicense OR MIT — resolves under the existing
allowlist, but run `cargo deny check bans licenses sources` in the PR and say so).

Five things that will be got wrong if they are not held in mind:

1. **`ISBN` and `ISBN13` arrive Excel-armoured**, as `="0316769487"` — Goodreads
   writes them that way so spreadsheets do not eat the leading zero. Strip the
   armour, then put them through `normalize_isbn` like everything else. No
   exceptions, per `CLAUDE.md`.
2. **Matching order is ISBN13 → ISBN10 → the existing `search.rs` jaro-winkler
   fingerprint.** *Do not invent a second matcher* — `docs/decisions.md` names
   that rule under **Files** and it applies wherever books are matched. Below the
   auto-match band, report unmatched **with candidates**, reusing
   `UnmatchedSidecar`'s vocabulary so unmatched is a decision and not a dead end.
3. **`Exclusive Shelf` maps onto `readings`**, not onto a status column: `read` →
   a finished reading, `currently-reading` → an open one, `to-read` → **a book
   with no reading at all**. That last one is the honest encoding, and it is why
   `to-read` needs no collection to live in.
4. **`Read Count > 1` means that many readings**, `source = 'goodreads'`. Only the
   most recent carries `Date Read`; the rest get NULL dates. Inventing dates for
   the earlier ones would be worse than admitting we do not have them — the same
   argument that leaves `reading_id` NULL on unattributable highlights.
5. **`My Rating` is stored raw, against the `goodreads` scale.** It is an integer
   0–5 where 0 means unrated. *Never reverse the `rating_map`* to guess an
   equivalent on the user's own scale: the map is many-to-one, so its inverse is a
   guess, and `docs/decisions.md` says store the raw value plus the scale id.

`My Review` → a note of `kind = 'review'` on that reading, respecting
`idx_one_review`. `Private Notes` → a plain note titled for its origin: it is
per-book prose that is neither a review nor a reflection, and folding it into
either would put words in the user's reflection that they did not write there.

Re-importing the same CSV must create no second reading, no second review and no
duplicate tag rows.

## Export — `Engine::export_goodreads() -> (String, Vec<Diagnostic>)`

Mirror `export_flashcards`'s shape: return the payload, let the caller write it.

Only the columns Goodreads' importer reads: `Title, Author, ISBN, My Rating,
Date Read, Date Added, Bookshelves, Review`.

Two honest failures, both `Diagnostic`s rather than silence:

- **An unmapped rating skips its row and says why.** `Engine::goodreads_rating`
  already returns `EngineError::UnmappedRating` and its doc comment says to do
  exactly this. A rounded star is precisely the failure the explicit lookup table
  exists to refuse — formulas are always wrong at the ends.
- **Rereads lose all but the most recent.** Goodreads' CSV has no read-count
  column on import. Export the latest reading and report the dropped ones.
  Truncating silently would look like data loss on the far side.

## Fixtures

`crates/corpus` gains `gen-goodreads`. It keeps the crate's rule: **no dependency
on `readingbuddy`**, because reusing the engine's own parsing to build its
fixtures bakes any bug straight into the goldens.

Beside it, two or three **small hand-authored CSVs** — and put the reason in
their header, because it bumps against "fixtures are generated, not
hand-written". A Goodreads CSV is a *recorded artifact of another system*, like
the three KOReader checksums in `docs/koreader-format.md` §5. Generating one from
our own understanding of the format would prove only that we agree with
ourselves. They pin the shapes only a real export has:

- Excel-armoured ISBNs, and a row with no ISBN at all
- an empty `My Rating` beside an explicit `0` — different meanings
- `Read Count 3` with one `Date Read`
- a comma and a double quote inside a title
- CRLF line endings

**The property worth having:** export → import → export is stable. It covers the
mapping table, the armour stripping and the reading reconstruction in one
assertion.

## CLI

`readingbuddy goodreads import <file> [--dry-run]` and
`goodreads export [-o file]`. `--dry-run` prints what would change and names the
flag that would do it — the same refusal-with-a-next-move shape as `ko sync`.

Note `crates/cli/tests/cli.rs` now drives the real binary as a process, and it
has a golden of the **subcommand name set**. Adding `goodreads` will fail it;
updating that list in the same commit is the point of it.

## Done when

`make ci` is green, `cargo deny check bans licenses sources` is ok, the
`cargo-tester` agent reports clean, and the round trip has been run by hand:
import the fixture CSV, `rb show` a reread book and confirm it names three
readings, then export and diff against the input. The PR body says what changed
and what was deliberately left out.
