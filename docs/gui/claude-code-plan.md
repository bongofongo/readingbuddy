---
title: Running Claude Code against the GUI wave
date: 2026-08-04
source: measured against this repo's `.claude/`, `CLAUDE.md` and `Makefile` on
        2026-08-04; see `docs/gui/spec-gui-17-28.md` for the work itself
---

# Running Claude Code against the GUI wave

Ordered by leverage. The first four are worth doing before item 17 starts; the
rest can land alongside it.

The honest framing up front: **most of what makes agents fast on this repo is
already here.** The `docs/prompts/` one-file-per-thread pattern, the
`cargo-tester` agent that reports only failures, the `wrap-session` skill, the
container-warming hook, `make ci` reproducing the gate exactly, and the "tell
each thread to push back rather than comply" instruction that four of five
threads used — that is a better setup than most repos have. What follows is
mostly about **the seam that does not exist yet** (Rust → TypeScript → pixels),
because that is where a GUI wave goes wrong, and about **proportion**, because
the context that made the engine wave fast will actively slow the GUI wave down.

---

## 1. Split `CLAUDE.md` by directory

Measured today:

| section | bytes | share |
|---|---|---|
| **`## Architecture`** | **87,452** | **80%** |
| `## Commands` | 7,872 | 7% |
| `## Engine standards` | 4,757 | 4% |
| `## Releasing` | 4,640 | 4% |
| everything else | 4,848 | 4% |
| **total** | **109,569** (~27,400 tokens) | |

Be precise about what the problem is, because the obvious framing is wrong.
With prompt caching this is **not primarily a cost problem**. It is two other
things:

- **Context budget.** 27k tokens is spent before the session reads a single
  file, and it is spent again in every subagent you spawn. Six parallel threads
  is 164k tokens of the same prose.
- **Signal-to-noise.** `## Architecture` is one bullet per crate and the
  `crates/cli` bullet alone is a multi-thousand-character paragraph. It is
  excellent writing and it is *why the engine wave went well*. It is also, for
  a thread writing a Svelte component, a wall of detail about the kitty graphics
  protocol, `MAX_LINE`, clap subcommand refusal wording and the octant
  rasterizer. Attention spent there is attention not spent on the task.

**The split.** Claude Code reads `CLAUDE.md` from the working directory upward,
so a `CLAUDE.md` inside a crate is loaded when a thread is working in that
crate and not otherwise. (Worth confirming the exact behaviour on your version
before committing to the reorganisation — it is the one assumption here that is
about the tool rather than about your repo.)

```
CLAUDE.md                  → project, the axiom, conventions, engine standards,
                             a ONE-LINE-PER-CRATE map, and the ~10 commands
                             anyone actually runs.  Target: under 6k tokens.
crates/engine/CLAUDE.md    → the engine bullet, storage rules, migration ritual
crates/tui/CLAUDE.md       → the TUI bullet, render3d, caps, bench/perf, kitty
crates/cli/CLAUDE.md       → the CLI bullet (currently the single largest)
crates/api/CLAUDE.md       → the API + daemon bullets, DTO rules
gui/CLAUDE.md              → new; component conventions, the API-only rule
docs/releasing.md          → `## Releasing` and the bench harness leave CLAUDE.md
                             entirely; they are reference, not standing context
```

Keep in the root file the things that decide arguments rather than describe
code: the axiom, "the engine does no terminal I/O", "never edit an applied
migration", "push back rather than comply", and the pointer to
`docs/decisions.md`. Those are worth 27k tokens; the kitty protocol is not,
when the task is a shelf.

**This is the highest-leverage single change and it is an afternoon.**

---

## 2. Generate the TypeScript types from the Rust DTOs

There is no `ts-rs`, `specta`, `typeshare` or `tauri` in any `Cargo.toml`
today, so this seam does not exist yet — which means it can be done right the
first time.

The numbers: **70 DTO types** in `crates/api/src/dto.rs` and **86 request
variants** in `protocol.rs`. Hand-written TypeScript mirrors of those will
drift, and the drift is invisible until runtime — in a webview, where a wrong
field name is a blank panel and a console error nobody is reading.

This matters *more* for agent-written code than human-written code, for one
reason: **an agent's error rate on remembered field names is much higher than a
human's, and its ability to notice a silent failure is much lower.** Generation
converts that entire class of bug into `tsc` output, which the agent reads,
understands and fixes in the same turn.

Use `ts-rs` (derive on the DTOs, emit `.ts` in a build step) or `tauri-specta`
(also generates typed command bindings, which is more machinery but removes the
hand-written Tauri command layer too). Either way:

- Generation runs in `make` and the output is **committed**, so a thread that
  cannot run the generator still sees current types.
- A CI job regenerates and diffs, so a DTO change that skips the generator
  fails the gate rather than shipping.

---

## 3. Make the GUI headlessly visible — copy your own `--dump-frame`

This is the one that is specifically about *GUI* work rather than agent work in
general, and you have already solved it once.

The TUI has `--dump-frame 100x30 [--dump-png out.png]`, `--probe`, the
`print_layout` and `print_ambient` aids, and `every_screen_draws_at_every_size`
in a fully headless suite. `CLAUDE.md` says why: *"the TUI suite is fully
headless and needs no terminal … which is how a cloud thread shows what a
layout change looks like."*

That instinct is exactly right and the GUI has no equivalent. Without one,
every agent working on the GUI is **writing a visual medium blind** — it can
confirm the code compiles and cannot confirm anything rendered.

The equivalent:

- Playwright against the Svelte dev server. **Chromium is already installed in
  the cloud container** with `PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`, so a
  cloud thread can do this today with no download.
- `make shots` — screenshot every route at three viewports into `shots/`.
- Commit them as goldens; a diff is a reviewable artifact, exactly as
  `make golden` already works for import snapshots.
- A `did_every_route_render` test — the direct descendant of
  `every_screen_draws_at_every_size`, and the one thing standing between a
  layout bug and a white screen.

Then a subagent can **look at its own output**, which changes GUI work from
"describe what you intended" to "check what happened".

---

## 4. `make dev-db` — a library worth rendering

There is no seeded-library target. `make synthetic` and `make corpus` generate
KOReader *import fixtures*; nothing produces a populated database. So every GUI
thread either starts against an empty shelf or against your personal library,
which is uncommittable.

`make dev-db` should produce a realistic one — ~200 books with real covers,
several rereads, notes with resolved and dangling links, a reflection reaching
three books, unattributed highlights, an abandoned reading — **and the edge
cases on purpose**: a book with `page_count = 0` (which is the false-denominator
bug in item 17b), a 1,400-page doorstop and a 48-page pamphlet next to each
other on the shelf, a book with no cover, a title long enough to need clipping,
an author who is `Surname, Given` and one who is a mononym.

Those edge cases are the visual regression suite. They are also, not
coincidentally, the exact inputs item 17's derived-facts layer has to get right.

Follow the `crates/corpus` discipline that is already written down: the
generator **does not depend on `readingbuddy`**, so a bug in the engine cannot
bake itself into the fixture.

---

## 5. Three more agents

`cargo-tester` is the right shape — *"never a wall of passing test names"* — and
should be the template. Three siblings:

**`web-checker`** — the direct mirror. `svelte-check`, `tsc --noEmit`, eslint,
`vite build`. Failures only, all of them in one pass, fixes nothing.

**`screenshot-reviewer`** — takes a route, runs `make shots` for it, reads the
PNG, and critiques it against `gui/CLAUDE.md`'s rules. The one agent that can
catch "this renders and it is ugly", which no test does.

**`api-surface-auditor`** — the important one, and the specific defence against
the failure mode this phase is most likely to hit. Given a GUI feature, it
answers: *can the API serve this?* — and when it cannot, it reports the gap as
an **engine item** rather than letting the frontend route around it. The rule in
item 25 (every call goes through the API vocabulary, a gap is a compile error
not a temptation) needs an agent whose whole job is enforcing it, because the
pressure to reach past a seam is highest at 11pm on the third day of a feature.

Give all three the "push back rather than comply" clause. It earned its place.

---

## 6. Write the prompts before starting, as you already do

`docs/prompts/` has 13 files, one per thread, and the spec is already
prompt-shaped. Write `17a`, `17b`, `17c`, `17d`, `17e`, `18`, `19`, `20`, `21`,
`22`, `23`, `24` before the wave starts rather than one at a time.

Two things to put in each, because the audit that produced the spec found both
in the codebase already:

- **The corrections this thread should expect to make.** Every item in
  `decisions.md` records what building it changed about the plan. Ask for that
  explicitly and it arrives; do not and it is discovered by the next thread.
- **What this item must NOT do.** Item 17's "prose stays in the frontends" and
  item 22's "no embedded reader" are the kind of boundary an eager thread
  crosses helpfully.

---

## 7. Parallelism: six threads, then a hard serialisation

**Items 17, 18, 20, 21, 22 and 24 share no files.** That is six concurrent
worktrees, and it is the same shape as the 11/12/13 wave that worked. Migration
numbers are pre-allocated (`0011` → 20, `0012` → 21, `0013` → 23) and merge in
numeric order, which is what `migration_versions_are_contiguous_from_one`
exists to catch.

Then it stops being parallel. Items 26, 27 and 28 share components, layout, the
design system and every visual decision — three agents in parallel there produce
three dialects of the same app. Run them in sequence, or split by *component
layer* rather than by feature (primitives → composites → routes), which is the
only decomposition of frontend work that does not collide.

Note the asymmetry this implies, and plan the calendar around it: **the engine
wave is extremely well suited to agents** — typed, testable, unambiguous
acceptance criteria, existing lint and test gates — and **the GUI wave is not**.
Front-load the fleet on 17–24. Keep a tight human loop on 26–28, where the
question is "does this feel like a place" and no test answers it.

---

## 8. Two skills, and a hook

**`gui-component` skill** — the conventions every component follows, so twelve
sessions produce one dialect: file layout, prop naming, where state lives, the
design tokens, how a component reaches data (through the client trait, never
directly), and the accessibility floor. `theme.rs`'s accent system already
exists and the GUI should inherit it rather than invent a second palette.

**`new-engine-item` skill** — the ritual `decisions.md` describes but nothing
enforces: pre-allocate the migration number, one PR per thread, write the spec
entry, record the corrections the build forced, write the session log. You have
`wrap-session` for the end; this is the start.

**A `PostToolUse` hook on `Edit`/`Write`** running `cargo check -p <crate>` or
`svelte-check` on the touched crate. Your `SessionStart` hook is genuinely well
built — the comment explaining why nextest is deliberately *not* installed is
the sort of thing that saves a future session an hour. The gap is that errors
currently surface at wrap-up rather than in the turn that caused them, and an
agent that learns about a type error six edits later has to re-derive the
context it already had.

---

## What not to spend effort on

- **Do not try to parallelise the visual design.** See item 7.
- **Do not let an agent decide the shelf's feel.** It can build the shelf, take
  a screenshot and tell you what is on it. Whether it reads as a place is yours.
- **Do not port the ray tracer to the GUI** to reuse it. Item 26 covers this —
  what crosses is four lines of arithmetic, and porting `blit.rs`, `kitty.rs`,
  `caps.rs` and `present.rs` would be ~3,300 lines of terminal plumbing with no
  GUI analogue.
- **Do not skip the API seam to move faster early.** It is the one shortcut in
  this phase that cannot be undone later, because by the time it hurts, every
  screen depends on it.

---

## Suggested order

```
now       1 (split CLAUDE.md)          ← afternoon, helps everything after it
          4 (make dev-db)              ← unblocks visual work
          6 (write the 12 prompts)
then      17 18 20 21 22 24            ← six parallel worktrees
          2 (ts types)  3 (make shots) ← alongside, before any GUI code
          5 (three agents)  8 (skills + hook)
then      19, 23                       ← depend on 20 and 21
then      25 (scaffold) → 26 → 27 → 28 ← serial, tight human loop
```
