# Prompt — Item 37: one fixture, two consumers

**Run from the main checkout**, after items 33, 34, 36 and 35 have merged. Not
from a worktree, and this is the whole reason it is last: it touches `gui/`, so
its gate is `make web-check` — and a fresh worktree has **no `gui/node_modules`**
(gitignored), where `web-check` and `routes` degrade to a stated `SKIPPED:`. A
worker would therefore "pass" them without running them.

---

Read `crates/corpus/CLAUDE.md`, `gui/CLAUDE.md` and `docs/gui/testing.md` (the
layer model is the subject of this item).

**No migration.**

## What

Assert that `crates/corpus`' `gen-devdb` and `gui/src/lib/api/fake.ts` agree
about the shapes they both claim to model, and give the cover layout a headless
regression test.

## Why it bites now

The fake serves **no covers, ever** — a deliberate choice, and documented as one
at `gui/src/lib/api/fake.ts:353`:

> No cover in the fake, ever, and that is the honest answer rather than a gap:
> layer 2 runs in a bare browser with no asset protocol, so any URL here would
> render as a broken image. Every tile therefore exercises the **no cover** path,
> which is the one that has to be a designed empty state — and the covers
> themselves are checked in the real app, against `make dev-db`.

That reasoning was correct and is now half-obsolete. As of 2026-08-06 the real
app finally *can* show them — whole `cover_path`s, 202 measured covers — which
makes the untested half the one item 26 is about to build on.

## Why it is an item and not a chore

A fixture can disagree with the engine about a column's shape and **nothing
notices**. Found 2026-08-06: `gen-devdb` wrote a *relative* `cover_path` where
the schema holds `images_dir.join(name)`. It was only observable by running a
command that read the file the path names, and it would have made item 26's
shelf show zero covers. The habit this item encodes: **a path in a fixture is
not checked until something opens it.**

## Done when

- A test **fails** when an edge case exists in one fixture and not the other.
  That is the real bar and it is easy to miss: a test that merely enumerates both
  and passes is a description, not a guard. Write it, then delete an edge case
  from one side and watch it go red.
- The shape of what a cover-bearing tile reserves — `cover_aspect`,
  `cover_accent`, `cover_shelf_path` — is exercised headlessly.

**The trap in that last line.** dev-db's covers are 240×360, which is **below the
shelf tier**, so `cover_thumb_path` is NULL for all 202 and `cover_shelf_path`
correctly falls back to the original. A test that asserts a thumb exists would be
asserting the fixture, not the rule. Assert the *fallback*.

## Must not

- Let `crates/corpus` depend on `readingbuddy`.
- Give the fake a cover URL that renders as a broken image in layer 2 — the
  comment above is still right about that. Exercising the cover-bearing *shape*
  is not the same as serving a jacket.
- Hand-edit `gui/src/lib/api/bindings.ts`.

## Files

`crates/corpus/src/devdb.rs`, `gui/src/lib/api/fake.ts`, a new test on whichever
side can see both, `gui/tests/`.

## How it is gated

The full **`make ci`** — this is the one item in the wave that can run it
honestly, because the main checkout has `gui/node_modules`. Plus `make dev-db`
and a look at `make shots`.

## `docs/decisions.md`

**Append**. The file is in build order, not numeric order.
