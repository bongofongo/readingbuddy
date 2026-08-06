---
title: Orchestrator handoff — the non-GUI wave (findings 2–8 of the 2026-08-06 handoff)
date: 2026-08-06
source: docs/next-thread-handoff.md (open work 2–8);
        sessions/2026-08-06-the-door-onto-the-cover-back-fill.md (finding 1, closed)
for: an orchestrator thread running several workers, one worktree each
---

# Orchestrator handoff — the non-GUI wave

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation. **You are the orchestrator**: you allocate the numbers,
write the prompts, cut the worktrees, launch one worker per item, merge them in
a stated order, and run the real gate from the main checkout after each merge.
You do not build the items yourself, with one stated exception (item 37).

## Where the tree stands

- `main` at **`70531ec`**, working tree clean, nothing pushed (this repo has no
  remote it uses).
- **`make ci` exit 0** on that commit — fmt, clippy `-D warnings`, plain
  `cargo check --workspace`, `ts-check`, whole-workspace tests, `web-check`, and
  30 Playwright routes on WebKit.
- Highest applied migration is **`0014`** (item 20, cover metrics).
- The GUI half — items 26, 27, 28 and 23 — is **not in this wave** and must not
  be started inside it. Three agents there produce three dialects of one app,
  and whether a shelf reads as a place is the user's call, not an agent's.

## Every database in play is disposable (stated by the user, 2026-08-06)

There is no durable library yet: `dev-data/` is seeded, gitignored and rebuilt
by `make dev-db`, and the user has said all data currently in use is play data
that may be deleted. Three consequences, and they expire the day a real library
exists — **re-check this section before relying on it**.

- **Renumbering a migration is legal.** The "rebase, never renumber" rule exists
  because sqlx checksums each applied migration (sha384) into
  `_sqlx_migrations`, so a renumber is a `VersionMismatch` at startup for
  anybody holding a database. Nobody is. A worker that collides on a number
  therefore **renumbers at merge time** instead of waiting for its predecessor,
  which removes the "a branch holding `0016` is red until `0015` lands" pain
  from parallel worktrees. The reservation shuffle below is still worth doing —
  it costs one commit — but it is now a convenience rather than a constraint.
- **A back-fill is no longer a prerequisite for the column it fills.** Item 34's
  `sort_author` was refused twice on the ground that the column is NULL for
  every existing row until a back-fill nobody has run runs; here the answer is
  `make dev-db`. Still write the door — `rb covers` demonstrated that the door
  is the only thing that ever *exercises* the code, and a shipped library will
  need it — but it does not gate the index.
- **A shape change needs no data migration.** Wipe and rebuild. This is exactly
  how the relative→absolute `cover_path` fix landed on 2026-08-06.

**What is not play data**, and stays untouched no matter what: `personal_data/`
(the opportunistic `partial_md5.rs` checks), the `real/` fixtures, and anything
on a mounted KOReader device.

## Read this before allocating anything: `0015` is not free the way you think

`docs/gui/spec-gui-17-28.md` reserves **`0015` for item 23** (moments), which is
a GUI-wave item and is **not being built in this wave**. Two facts collide:

- `migration_versions_are_contiguous_from_one` (in `crates/engine/tests/`)
  **fails on a gap as well as on a duplicate**.
- So a wave that lands `0016` and `0017` while `0015` is still unwritten leaves
  `main` red.

**Therefore this wave claims `0015` and `0016`, and item 23's reservation moves
to `0017`.** Edit `docs/gui/spec-gui-17-28.md` (three places name the number:
the allocation list near line 37, item 23's own heading, and the dependency
summary near line 530) in your **first** commit, before any worker starts —
a reservation two threads disagree about is exactly the failure the
pre-allocation ritual exists to prevent.

## The items

> **The numbers below are wrong, and were corrected during the run to 34–38.**
> This section reads `docs/prompts/` for the last item number, sees `32-`, and
> proposes 33. But **item 33 was already spent** on 2026-08-05 — "Surfacing
> 21/29/30/31/32", which was minted mid-session and so has a `decisions.md`
> entry and a session log but no prompt file. The register of spent numbers is
> `grep '^[0-9]\+\. \*\*' docs/decisions.md`. Read the mapping as
> 33→34 (search), 34→35 (sort keys), 35→36 (chooser), 36→37 (PDF sidecar),
> 37→38 (fixture parity); migration numbers are unaffected.

Numbers below are **proposed**: 33–37, following item 32. The user allocates
numbers — confirm them in one line before writing the prompt files, and if they
move, the only edits are the prompt filenames and the headings.

Each item gets a `docs/prompts/<n>-<slug>.md` written by the **`new-wave-item`**
skill before its worker starts. Every item's "must not" list is load-bearing:
five of them are things a previous wave already decided and a fresh worker will
cheerfully re-open.

---

### Item 33 — one search surface — migration `0015`

**What.** A `highlights` FTS index, and *one* request that answers notes and
highlights together, plus `title` as a predicate on `BookFilter`.

**Why one request.** Two lists a frontend interleaves is a relevance ordering
invented above the seam. And `find_books_by_title` belongs in the same item as a
**filter**, not a seventh endpoint: item 18 already made every list a filter +
sort + page over one shared clause, and a title search is that clause with one
more predicate.

**Where.** `crates/engine/migrations/0015_*.sql`,
`crates/engine/src/storage/highlights.rs` (the writer),
`crates/engine/src/storage/notes.rs` (`search_notes`, `NoteSearchHit` — the
`snippet()` shape to match), `crates/engine/src/storage/query.rs` (`BookFilter`,
where `author`/`year`/`language`/`tag`/`has_cover` already live and `title`
joins them), `crates/api/src/dto.rs`, `crates/api/src/protocol.rs`,
`crates/api/src/lib.rs`, `gui/src/lib/api/bindings.ts` (**generated** — see the
conflict rules), and a CLI door (`notes --search` exists; the unified one needs
one too, or the surface is unreachable from a terminal the way
`measure_stored_covers` was).

**Decisions the item must make, and the constraints on them.**

- **Triggers or an explicit writer.** `notes_fts` has **no triggers** — the
  engine populates it in application code, and `crates/corpus`' seed writes the
  FTS row itself for that reason. Highlights arrive in bulk from importers, so
  whichever is chosen, *every* insert path must go through it; a highlight that
  is in `highlights` and not in the index is invisible to the one thing item 27
  exists to do.
- **`search_notes` on the wire.** Nothing consumes it (`gui/src` has no
  reference; only the generated bindings mention it). Adding a method does not
  bump `API_VERSION`; changing or removing one does. Recommendation: **replace
  it and bump `API_VERSION` to 2** — it is free now and never again, and leaving
  both doors up is the interleaving problem with extra steps.
- **The privacy rule is not negotiable.** Highlight text, note bodies and search
  queries are the user's private reading: **never above `trace!`**. A new query
  path is exactly where a helpful `info!("searching for {q}")` gets added.

**Done when.** A highlight imported by the KOReader path is findable; a note and
a highlight come back in one ordered list with snippets; `BookFilter { title }`
composes with the existing filters, with the offset paging and with
`count_books`; `make ts` regenerated; `docs/decisions.md` **appended**.

**Must not.** Add a seventh endpoint for titles. Return two lists. Introduce a
second pagination shape (item 18 settled on offset everywhere — the two sorts
with no cursor key are exactly the two whose pages are whole-table reads).

---

### Item 34 — the sort keys get an index, and a writer — migration `0016`

**What.** Indexes on `books.last_modified`, `books.title COLLATE NOCASE` and
`books.publish_year`; a **writer** for `sort_title`; and `sort_author` only
inside the same item.

**Why together.** There is no index on any sort key today, so `ORDER BY title`
sorts the whole table however you paginate — this is what makes a deep page
cheap, and it is what turns item 18's `books.id` tie-break from insurance into
load-bearing. And `sort_author` was refused twice (items 20 and 18) on one
ground: SQLite cannot compute the value, so the column is NULL for every
existing row and `ORDER BY sort_author` is silently *wrong* until a back-fill
that nobody has run runs. It only pays **inside** this item, where the back-fill
and the index arrive together.

**The trap this item is really about.** `sort_title` has **never been computed
by anything**. It is a `MERGE_RULES` column, `Federated::Local`, present on
`Book` and on the DTO — and `BookSort::Title` orders by
`books.title COLLATE NOCASE`, because the column it should have used is NULL
everywhere. **A sort-key column added without a writer looks answered and is
not.** This item either gives it a writer or deletes it; leaving a third state
is what produced the trap.

**The pattern to copy.** Item 20's `invalidate_cover_metrics`: a companion
clause **generated from another column's value expression**, bound from Rust
because SQL cannot derive it, so it cannot fall out of step with the merge rule,
the user guard, or the `dst`-wins inversion. `crates/engine/src/names.rs` already
holds the name arithmetic, so the derivation exists; what is missing is the
wiring and the back-fill. Note that `names::sort_key` returns
**`(u8, String, String)`** — a rank plus two parts — and a column is one TEXT
value, so the item has to decide the encoding and make `ORDER BY` on the stored
string reproduce the tuple's order exactly. A column that *nearly* agrees with
the function is worse than no column, because both exist and only one is read.

**The back-fill needs a door.** `rb covers` (landed 2026-08-06, see
`crates/cli/src/commands/covers.rs`) is the shape: a CLI verb, idempotent, whose
work list is "the rows that have not got one yet", whose wording says
`every … is already …` rather than a zero, and which `make dev-db` runs. A
back-fill with no door is what finding 1 was.

**Done when.** `EXPLAIN QUERY PLAN` shows the index used for each sort (assert
it — a behavioural test cannot see an index, exactly as
`order_by_is_a_total_order` had to read the SQL because removing the `books.id`
tie-break left the behavioural partition test **green**, measured); every write
path that changes a title or an author list moves the sort key with it;
back-fill door exists and `make dev-db` runs it; `docs/decisions.md` appended.

**Must not.** Add `ORDER BY sort_author` as a *second* arm beside the slow one —
it replaces or it does not land. Ship the column without the back-fill.

---

### Item 35 — the chooser knows who wrote it — no migration

**What.** `MatchCandidate` / `MatchCandidateDto` carry the author (and consider
`publish_year`; `cover_path` only if a chooser would actually show a jacket).

**Why.** `koreader::band` (`crates/engine/src/koreader.rs:636`) already holds the
whole `Book` and throws everything but `book_id`, `title`, `score` away — so
"which Dune is this", the **first** screen a refusal sends you to, costs an N+1
`get_book` per candidate, and a chooser that shows only titles cannot answer the
question it is asking. Reported independently by items 22 and 18.

**Where.** `crates/engine/src/koreader.rs` (`MatchCandidate`, `band`),
`crates/api/src/dto.rs`, `bindings.ts` (regenerate). Every candidate-producing
path returns the same shape, so check `sidecar_candidates`, `identify_file` and
`import_calibre_library`/`import_goodreads` all flow through it.

**Done when.** No caller needs a `get_book` to render a candidate row; `make ts`
regenerated. Adding fields to a DTO does not bump `API_VERSION`.

**Must not.** Grow into a matcher change. The band's *membership* is item 22's
decision and is not being revisited.

---

### Item 36 — a real PDF sidecar, and a `Diagnostic` instead of silence — no migration

**What.** A PDF-shaped sidecar in the tier-1 corpus, and a typed degradation
where KOReader's PDF annotations are currently dropped without a word.

**Why.** `entry_to_highlight` requires a **string** `pos0`
(`crates/engine/src/koreader.rs:263`), and KOReader stores a *table* there on
PDF — so those entries are skipped **in silence**. That behaviour is *reasoned,
not observed*: `docs/koreader-format.md` files PDF annotations under
**unobserved** for exactly this reason. Silence is also the wrong answer
regardless of the fixture: the engine's rule is that a partial failure returns a
`Diagnostic` carrying the path and an `ErrorClass`, never nothing and never a
pre-formatted `String`.

**Where.** `crates/corpus/src/synthetic.rs` (tier 1 is committed and covers
*shape* — this is a shape), `crates/engine/src/koreader.rs`,
`crates/engine/src/diagnostic.rs` (`ErrorClass`, if a new class is warranted —
`EngineError::Other` is last-resort and a caller that might branch deserves a
variant), `docs/koreader-format.md` (move PDF out of *unobserved*, or state
precisely what is still unobserved after the fixture).

**Done when.** `make golden` regenerated and the goldens show the skipped
entries as diagnostics rather than as an absence; the count of imported
highlights is unchanged for every existing fixture (this must be a pure
addition); `docs/decisions.md` appended.

**Must not.** Make the fixture by reading a real personal PDF sidecar into the
repo. And **the generator must not depend on `readingbuddy`** — reusing the
engine's own parsing to build its fixtures bakes any bug straight into the
goldens, which is the whole of `crates/corpus`' value.

---

### Item 37 — one fixture, two consumers — no migration — **orchestrator runs this one**

**What.** Assert that `crates/corpus`' `gen-devdb` and `gui/src/lib/api/fake.ts`
agree about the shapes they both claim to model, and give the cover layout a
headless regression test.

**Why it is last and why you run it.** It touches `gui/`, so its gate is
`make web-check` — and a fresh worktree has **no `gui/node_modules`** (it is
gitignored), where `web-check` and `routes` degrade to a stated `SKIPPED:`. A
worker would therefore "pass" them without running them. Run it from the main
checkout after the four merges.

**Why it bites now.** The fake serves **no covers, ever** — a deliberate choice
(`fake.ts:353`), so every tile exercises the no-cover branch and covers are
"checked in the real app, against `make dev-db`". As of 2026-08-06 the real app
finally *can* show them (whole `cover_path`s, 202 measured covers), which makes
the untested half the one item 26 is about to build on.

**Done when.** A test fails when an edge case exists in one fixture and not the
other; the shape of what a cover-bearing tile reserves (`cover_aspect`,
`cover_accent`, `cover_shelf_path`) is exercised headlessly. Note that dev-db's
covers are 240×360 — **below the shelf tier**, so `cover_thumb_path` is NULL for
all 202 and `cover_shelf_path` correctly falls back to the original. A test that
asserts a thumb exists would be asserting the fixture, not the rule.

---

### Deliberately not in this wave

**The duplicated border-median accent arithmetic** (`crates/engine/src/images.rs`
vs `crates/tui/src/render3d/texture.rs`) is a **decision, not a task**:
`images.rs` measures the original file and `texture.rs` measures the *scaled
texture*, so they can legitimately differ, and the renderer is frozen. Deleting
the renderer's copy is a decision about what it draws. Take it to the user; do
not hand it to a worker.

## The schedule, and why it is this one

Collisions, measured against the files above:

| item | its files | collides with |
|---|---|---|
| 33 | `storage/{notes,highlights,query}.rs`, api/*, bindings | 35 (api + bindings) |
| 34 | `storage/books.rs`, migrations, cli | — |
| 36 | `koreader.rs`, `corpus/synthetic.rs`, docs | 35 (`koreader.rs`), 37 (corpus) |
| 35 | `koreader.rs`, api/* | 33, 36 |
| 37 | corpus, `gui/` | 36 |

So:

**Round 1 — three workers in parallel: 33, 34, 36.** Pairwise disjoint. 33 is
the longest pole and starts first.

**Merge order as they land: 34 → 36 → 33.** 33 last because it owns the API
surface; `bindings.ts` regeneration is a whole-file conflict and it is cheaper
to have it happen once, at the end.

**Round 2 — one worker: 35.** Cut its worktree from the **finished** `main`,
after all three merges. It collides with both 33 (api) and 36 (`koreader.rs`)
and is roughly an hour of work — so giving it a base that already contains them
costs nothing and removes both conflicts. This is the previous wave's own
lesson: item 18 had the one real file collision and merged with **zero**
conflicts because its base already contained its four siblings. **Reset the last
worker's worktree onto the finished main rather than rebasing it afterwards** —
rebase-after is what produces semantic conflicts git cannot see.

**Round 3 — you, in the main checkout: 37.** Needs node.

After **each** merge: `make ci` from the main checkout, then `make dev-db`
(it is disposable by design and both 34 and 37 change what it produces).

## Worker mechanics — what a previous wave learned the hard way

- **A worker cannot gate on `make ci`.** No `gui/node_modules` in a fresh
  worktree ⇒ `web-check` and `routes` print `SKIPPED:` and the worker "passes"
  them. Gate workers on **`make fmt lint build-check test ts-check`** plus the
  **`cargo-tester`** agent. `ts-check` needs cargo and no node, so it is the
  cheap guard on a DTO change. **You** run the full `make ci` from the main
  checkout after every merge.
- **Never read a pipeline's exit code.** `make test | tail -25` reports *tail's*
  status; it was 0 over an unread log. Redirect to a file and read `$?`.
- **APFS-clone `target/debug` into each worktree** (`cp -Rc`, near-zero bytes)
  but **strip `incremental/` and set `CARGO_INCREMENTAL=0`** — it was 26G of
  51G, and four diverging copies is the difference between a full disk and a
  comfortable one. Budget ~10 minutes for the clone; it is not instant.
- **A worker can die instantly on a session limit**, and the notification
  carries a one-line transcript that looks like a result. The worktree and
  branch survive untouched, so a relaunch costs only wall clock — but **check
  what actually landed** before believing a short report.

## The two guaranteed conflicts, and their resolutions

- **`docs/decisions.md`.** Three of four merges last wave conflicted there and
  nowhere else. Tell every worker to **append** its entry and restructure
  nothing; the resolution is then deleting three marker lines. It is in **build
  order, not numeric order** — that is deliberate.
- **`gui/src/lib/api/bindings.ts` is generated.** Never hand-merge it and never
  hand-edit it. Take either side wholesale, run `make ts`, commit the result;
  `make ts-check` is what proves you took the right one. (New this wave: two
  items in it, 33 and 35, both change DTOs.)

## Traps that are still live, all of them found by shipping

- **A column with no writer looks answered.** `sort_title` — item 34 is the fix,
  and the habit is: before leaning on a column, check something writes it.
- **A behavioural test that cannot fail is not a guard.** Removing item 18's
  `books.id` tie-breaks leaves the behavioural partition test **green**;
  SQLite's sorter is deterministic for one plan over one set of rows, and that
  determinism belongs to the query plan, not the schema. `order_by_is_a_total_
  order` reads the SQL instead. Item 34 needs the same trick for its indexes.
- **A fixture can disagree with the engine about a column's shape and nothing
  notices** (found 2026-08-06). `gen-devdb` wrote a *relative* `cover_path`
  where the schema holds `images_dir.join(name)`; it was only observable by
  running a command that read the file the path names, and it would have made
  item 26's shelf show zero covers. Item 37 is the general fix; the habit is:
  a path in a fixture is not checked until something opens it.
- **Absence is not zero, anywhere** — `activity`'s "not measured", `rb covers`'
  "every stored cover is already measured", `BookFilter`'s `None` meaning *not
  asking*. New surfaces in this wave (a search with no hits, a candidate list
  with no candidates) land straight on it.
- **No silently-skipping tests.** Every skip prints `SKIPPED:` and honours
  `READINGBUDDY_REQUIRE_FIXTURES=1`. Scope that variable to the nightly corpus
  job only — set globally it fails on `real/` and on `partial_md5.rs`'s
  opportunistic `personal_data/` checks, both absent by design.
- **`crates/corpus` must never depend on `readingbuddy`** (items 36 and 37 both
  touch it).
- **The sandbox proxy blocks gutenberg.org**, so `make corpus` — tier 2 — cannot
  run in a cloud session at all. Tier 1 (`make synthetic`) is committed and
  runs anywhere; item 36 lives there for that reason.

## What "done" looks like for the wave

`make ci` exit 0 on `main` with all five items merged, `docs/decisions.md`
carrying five appended entries in build order, `docs/gui/spec-gui-17-28.md`
showing item 23 on `0017`, a session log per item, and this file superseded by a
fresh `docs/next-thread-handoff.md` whose open-work list is what is left:
the accent-arithmetic decision, and the GUI half — items 26, 27, 28, 23, in
sequence, with the shelf's feel still the user's call.
