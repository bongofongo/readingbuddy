---
title: Item 19 — the shape of an edition, in the engine
date: 2026-08-05
source: docs/gui/spec-gui-17-28.md item 19; docs/decisions.md entry 17 for the
        rule this item is the second instance of
follows: sessions/2026-08-05-the-derived-facts-layer.md
---

# Prompt — Item 19: the shape of an edition, in the engine

Paste into a fresh session at the repo root, on branch `feat/engine-edition-shape`,
branched from `main`. **Can land before item 20** with an aspect parameter and be
rewired afterwards — see *Launch order* in `docs/next-thread-handoff.md` and the
dependency note below, which is the one real decision in this item's sequencing.

Read `CLAUDE.md` (**Engine standards** is binding — the *"derived facts live
here, phrasing does not"* bullet is literally about this item), then item 19 in
`docs/gui/spec-gui-17-28.md`, then `crates/tui/src/render3d/CLAUDE.md` for the
renderer you are **not** changing.

**Engine only. No migration** — `0014` is item 20's and `0015` is item 23's.

## What the item is

`Model::new` (`crates/tui/src/render3d/mod.rs:121`) is nine lines, four of them
arithmetic:

```rust
let h = scene::HALF_HEIGHT;
let width = h * cover.aspect.clamp(0.55, 0.85);
let pages = book.page_count.unwrap_or(320).clamp(48, 1400) as f32;
let depth = (0.045 + pages / 9000.0).clamp(0.05, 0.20);
```

That is not rendering. That is the answer to *what shape is this edition*, and
it belongs in the engine so that a **WebGL shelf** and a **Unicode-glyph book**
agree about how fat *Infinite Jest* is. `make dev-db` has a 1,408-page doorstop
beside a 48-page pamphlet for exactly this comparison.

**Move the derivation. Leave the renderer alone** — `docs/decisions.md` freezes
it, and freezing it is right.

## This is the second instance of a rule item 17 just established

Item 17 landed `readingbuddy::names` and `readingbuddy::progress` on the same
argument, and `CLAUDE.md`'s Engine standards now states it: *a `Progress` enum is
not terminal I/O; `"p.42"` is.* Your version: **half-extents are not rendering; a
Bézier spine highlight is.**

So copy the shape of `progress.rs` rather than inventing one:

- A **value type** with a name (`EditionShape`? `Extent`? pick one and defend
  it), constructed from a `Book` plus whatever the cover supplies.
- The magic numbers travel **with** the derivation and get a comment saying what
  they mean — `0.55..0.85` is "shapes books actually come in", `320` is a
  paperback, `48..1400` is the page range past which thickness stops being
  informative. Right now those constants are only explicable next to a ray
  tracer, which is the wrong place for a fact about paperbacks.
- **Properties where an invariant exists.** This one has obvious ones and they
  are the whole reason to move it: the output is always in range whatever the
  input; a thicker book is never thinner than a thinner one (monotonicity); a
  `None` page count lands inside the range rather than at an edge. A property
  here is worth more than five examples, and `progress.rs`'s `mod props` is the
  pattern.

## The dependency, and how to not be blocked by it

`Model::new` takes a `&Cover`, and **`Cover::aspect` is computed by decoding the
image** (`crates/tui/src/render3d/texture.rs`, format-agnostic). The engine
stores no cover dimensions — no migration defines a `width`, `height` or `accent`
column — so a shelf of three hundred spines would decode three hundred images to
find out how wide to draw them.

That column is **item 20b**, and it is running in parallel with you.

**Take the aspect as a parameter** (`Option<f32>`, with the same default the
renderer uses today when there is no cover) and land without waiting. When item
20 merges, the call site changes from a decode to a column read and your
signature does not change at all. Say in the doc comment that this is what the
parameter is for, so the rewire is one obvious edit rather than an archaeology
problem.

Do **not** add the column yourself. It is a migration and it is allocated.

## What must not happen

- **Do not touch `render3d/`'s rendering.** Changing `Model::new` to call the
  engine is in scope. Changing what the ray tracer draws is not, and
  `every_screen_draws_at_every_size` is the test between a layout change and a
  panic in the user's tmux pane.
- **No migration**, and no column. See above.
- **No prose.** Half-extents are numbers; "a fat book" is a word.
- **Do not fold in `HALF_HEIGHT`.** A scene constant is the renderer's — the
  engine's answer should be a *ratio* or an absolute in some stated unit, and the
  renderer scales it. If the engine hands back a number that only means something
  inside one scene, you have moved the arithmetic without moving the decision.
  This is the subtle part of the item; get it wrong and the WebGL shelf inherits
  a ratatui constant.

## Files you own

`crates/engine/src/` (a new module beside `names.rs` and `progress.rs`),
`crates/engine/src/lib.rs` for the export, `crates/tui/src/render3d/mod.rs` for
the call site. **No collisions** with items 18, 20, 22 or 24 — the `lib.rs`
export list is the only file several of you touch and the conflicts there are
textual and trivial.

Worth knowing while you are in the renderer: it is fully headless in tests
(ratatui's `TestBackend`), and `--dump-frame [--dump-png]` plus the `--ignored`
`print_layout` aid are how you show what a change looks like without a terminal.
Only `make bench`, `make bench-box` and `--probe` need a real, active pane.

## Push back rather than comply

Four of five threads in the last wave did, and each time they were right. Two
lines worth arguing with here:

- **Whether the clamps belong in the engine at all.** They are *aesthetic*
  decisions — a book 30% as wide as it is tall looks wrong, so it is clamped —
  and an argument that aesthetics are a frontend's is not stupid. The
  counter-argument is that two frontends clamping differently is a shelf that
  disagrees with a book view about the same edition. Decide, do not default.
- **Whether `page_count.unwrap_or(320)` is honest.** Item 17 spent real effort
  establishing that **absence is not zero** and that a `NULL` page count must not
  become a drawn empty track. A silent default of 320 is the same class of thing
  wearing better clothes: it invents a shape for a book whose length nobody
  recorded. It may still be right — a book has to be drawn *some* thickness —
  but if it is, that is because a renderer must commit where a progress bar need
  not, and that difference deserves a sentence.

## Done means

- `make ci` exit 0.
- The `cargo-tester` agent before you call it done.
- **The corrections this build forced, written into `docs/decisions.md`.**
- A session log, via the `wrap-session` skill. Say in it whether item 20 had
  merged and whether the rewire happened, so the next thread does not go looking
  for a parameter that is already gone.
