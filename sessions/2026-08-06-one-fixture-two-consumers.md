---
title: Item 38 — one fixture, two consumers
date: 2026-08-06
follows: sessions/2026-08-06-the-chooser-knows-who-wrote-it.md
---

# Session log

The last item of the 2026-08-06 non-GUI wave, and the one the orchestrator ran
itself rather than handing to a worker. The reason is mechanical: it touches
`gui/`, so its gate is `make web-check` and `make routes`, and a fresh worktree
has no `gui/node_modules` — both degrade to a stated `SKIPPED:` there, so a
worker would have "passed" them without running them.

## What it is

`crates/corpus/edge-cases.json` declares the hostile set once. Two tests read it:
`devdb::tests::the_edge_cases_are_the_declared_ones` on the Rust side and
`gui/src/lib/api/fake.test.ts` on the frontend's. Neither fixture is the
declaration and nothing generates it.

Plus the four cover fields `fake.ts` had never carried, and the tests that pin
what they mean.

## Decisions locked

- **Hand-edited declaration, not a generated one.** The tempting design is to
  emit the JSON from `gen-devdb` and have the frontend check itself against the
  emission. That makes the Rust side vacuous — it would be checking its own
  output — and `crates/corpus`' whole value is that it is an *independent
  oracle*. The same argument, one level up.
- **Length compared first, on both sides.** A per-case loop catches a case added
  to the declaration and misses one deleted from it. Two fixtures drift apart by
  deletion at least as often.
- **Believe the guard only after watching it fail.** Ran it against a case
  present in the declaration and absent from the generator, and against a shape
  disagreement on a case both have. Both went red with the message naming the
  case. This is the item's own stated bar and it is easy to skip once the suite
  is green for the good reason.
- **The intersection, not the union, of what the two fixtures can answer.**
  Highlights, notes and tags live in side tables in one and in separate maps in
  the other; a claim only one side can check is decoration. Six fields plus the
  cover flag is what both can honestly answer.
- **The blank-title bug is recorded, not fixed.** `Book::display_title` is
  `unwrap_or("(untitled)")`, and `books.title` is `TEXT NOT NULL DEFAULT ''`, so
  the branch it guards is unreachable for any stored book and the reachable case
  falls through as a blank. 76 call sites, and three sibling copies where `None`
  genuinely is reachable. Patching that inside a fixture-parity item would be the
  scope creep every prompt in this wave warned about.

## What it found

Four disagreements on the first run, none of which any existing test could see:

| | `gen-devdb` | `fake.ts` |
|---|---|---|
| subjects | one per edge case | `[]` |
| The Claw Of The Conciliator | reading | unread |
| `reading_state: other` | absent | present |
| untitled case | `''` | `null` |

And three structural findings behind them:

1. **`as StoredBook` was defeating the file's own stated purpose.** A cast makes
   a *renamed* field an error and an *added* field silently absent.
   `cover_shelf_path`, `cover_aspect` and `cover_accent` had been missing since
   item 20 landed, so layers 1 and 2 rendered the no-cover branch exclusively —
   and the cover-bearing branch, which item 26 is about to build on, was
   exercised nowhere at all.
2. **Removing the cast surfaced a second hazard immediately.** `BookDto.id` is
   `number | null`; `StoredBook.id` is `number`. `...over` could therefore put a
   null in the field every route keys on. `id` now goes after the spread.
3. **`TauriClient::coverSrc` read `cover_path`**, against a rule `gui/CLAUDE.md`
   has stated since item 20c. Invisible to every screenshot: against `dev-data`
   the two paths name the same file, because 240×360 is under `THUMB_MAX`.

## Gotchas

- **`noUncheckedIndexedAccess` is on.** `books[i]` is `T | undefined`, so the
  first draft of the test did not typecheck. Rewritten around a `find` plus an
  explicit throw naming the missing id, which is a better failure message anyway.
- **`node:fs` has no types here.** The first draft read the declaration through
  `readFileSync` and svelte-check rejected all three `node:` imports. Importing
  the JSON works instead — `resolveJsonModule` is already on — with one
  `server.fs.allow` entry scoped to `crates/corpus` rather than the repo, so the
  one file reached out of `gui/` keeps saying it is one file.
- **The visual gate moved and had to be looked at, not just regenerated.** 21 of
  the committed PNGs changed. `make shots` then reading them is the step
  `gui/CLAUDE.md` asks for and the step that is easy to skip: the library grid is
  what shows `The Claw Of The Conciliator` finally rendering `Reading · 33%`, and
  `A Book Some Other App Touched` rendering its unknown status as `paused`.

## Left for later

- **`Book::display_title` and the blank stored title.** Above. An engine item.
- **A generator that emits both fixtures**, which `fake.ts`'s header has called
  open work since the scaffold. This item makes the drift *loud* rather than
  removing the second fixture, and that is deliberate: layer 2 must run in a bare
  browser with no IPC, so the frontend's fixture cannot be the database one. What
  could be generated is `fake.ts` itself, from the declaration — worth doing when
  the declaration has earned more fields than it has now.
- **The tile that reserves the box.** The cover *shape* now crosses into layers 1
  and 2 and is asserted there, but no component draws a cover yet — the current
  `+page.svelte` is a plain grid, deliberately not a half-built item 26. Wiring
  `cover_aspect` into a reserved box is item 26's, and it now has a fixture that
  can exercise it.
