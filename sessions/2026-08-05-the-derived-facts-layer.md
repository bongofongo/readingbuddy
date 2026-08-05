---
title: Item 17 — the derived-facts layer, and the four things it refused to move
date: 2026-08-05
follows: sessions/2026-08-05-gui-scaffold-and-the-seam.md
---

# Session log

The handoff named one gap in a green build (`cargo deny` had not been run since
the Tauri tree landed) and one item to do next (17, with its prompt already
written). Both are done. `make ci` exit 0.

`cargo deny check bans licenses sources` — **ok on all three**. A few hundred
crates arrived with Tauri 2, `ts-rs` and `png`, and `deny.toml` is
permissive-licences-only with five named exceptions; none of them needed a sixth.
The `bans` half reports duplicate-version *warnings* only — `zip` 2.4.2 beside
3.0.0 (the pinned `epub = "=2.1.4"` holds the newer one), plus `toml_edit` and
the usual windows-sys spread. Nothing denied.

## What item 17 is about, restated

`crates/cli/src/commands/goodreads.rs` states the rule the codebase follows:
*"All the printing lives here — the engine does no terminal I/O."* That rule is
right. It had been read as *the engine does no derivation*, and the cost was
visible the moment a second frontend existed: the GUI could not sort by author,
could not show a percentage honestly, could not tell *put down* from *reading*
without a stringly comparison, and reconstructed the series pair itself.

The fix is not to move formatting down. It is to move the **values** down and
leave the *rendering* up. A `Progress` enum is not terminal I/O; `"p.42"` is.

## Decisions locked

- **`Progress` is one type, and the four implementations disagreed.** The CLI's
  had no `total > 0` guard, so `make dev-db`'s zero-page-count book printed
  `[12/0]` — a **false denominator**, not a crash, which is exactly why nothing
  ever caught it. `Progress` normalises a zero length to *absence*, so no caller
  can reach one, and a property (`a_reported_length_is_never_zero`) states that
  as a rule rather than as one more example.
- **Pages win where they can answer; the device fills in where they cannot.**
  The `ko_percent` fallback lived in one screen of one frontend
  (`tui/src/ui/home.rs`). That meant every list built off `Book` alone — every
  GUI list — showed *nothing* for the commonest row in a KOReader-sourced
  library. It is now the only copy of the rule, and the TUI's library list
  inherited it: a visible behaviour change, asserted.
- **`Book` gained a sixth reading projection, `ko_percent`.** Not a schema
  change: `readings.ko_percent` has existed since migration `0005`. It is what
  makes `Progress` computable from a `Book`, which is what a list needs, and it
  is the same move `reading_status` made in item 25.
- **`percent` crosses the wire beside `fraction`, and is not redundant.** The
  page-based percentage is an *integer division*. `29/100` is
  `0.28999999999999998` in binary, so `Math.floor(fraction * 100)` gives **28**
  where the division gives 29. Two frontends already did it in integers and were
  right to; carrying `percent` is how the third gets the same number.
- **`ReadingState` is typed and `readings.status` stays a `String`.** The
  argument for the string was about storage and still holds — an importer can
  write a status this build does not know, and a parse that refused one would
  turn a foreign device's vocabulary into an error on the read path. It was never
  an argument for handing every frontend the same `switch` over three magic
  words, where the one that misspells `abandoned` styles a put-down book as an
  active read. `Other(raw)` carries the unknown value whole — the shape
  `KoStatus` already had, so the two enums are deliberately twins.
- **There is no `NeverOpened` variant.** A book with no reading is
  `Option::None`. Naming it puts *unread* into the type system, and a variant is
  a thing a UI filters on, counts, and eventually puts a badge beside — the
  framing `docs/decisions.md` bans. It is also the honest answer: the engine
  knows there is no reading, not that the book has not been read.
  `no_reading_is_absence_rather_than_a_variant` pins it at the seam.
- **Author names moved into `readingbuddy::names`.** `last_name`, `PARTICLES`
  (18 entries) and `SUFFIXES` were the TUI's private knowledge; a GUI without
  them files *The Overstory* under nothing. `display_order` is the same reading
  of the comma run backwards, and the two share `comma_is_only_a_suffix` — so
  the sort and the label can never name different surnames.
  `filing_survives_the_flip` asserts that as a property, and is **scoped
  honestly**: a given name that is also a particle (`de`) and a surname that is
  also a suffix (`Ii`) are genuinely ambiguous once the comma is gone, and the
  property says so rather than being weakened silently.
- **`BookSort` gained `Author` and `Year`, and `limit` selects along the sort
  key in every arm.** That is what `LIMIT` means, and a paginated shelf needs it.
  The TUI's opposite policy — fetch one page by recency, reorder *that page* in
  Rust, so `s` reorders the list rather than swapping its contents — is a
  decision about a fixed page and stays in the TUI, now with a paragraph saying
  the two coexist. `Author` cannot be an `ORDER BY` at all, so its arm reads the
  library, sorts in Rust, and truncates; the base order is recency and
  `sort_by_key` is stable, so two books by one author keep newest-first
  underneath. A `sort_author` column is the follow-on and needs a migration.
- **`CalibreReport::row_state`, `CalibreRowState::is_importable` and
  `ReadNumbering` moved down** (17c/17d/17e). Each was a domain join sitting in
  frontend state. `ReadNumbering` is the sharpest: it silently depended on
  `list_readings`' oldest-first ordering contract, so a second frontend would
  disagree with `rb show` about which read a highlight came from and *nothing on
  either screen would look wrong*.

## Four things it refused to move, which is the part worth re-reading

The prompt said to push back. These are where it did.

- **Dates.** Nothing moved. "3 days ago" needs an answer to *what is today*, the
  engine's day convention is UTC, and inventing a local-time answer is precisely
  what item 31 refused for reading minutes. The same refusal applies; the
  timestamps already cross as integers and that is the whole of what a frontend
  needs.
- **Absence wording.** `titleLabel` and the empty-authors case stayed in
  `phrasing.ts`. The rule chosen, of the two that were both plausible: **the
  engine states the absence, the frontend words it.** `title` is `NULL` and
  `authors` is `[]` on the wire — that is the engine stating it. *Untitled* is a
  word.
- **`ReadingDto.source`.** Still a `String`, and `status` is not. The three
  reasons `status` earned an enum — an engine type to mirror, a closed
  vocabulary, frontends branching on it — are all absent here: `source` is the
  *name of a writer*, it grows by one every time an importer is added, and
  nothing branches on it. An enum would be a second list of importers to keep in
  step with the first.
- **A per-row summary of what is behind a book.** Reassigned to **item 18**. It
  is a query shape, not a derivation — the detail screen makes four calls for one
  book, which for a list is eight hundred — and item 18 is "list endpoints that
  survive a real library". Noted against the axiom: a count of *your own
  highlights* is past tense and allowed; a count of what you have not done is
  not.

## What the frontend gave up, which is the test of whether it landed

Item 17 is only done if the derivations *left* the Svelte. They did:

- `seriesLabel` is **deleted**; `book.series_label` is the engine's.
- `authorsLabel` now joins `authors_display` and never touches `authors`.
- `readingStateLabel` switches on a typed union.
- `progressLabel` / `progressDetail` are new and do **no arithmetic** — two
  phrasings of one value, which is the frontend's half working as designed (the
  TUI words the same `Progress` four ways).
- `fake.ts` carries the derived fields as **literals**. A fake that re-derived
  them would be a second implementation of the rules this app exists not to have
  twice, and one that agreed with itself no matter how wrong it was.

The screenshots are the check that can see, and they show it: *A Book Filed
Under Surname* now reads **Jorge Luis Borges**; *The Doorstop* reads *Reading ·
35%*; *A Book Of Zero Pages* reads *Reading · p. 0* with no percentage at all;
*A Book I Put Down* reads *Put down · 20%*, styled like every other row.

## Corrections this build forced

- `From<Book> for BookDto` computes the derived fields **before** the struct
  literal, because the literal moves the fields they read. Obvious in hindsight,
  a compile error the first time.
- `home.rs`'s test fixture let a `Book`'s `current_page` disagree with its
  `Reading`'s. On real data they cannot — the book's is a projection of the
  reading — so the fixture was testing the fixture. Fixed rather than worked
  around.
- The engine's `Progress::percent` clamps and uses `saturating_mul`, because
  `page * 100` is reachable with an i64 from an importer.

## Still open

- **`sort_author` as a stored column.** `BookSort::Author` reads the whole table.
  Correct and simple; a library where it hurts is a library large enough to have
  said so. Needs a migration, so it is not item 17's.
- **`highlights.color` (G4)** and **the Google Books cover collision (G3)** are
  where the prompt left them — item 27 and item 20 respectively. Neither was
  touched here.
- **The two fixtures can still diverge.** `gen-devdb` and `fake.ts` now have more
  to keep in step, not less: every derived field is spelled out in the fake.
  Unifying them is still open work.
