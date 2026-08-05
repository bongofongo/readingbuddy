---
title: Item 19 — the shape of an edition, and the ratio that had to stay a ratio
date: 2026-08-05
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Session log

Item 19, built in the worktree `rb-wt/19-edition` on `feat/engine-edition-shape`,
branched from `main` at `a8ff043`. Three siblings (20, 22, 24) were building in
parallel; integration is the orchestrator's. Not pushed, not merged, no PR.

**Item 20 had not merged when this landed, and the rewire has not happened.** The
cover aspect is still a parameter fed by an image decode at the one call site in
`crates/tui/src/render3d/mod.rs`. See *The rewire, for whoever picks it up* at the
bottom — it is one line and the signature does not change.

## What it is

`Model::new` was nine lines, four of them arithmetic, and those four answered a
question no ray tracer asks: *what shape is this edition*. They now live in
`crates/engine/src/edition.rs` as `EditionShape`, and `Model::new` scales the
answer into scene units.

This is the second instance of item 17's rule, and it names the rule's other
half. *A `Progress` enum is not terminal I/O; `"p.42"` is* has a twin:
**proportions are not rendering; a Bézier spine highlight is.**

## Decisions locked

- **`EditionShape`, not `Extent`.** Half-extents are a graphics word; importing
  the renderer's vocabulary into the engine is the coupling the item exists to
  remove. *Edition* rather than book or work, because page count and cover art
  belong to a printing — two printings of the same novel are different objects
  on a shelf and this type is the only thing that tells them apart.
- **Everything is a multiple of the book's own height, and height is not a
  field.** This was the subtle part and the prompt was right to flag it hardest.
  `scene::HALF_HEIGHT` is one ratatui camera rig's idea of how big a book is; had
  the engine handed back a number that only meant something inside that rig, the
  arithmetic would have moved and the *decision* would not have — a WebGL shelf
  would have inherited a terminal's scene constant, which is worse than the
  duplication it replaced. Each frontend picks its own unit and multiplies.
- **Millimetres were considered and rejected.** The prompt allows "a ratio or an
  absolute in a stated unit", and millimetres were tempting because they are what
  a book actually has. But we do not know an edition's real dimensions; deriving
  "152mm" from a cover *image's* aspect ratio is inventing a measurement, and a
  number wearing a physical unit it did not come by is a worse lie than an honest
  ratio.
- **The constants were rewritten into height-relative form and the result is
  numerically identical.** Old: `depth = (0.045 + pages / 9000).clamp(0.05,
  0.20)`, an absolute in a scene where half-height is `0.75`. New:
  `thickness_over_height = (0.06 + pages / 6750).clamp(1/15, 4/15)`, and
  `0.75 × that` is the old expression exactly. Both bounds turned out to be exact
  fifteenths, which is a small piece of luck.
- **`PAGES_PER_HEIGHT = 6750` is a name for a number that had none.** It is how
  many pages stack to the book's own height. At trade-paperback scale (229mm)
  that is ~0.034mm a page, ~0.07mm a leaf, against ~0.09mm for real book stock —
  so the slope is roughly real paper and the `BOARDS = 0.06` base is not (real
  boards are far thinner). Between them the model lands slightly fatter than
  life, which is what makes a spine legible at forty terminal cells. Writing that
  down is most of the value of moving the constants at all: next to a ray tracer
  they were only explicable as tuning.

## Where I pushed back

Both of the prompt's two invitations, and one it did not raise.

**Do the clamps belong in the engine?** Ruled yes, with a line drawn rather than
a blanket answer. The **object's proportions** are the engine's, because they are
what makes an edition that edition and because two frontends clamping differently
is a shelf that contradicts a book view. **Everything about how it looks** —
colour, lighting, bevels, shadow, spine typography, whether it is drawn at all —
is the frontend's. That line also disposes of the "aesthetics are a frontend's"
argument on the width clamp specifically: `0.55..0.85` is not taste. A cover
image is cropped, scanned and jacketed at whatever aspect a provider felt like, so
a 1:1 thumbnail is not evidence of a square book. The clamp corrects an unreliable
proxy back onto a plausible physical object, which is a data judgement.

**Is `page_count.unwrap_or(320)` honest?** Ruled yes *here*, for the reason the
prompt anticipated and no other: a renderer has no `None` to draw. A solid has to
be some thickness; a progress bar may legitimately draw nothing. That difference
licences filling the absence — it does not licence hiding it. So
`ShapeSource::{Recorded, Assumed}` marks each of the two numbers, exactly as
`FractionSource` marks where a fraction came from, and the invented thickness is a
middling paperback strictly inside the range rather than at an edge, so a book of
unknown length cannot masquerade as a remarkable one. `EditionShape::new(None,
None) != EditionShape::new(Some(320), None)` — same numbers, different
provenance — and that inequality is asserted, because it is the whole distinction.

**The pushback nobody asked for: `Some(0)` was drawing as a pamphlet.** Item 17
established that a zero page count is a false number rather than a small one, and
`progress::denominator` filters it. The renderer's `unwrap_or(320).clamp(48,
1400)` sends `Some(0)` — and every negative — to 48, the *thinnest book the model
allows*, and `make dev-db` has real zero rows. Unknown length was rendering as
"very short", which is the same class of lie as `[12/0]` and survived for the same
reason: it does not look like a bug, it looks like a pamphlet. Fixed;
`usable_pages` is `denominator`'s twin, deliberately written to look like it.

## Properties asserted

`mod props`, following `progress.rs`:

- `a_shape_is_always_drawable` — over `Option<i64>::ANY` pages and
  `Option<f32>::ANY` aspect, *including `NaN` and both infinities*: both numbers
  finite and inside their published ranges. This one earned its keep. `f32::clamp`
  passes `NaN` straight through (neither comparison fires), so a `NaN` aspect from
  a bad decode would have reached a vertex buffer; `usable_aspect` filters on
  `is_finite() && > 0.0` because of it.
- `more_pages_is_never_thinner` — monotonicity, which is the only thing that makes
  a shelf of spine widths mean anything.
- `a_wider_cover_is_never_a_narrower_book` — the same rule on the other axis, so a
  later clamp cannot be written as a fold or a wrap.
- `recorded_means_something_was_recorded` — the converse guard, the shape of
  `untouched_only_when_there_is_genuinely_nothing`: a number may only claim
  `Recorded` when something usable actually was. That is what stops a future arm
  quietly laundering a guess.

Plus, in the renderer, `the_engines_shape_reproduces_the_renderers_old_arithmetic`:
it reruns the four deleted lines verbatim and compares half-extents across six
page counts. The renderer is frozen, so the move must not be a redesign — and it
is also the *only* place the scene-unit ↔ height-ratio factor is checked. Get that
factor wrong and every spine changes thickness by a third with nothing failing.
The one deliberate difference is isolated in its own test,
`a_zero_page_count_no_longer_draws_the_thinnest_possible_book`, so it cannot be
mistaken for drift.

## Corrections this build forced

1. **`page_count = 0` and negatives are absence.** Above. The only user-visible
   change in the item.
2. **`NaN` could reach the renderer.** Not hypothetical in kind — `Cover::aspect`
   is a decode away from arbitrary — and `clamp` does not stop it.
3. **The engine cannot serve this to the GUI, and that is an API gap.** `crates/api`
   was out of scope (item 18 is editing DTOs in parallel; a collision there helps
   nobody), so `EditionShape` is engine-side only. The GUI links `api` and
   deliberately not the engine, and the webview sees JSON — so a WebGL shelf
   either gets a DTO field or re-derives the arithmetic in TypeScript, which is
   the exact failure this item was written to prevent. **The shelf item must not
   start before that DTO exists.** Recorded in `decisions.md` entry 19.
4. **The double clamp is not redundant and is now documented as deliberate.** The
   page clamp is the *decision* (past 1,400 pages thickness stops being
   informative); the output clamp is what makes "always in range" a property of
   the type rather than of today's curve.

## What I deliberately did not build

- **No migration, no column.** `0014` is item 20's. The cover aspect is an
  `Option<f32>` parameter.
- **No DTO, no API change.** Item 18 owns that file this week. See correction 3.
- **Nothing in `render3d/` beyond the call site.** The renderer is frozen and
  `decisions.md` is right to freeze it.
- **No `EditionShape` for the CLI.** It has nothing to draw a solid with, and a
  derived fact with no caller is speculation.

## The rewire, for whoever picks it up

When item 20 merges, one line in `crates/tui/src/render3d/mod.rs`:

```rust
let shape = EditionShape::of_book(book, Some(cover.aspect));
```

becomes a read of the stored `width`/`height` columns instead of a decoded
image's aspect. `EditionShape::of_book`'s signature does not change; the doc
comment on the `cover_aspect` parameter says so in as many words, which was the
point of writing it that way. Note that the *renderer* is a legitimate holdout —
it draws one book and has the decoded image in hand anyway — so the rewire is
about the shelf, not about this call site being wrong.

## Verified

From the worktree, all with `CARGO_INCREMENTAL=0`:

- `make fmt` — clean.
- `make lint` (`clippy --workspace --all-targets -D warnings`) — exit 0.
- `make build-check` (`cargo check --workspace`, the build where `internals` is
  off) — exit 0.
- `make test` (whole workspace) — exit 0, no failures anywhere.
- `cargo-tester` agent — run before calling it done.

**Not** claimed: `make web-check` and `make routes` degrade to `SKIPPED:` in a
worktree (no `gui/node_modules`), and neither was exercised. Nothing under `gui/`
changed. Full `make ci` runs on `main` after merge.
