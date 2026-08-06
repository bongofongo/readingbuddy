---
title: The GUI wave — eight items, seven threads, one orchestrator
date: 2026-08-06
items: 26, 39, 23, 40, 27, 42, 44, 28 (merge order)
handoff: docs/next-thread-handoff.md, rewritten
---

# The GUI wave

Started from "what is the next step in the project?". Ended with the GUI wave
closed: items 25–28 all merged, plus the engine work they turned out to need.

## Decisions locked (all the user's)

- **The WebGL spine shelf is deferred as cosmetic, not abandoned.** Item 26 was
  rebuilt around that rather than half-built toward it.
- **The shelf's layout is a seam, toggleable, with room for future
  arrangements.** This was the user's addition to the ask and it became the
  substantive half of item 26.
- **The card is per-book now; the wall across the library is later** (needs item
  43). Item 28 shipped the per-book form.
- **A number on the home surface may describe one book, never the collection.**
  Resolved a three-times-stated absolute rule against three waves of shipped
  behaviour, in favour of the behaviour — with the rule restated precisely.
- **Commit and merge as I go**, full `make ci` after each merge.

## Bugs found — all pre-existing, none by the item that found them

- **The GUI had no luma policy at all.** `images.rs` stores `cover_accent`
  *unclamped* and says why (a clamp is a renderer's policy about its own
  lighting); the TUI has one, the GUI was painting the raw measurement. Only
  visible because `fake.ts` invents hostile full-gamut accents — against a
  library of real jackets it would have looked fine indefinitely.
- **Two cache keys were blind to a back-fill.** `rb covers` fills `cover_accent`
  without moving the path, id or title, so `Scene::cover_key` and
  `present::cover_hash` would both serve a back-filled book its old spine
  forever — and a *transmitted* kitty frame that is never re-sent is a stale
  image with nothing on screen looking wrong. Found by item 39 while deleting a
  duplicate.
- **The note editor's `Saved` measured 2.31:1.** It inherited
  `button:disabled { opacity: 0.55 }`, so the only statement on the page that the
  note reached the vault was the least legible text on it.
- **Item 23 found a hole in its own spec.** The spec rules that an import mints
  no moments; it says nothing about an *upgrade*. A library that has been here
  months has honest `created_at`s, so the first launch after migration `0017`
  would fire a moment for every reading ever closed. `moment_epoch`, seeded by
  the migration, is one instant before which everything is history.
- **A stale doc claim survived three handoffs on a false justification.** The
  accent duplication was kept because "the two measure different images". They
  do not: `texture.rs` measured the full-resolution decode and resized on the
  *next line*, on the same file `images.rs` measured. Nobody had re-read the code.

## Technical gotchas

- **`ts-rs` emits a new field as required in TypeScript however
  `#[serde(default)]` the Rust is.** Item 40 added `reading_id` to `ListNotes`
  and broke `gui/src/lib/api/client.ts`. Its own gate passed: a fresh worktree
  has no `gui/node_modules`, so `web-check` prints `SKIPPED:`. **An engine item
  that changes a request shape can only fail on the frontend, and a worker
  cannot see it.** Prefer adding a request over changing one.
- **Four of six agent worktrees were created ~80 commits behind `main`**,
  migrations stopping at `0010`. Every thread caught it only because its prompt
  said to check; one would otherwise have written a migration into a
  five-version gap. Put `git log --oneline -1` + `ls crates/engine/migrations/`
  in every worktree prompt.
- **`{@const}` must be the immediate child of a block** in Svelte 5 — not of an
  element inside one.
- **`noUncheckedIndexedAccess` is on**, so `LAYOUTS[0]` is `| undefined`. Typing
  the registry as a non-empty tuple `[T, ...T[]]` encodes the real invariant
  instead of adding a guard for an impossible case.
- **A `color-mix` cannot step *away* from a colour**, only toward one named at
  author time — so a fixed pole makes an inset panel crisp on dark jackets and
  invisible on pale ones, or exactly the reverse. The step has to be computed
  where the colour is.
- **A luma band's floor is not always reachable.** Scaling toward the band
  preserves hue where per-channel clipping would not, but saturated blue is
  already at maximum on its one bright channel. Asserted honestly: ceiling
  unconditional, floor only as "never gets darker", pure blue pinned as the limit.
- **Byte tolerances.** The output is three integers, so a luma band holds to
  within ~1/255 and no finer; asserting to 1e-6 is asserting about arithmetic
  that never reaches a screen.
- **Playwright captured a hover state** in two committed shots, which would read
  as a regression in a future diff. Screenshot fixtures are not automatically at
  rest.

## What the screenshot reviews caught that no assertion did

Three rounds, and the pattern held every time: the reviews found **axiom**
defects, not taste defects.

- `Rows` **hid the state label** below 420px. The state is *what you did* — the
  one thing that surface exists to say — so a phone saw a shelf that said what
  you own and nothing about your reading.
- An unmeasured cover got a **flat pale chip** in one arrangement and a hatch in
  the other, collapsing "never measured" into "this jacket is grey".
- The accent measured **2.78:1** on the light theme while carrying every state
  label, and the segmented switch's *selected* segment was harder to read
  (2.95:1) than its unselected sibling (5.61:1).
- **The card's band heading was "What you left"** — the axiom's own sentence,
  word for word, over a band printing a count. It passed every assertion,
  because the assertions ban the words *somebody else* would use.

The last one is the argument for the whole practice: no type checker, unit test
or route assertion reaches it.

## One review finding declined

The lowercase `paused` (`ReadingStateDto`'s `other` arm) was reported as an enum
value leaking to the user. Declined: `raw` is another application's word, kept
verbatim by a decision item 17 made and `phrasing.test.ts` pins, and title-casing
only flatters the single-word case — the fixture's real shape is
`paused-by-some-other-app`. Recorded in `phrasing.ts` so it is not re-reported.

## Threads that pushed back, and were right

Every one of them. It is now five waves running.

- **Item 40** refused two adjacent `Option<i64>` on `list_notes` (book, reading —
  a transposition compiles, and both id spaces start at 1) and used a
  `NoteScope` enum. It also **corrected a property this orchestrator asserted in
  the prompt**: "scoping preserves relative order" is false across sources,
  because the merge keys on within-source *position* and filtering compacts it.
- **Item 44** chose *longest passage, ties by lowest id* and stated the cost
  rather than hiding it: it selects for the longest **drag**, not the best
  passage. It refused a length cap as a magic number.
- **Item 28** refused to speak a run of days as a number on the home surface
  ("a run of days describes a habit, and that is a streak one decision later"),
  refused to fake a read ordinal without item 41, and drew a card for the *open*
  read too — because gating it would say the read you are in has no card yet,
  which is completion framing.
- **Item 23** refused `RUN_MIN_DAYS > 2`: anything larger is the module deciding
  what counts as enough reading, which is the threshold the item forbids.

## Verification

`make ci` exit 0 on `main` after every merge, and at the end: 29 Rust suites,
**172** vitest tests, **81** Playwright routes on WebKit, `ts-check` matching,
no `SKIPPED:` lines. Migrations run to `0017`; `API_VERSION` stayed at **2** —
everything added to the wire this wave was additive.

Item 44's thread ran **six neuter checks** (break the code, confirm the test goes
red, revert) and deleted the `proptest-regressions/` files those runs generated,
on the grounds that they record inputs which broke deliberately-broken code.

## Deferred, with numbers

**41** the read number crosses · **43** readings across the library (the wall's
blocker) · **45** a flashcard can be made · **46** which passages are already
cited · card image export (needs canvas rasterisation; not a wiring job).

Next free number is **47**. New failure mode for the register: an audit
**allocated 40–46 and only four were built**, so a number in the handoff is no
longer evidence that work exists. Only a `decisions.md` entry is.
