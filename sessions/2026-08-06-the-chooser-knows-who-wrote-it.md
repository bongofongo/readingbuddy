---
title: Item 36 — the chooser knows who wrote it
date: 2026-08-06
branch: feat/engine-candidate-author
---

# Session log

Item 36 of the 2026-08-06 non-GUI wave, built alone in a worktree on a base that
already contained items 34, 35 and 37 — deliberately, since this one collides
with 34 on the API and with 37 on `koreader.rs`. No migration. Two fields on one
struct and its DTO, and the rest of the item is the argument about which two.

The subject is four lines that have been in `koreader::band` since item 3:

```rust
Some(MatchCandidate {
    book_id: s.book.id?,
    title: s.book.display_title().to_string(),
    score: s.score,
})
```

`s.book` is the whole `Book`. Everything but three fields was thrown away, so the
screen a refusal leads to — where *which Dune is this* is the entire question —
could not answer it. Two editions with one title and two authors read as the same
row, in the list that exists to distinguish them.

## The framing that decided every field

**A `MatchCandidate` is a row to show, not a record to write.** The only thing any
caller does with one is hand `book_id` back to `link_sidecar` /
`link_calibre_book` / `link_goodreads_row`. Nothing on it round-trips. That one
sentence answered three questions at once, and it is worth stating before the
fields rather than after, because each of them falls out of it.

## `authors_display`, and no raw `authors` beside it

`BookDto` carries both `authors` (what the origin spelled) and `authors_display`
(item 17's parse, `names.rs`), because it is a record that goes back in. A
candidate is not, so a raw list here would be a second spelling of the same names
to keep in sync for a row nobody writes.

Which of the two crosses was not a toss-up. The **GUI links
`readingbuddy-api` and not the engine**, so a raw `authors` would be
`names::display_order` re-implemented in TypeScript — precisely the re-derivation
item 17 spent an item removing. The parsed form is the only one a client can use.

Named `authors_display` on both the engine struct and the DTO, matching
`BookDto`, because one idea spelled two ways is how two frontends end up
disagreeing. The **join** between names stays wording and stays a frontend's,
which is why it is a `Vec<String>` and not a sentence.

The test asserts the spelling and the absence, in that order: the library stores
`Lee, Min Jin` and the candidate says `Min Jin Lee`, and a book the library holds
no author for still produces a candidate with an empty list rather than a
placeholder.

## `publish_year` in, `cover_path` out

The prompt asked me to *consider* both. They came out opposite ways and the
reasons are different in kind.

**The year is in.** The band's ordinary case — a variant title of a book already
on the shelf, a subtitle dropped, a translator's spelling — usually has the
*same author on both rows*, so title and author both tie and the year is what is
left to choose by. A 1965 Dune against a 2005 reissue. It costs a fixed eight
bytes and no allocation.

**The path is out**, on the prompt's own test: *only if a chooser would actually
show a jacket*. None does. The TUI's `link_picker` is a text list, the CLI prints
lines, and the GUI has no chooser at all. And there is a second reason that
outlives the first: when a chooser does draw one, the field it needs is **not
this one**. Item 20 made `cover_shelf_path` what a grid loads, beside
`cover_aspect` and `cover_accent`, because a frontend reading `cover_path`
directly shows nothing for every cover small enough to have no thumb. So the
honest addition is a *cluster*, and it belongs to the screen that draws it.
Shipping the raw path now would be shipping the field a frontend gets wrong.

Both directions stay cheap, which is what makes deferring safe: adding to a DTO
is additive and does not move `API_VERSION`.

## The pushback the prompt nominated, measured

The prompt named its own likeliest error: *the honest answer may be that a
candidate should carry the whole `BookDto`, since `band` already holds the `Book`
and the picking is what created this bug* — and asked for a measurement rather
than an assertion.

Measured. A `BookDto` for a book with a real 681-character publisher blurb
serializes to **1846 bytes**; the candidate is **98**. That is 19×, and it is
still **10×** (1032 B) with `description`, `first_sentence` and `subjects` all
stripped, because `BookDto` is thirty-odd fields wide before any prose. Real
provider descriptions run longer than 681 characters, so 19× is the floor.

Weight alone would not settle it. Two more things do.

Candidates are produced **per row**: `import_calibre_library` and
`import_goodreads` attach a band to every unmatched row they report, so a
four-hundred-book library with a few near-misses each is megabytes of prose that
no chooser reads.

And `BookDto` answers a *different question*. It carries `progress` and
`reading_status`, which would invite a chooser to draw a progress bar on a book
it is asking you to identify. A candidate list is about identity; reading state
belongs to the book you have already decided this is.

The asymmetry is what closes it: picking wrong again costs **one more additive
field**, paid by whoever draws the screen that needs it. `BookDto` costs every
import report for ever. So the answer is no — but the prompt was right that the
picking is the hazard, which is why the next section exists.

## `band` is the only constructor, and structurally so

All five candidate-producing paths named in the prompt were audited:
`match_candidates` / `Engine::sidecar_candidates`, `files::identify` and
`import_file`, `import_calibre_library`, `import_goodreads`. **None builds a
`MatchCandidate` by hand.** The type system already forbids it — `band`'s input
`Scored` is `pub(crate)` and minted in exactly one place (`scores_for`), so there
is no way to reach the band without going through the constructor.

That is why **no `Default` impl was added**, which the prompt warned against: it
would have let a real call site forget the field silently, in exchange for saving
churn in test fixtures.

What guards it going forward is not the constructor, though — a *new* path could
always build one from something other than a `Scored`. It is the tests. Each of
the four surfaces' existing candidate assertions now checks that the author
arrived:

| surface | test |
|---|---|
| `match_candidates` / `sidecar_candidates` | `koreader.rs` → `a_candidate_carries_who_wrote_it_and_when` |
| `device::scan_device` | `device_scan.rs` → `a_new_row_carries_the_books_it_might_already_be` |
| `files::import_file` | `book_files.rs` → `level_3_refuses_to_create_over_a_candidate_and_writes_nothing` |
| `import_goodreads` | `goodreads.rs` → `a_near_miss_is_reported_with_candidates_rather_than_duplicated` |
| `import_calibre_library` | `calibre.rs` → `a_near_miss_is_offered_rather_than_duplicated_unless_asked` |
| the wire | `dto.rs` → `a_candidate_crosses_the_seam_knowing_who_wrote_it` |

Each seeds its library book with the author in the origin's spelling
(`Lee, Min Jin`, `Mandel, Emily St. John`) and asserts the *display* form comes
back, so the test would fail on a candidate that carried the raw string as well
as on one that carried nothing.

One trap while doing this: `matching.rs` reads authors as a **veto**, so seeding
an author on the library side can silently drop a book out of the band and turn a
green test into a green test that asserts nothing. Every seeded author was chosen
to agree with what its source actually carries — which meant switching
`book_files.rs` to `write_isbnless_epub_by(&src, "Pachinko", "Min Jin Lee")`,
since the default helper writes `A Test Author`. The score is title-only, so an
agreeing author moves nothing.

## What was deliberately not touched

**No CLI output, no TUI picker row, no GUI.** The prompt scoped it — *no TUI
beyond keeping it compiling* — and the scope is right. The item is about what a
candidate *carries*; what a screen does with it is that screen's decision, and
`ui/goodreads.rs` already has a regression test
(`a_long_candidate_title_never_clips_the_next_move_away`) about what happens when
a candidate row grows without the width budget being re-argued. The engine half is
the half that could not be worked around from above.

The band's **membership** is untouched. `CANDIDATE_MIN`, `can_auto`, the
`then(a.book.id.cmp(&b.book.id))` tie-break: all item 22's, all unchanged.

## The N+1 that was not there yet

The "done when" asked me to find the callers doing a `get_book` per candidate and
show they are gone. **There were none to delete**, and that is worth recording
rather than glossing.

The cost was structural, not committed. Every frontend that has drawn a candidate
row so far draws only a title — because a title is all it had. The N+1 was what
the *next* screen would have had to pay, which is exactly why items 22 and 18 both
reported it as a gap from the outside rather than as a bug in something they were
building.

## The churn, counted

Fourteen `MatchCandidate` construction sites in the TUI, all test fixtures
(`app.rs` ×8, `ui/calibre.rs` ×2, `ui/goodreads.rs` ×2, `ui/device.rs` ×1, plus
the two in one `vec!` in `app.rs`). They were given real authors and years rather
than `vec![]`/`None`, since a fixture that says nothing about a field is a fixture
that would not notice the field disappearing — except one, deliberately, which
carries an empty author list because that is the ordinary shape of a
sidecar-seeded book and a chooser has to draw it.

`make ts` regenerated `bindings.ts`: one type changed, `MatchCandidateDto`, plus
its doc comment. Nothing in `gui/src` reads it, so no frontend broke.

## Gate

`make fmt lint build-check test ts-check` — all green, exit 0, read from a
redirect rather than through a pipe. `make ci` was not run: a fresh worktree has
no `gui/node_modules`, so `web-check` and `routes` would print `SKIPPED:` and pass
without running. `make corpus` was not run — tier 2 needs gutenberg.org.

The four `ts-rs failed to parse this attribute` warnings are pre-existing —
`#[serde(other)]` on `ErrorCode::Internal`, documented in `crates/api/CLAUDE.md`
and deliberately not silenced.

`API_VERSION` stayed at **2**. Both new fields are `#[serde(default)]`, and
`a_candidate_crosses_the_seam_knowing_who_wrote_it` parses a three-field payload
written against the old shape to prove the additive growth is real rather than
claimed.

## Two stale references, reported rather than edited

- `docs/handoff-orchestrator-non-gui-wave.md:198` still calls this **item 35**.
  The wave was renumbered +1 mid-run because item 33 was spent by a mid-session
  item with a `docs/decisions.md` entry and no prompt file. Left alone; the
  register of spent numbers is `docs/decisions.md`, not that file.
- `docs/next-thread-handoff.md:139` lists this gap as open (entry 5 of 7). Now
  closed by this item, but left in place: entries there are numbered, the sibling
  items in this wave closed their own entries without touching it, and the
  orchestrator reconciles that file once rather than five times in five branches.

## Left for later

- **A chooser that draws a jacket**, and the `cover_shelf_path` /
  `cover_aspect` / `cover_accent` cluster it would need. Additive when somebody
  builds the screen; a decision for that screen and not for this item.
- **The TUI picker still shows title and score only.** It now has the author in
  hand and does not draw it. That is a layout item with a real constraint behind
  it (`DETAIL_MAX`, and the clip regression `ui/goodreads.rs` pins), not an
  oversight.
