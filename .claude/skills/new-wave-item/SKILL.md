---
name: new-wave-item
description: Start a numbered build item from docs/gui/spec-gui-17-28.md (or a later wave) to this repo's ritual — pre-allocate the migration, write the prompt file, then build. Use when beginning any numbered engine or GUI item. The bookend to wrap-session.
---

# new-wave-item

`wrap-session` closes a session. This opens one. The ritual is written down in
`docs/decisions.md` but nothing enforces it, which is how a step gets skipped.

## 1. Read what already decided this

In order, and do not skip to the code:

1. **`docs/decisions.md`** — what is settled, no reasoning. If the item
   contradicts a line here, that is a conversation with the user, not a thing to
   quietly do.
2. **The spec entry** — `docs/gui/spec-gui-17-28.md` for items 17–28,
   `docs/spec-11-16.md` for 11–16, `docs/spec-engine-*.md` earlier.
3. **The crate's own `CLAUDE.md`** — root routes you there. Do not read them all.
4. **`docs/gui/gui-vision.md`** if the item touches the GUI or rewards.

## 2a. The item number is not allocated by `docs/prompts/`

**`grep '^[0-9]\+\. \*\*' docs/decisions.md` is the register of spent item
numbers.** The prompts directory is not, and reading it instead is how the
2026-08-06 non-GUI wave allocated itself **33**–37 and shipped two items
numbered 33 before anyone noticed.

The one it collided with — item 33, "Surfacing 21/29/30/31/32" — was minted
*mid-session* rather than from a spec, so it has a `decisions.md` entry and a
session log and **no prompt file**. Any item that starts as a handoff's open
work rather than as a spec row has that shape, and there will be more of them.

The wave shifted to 34–38 once it was found, at the cost of one commit touching
32 files: worker threads write their item number into module headers, migration
headers, test section comments and `CLAUDE.md` routing rows, so a number is not
a label on a document — it is a fact scattered through the source. Check the
register first; it costs one grep.

## 2. Pre-allocate the migration number — before writing anything

If the item takes a migration, claim the number **now** and tell the user which.

Parallel branches are how two threads both claim `0008`, and a duplicate version
is not a git conflict — the filenames differ past the number.
`migration_versions_are_contiguous_from_one` catches it, but only after both are
merged, which is the expensive moment to find out.

Currently pre-allocated, in merge order: `0011`–`0018` are **applied** (items
21, 29, 32, 20, 34, 35, 23, 43 — `0017` moved down from `0015` on 2026-08-06 so
the non-GUI wave could land ahead of it). **Nothing is outstanding**: the next
migration is `0019` and no item in any wave has claimed it.

The contiguity test fails on a **gap** as well as on a duplicate, so a branch
holding `0012` before `0011` has merged is red until its predecessor lands.
That is expected — rebase, never renumber.

**Never edit an applied migration.** CI's `migrations` job refuses a modified,
deleted or renamed one outright.

## 3. Write the prompt file

`docs/prompts/<NN><a|b>-<slug>.md`, matching the thirteen already there. One
file per thread. It should carry:

- what the item is and what it must **not** do — the boundary an eager thread
  crosses helpfully;
- which files it touches, so parallel threads can be checked for collisions;
- the migration number if it has one;
- **an explicit request for the corrections it forced.** Every item in
  `decisions.md` records what building it changed about the plan. Ask and it
  arrives; do not ask and the next thread rediscovers it.
- **"Push back rather than comply."** Four of five threads did last wave and
  each time they were right.

## 4. Check the item is actually parallel-safe

Items 17, 18, 20, 21, 22 and 24 share no files and can run concurrently.
Items 26, 27 and 28 share components and design decisions and **must not** —
three agents in parallel on those produce three dialects of one app.

If you are starting a GUI item, say which of 26–28 are already in flight.

## 5. Build it

One PR per thread. Green CI. Nothing auto-merges.

- Engine work → the `cargo-tester` agent before you call it done.
- Frontend work → the `web-checker` agent, then `screenshot-reviewer` for
  anything that draws.
- A new GUI feature → the `api-surface-auditor` agent **first**, so a missing
  request becomes an engine item rather than a workaround.

## 6. Record what it changed

When the item lands, add its entry to the spec's build-order section in the
shape the existing ones use: **Done**, plus the corrections the build forced and
the things the spec was silent about. That paragraph is the most valuable thing
the item produces, and it is the one that is skipped when the tests go green and
the session ends.

Then `wrap-session`.
