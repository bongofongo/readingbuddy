---
title: Orchestrating the non-GUI wave — items 34–38
date: 2026-08-06
follows: sessions/2026-08-06-one-fixture-two-consumers.md
covers: the wave as a whole, from the orchestrator's seat; the five per-item
        logs are the work, this is the running of it
---

# Session log

Ran `docs/handoff-orchestrator-non-gui-wave.md`. Five items, three workers in
parallel, then one worker, then one item built here. `main` went `fcebced` →
`e4534dd`, `make ci` exit 0 at every merge point.

The five per-item logs carry the engineering. This one carries what the
orchestration cost, because the handoff that started this wave was mostly a
record of the *last* wave's orchestration and that is the part that paid.

## What the handoff got wrong, and how much each cost

Three of its stated facts were wrong. None was expensive to fix; all three were
expensive to *find*, which is the argument for writing them down here.

- **The merge order was impossible.** It stated `34 → 36 → 33` (in its own
  numbering) on the grounds that the item owning the API surface should merge
  last, so `bindings.ts` — a generated whole-file conflict — is resolved once.
  Sound reasoning about files, and it ignores migrations: the item it wanted
  first owned `0016` and the item it wanted last owned `0015`, and
  `migration_versions_are_contiguous_from_one` fails on a **gap**. Landing
  `0016` first leaves `main` red until its predecessor arrives, and the handoff
  also required `make ci` after every merge, so the two instructions could not
  both be obeyed. Reversed to `37 → 34 → 35`. The generated-file conflict still
  happened exactly once, because the PDF item had touched `bindings.ts` too — so
  the reordering cost nothing at all.

  **The generalisation worth keeping: merge order is constrained by migration
  contiguity first and by file collisions second.** Contiguity is a hard
  ordering; a conflict is a cost.

- **`main` was not where it said.** Two doc commits had landed after the handoff
  was written. Harmless, and a reminder that a handoff's stated SHA is a claim
  about the moment it was written.

- **Item 34's premise was sharper than described.** The handoff said
  `ORDER BY sort_author` was "silently wrong until a back-fill runs". There is no
  `sort_author` column at all — `BookSort::Author` reads the whole library and
  sorts in Rust. What *did* exist was `sort_title`: in the schema since
  `0001_init.sql`, bound by the upsert, on `Book`, on `BookDto`, in the generated
  TypeScript, asserted `Federated::Local`, written as literal `NULL` by
  `gen-devdb` — and computed by nothing, ever. The prompt was rewritten with line
  numbers and gave the worker explicit leave to delete the column instead.

## The numbering collision

The wave allocated itself 33–37 by reading `docs/prompts/` for the highest
number. I confirmed that to the user as "item 32 is the last built", and it was
wrong: **item 33 was already spent** by "Surfacing 21/29/30/31/32" on 2026-08-05.
That item was minted mid-session rather than from a spec, so it has a
`decisions.md` entry and a session log and **no prompt file**.

Found while resolving a `decisions.md` conflict — two entries numbered `33.`
appeared in the same file, four commits in.

The user chose to shift the wave to 34–38. Cost: one commit across 32 files,
because a worker writes its item number into module headers, migration file
headers, test section comments and `CLAUDE.md` routing rows. **A number is not a
label on a document; it is a fact scattered through the source.**

Recorded as `new-wave-item` step 2a, and as the first section of the new handoff:
`grep '^[0-9]\+\. \*\*' docs/decisions.md` is the register. `docs/prompts/`
under-reports permanently, and any item that begins life as a handoff's open work
has that shape.

## The mistake that reached `main`

Merged `feat/engine-sort-keys` and read `git merge`'s output through `tail -3`.
That showed the **last** `CONFLICT` line and hid four above it. Five files
conflicted; I resolved one, committed the other four with markers in them, and
`make ci` caught it two steps later — `encountered diff marker` in
`storage/query.rs`.

This is the rule I had written into all three worker prompts, in a different
costume. The known form is "never read a pipeline's exit code, because
`make test | tail -25` reports *tail's* status". The form that bit is **never
read a truncated report**: the exit code was fine, the summary was not.

All five were both-sides merges — an added test beside a rewritten doc comment on
the *next* test, two routing bullets against one, `0013, 0014 and 0015` against
`…and 0016`. The dangerous one was `crates/cli/tests/cli.rs`, where the two items
had added *different* tests that happen to share four setup lines, so git aligned
them into each other and the file read as one test with two bodies. Resolving
that by taking a side would have dropped an item's CLI-door test with nothing
failing — the "a behavioural test that cannot fail is not a guard" failure,
arrived at by merge rather than by design.

Folded the repair into the merge commit with `--amend` rather than stacking a fix
on top, so no commit on `main` is left non-compiling. Kept the two earlier merge
commits rather than rebuilding history: the worker branches were cut from them,
so removing them would have meant rebasing all three, and the honest record is
that the wave started on the wrong numbers and found out.

## Decisions locked

- **Worktrees live in `.claude/worktrees`**, which `.gitignore` already names as
  "Parallel-thread git worktrees". Each got `.cargo/config.toml` with
  `incremental = false` and `/.cargo/` in `.git/info/exclude`, so no worker could
  commit it.
- **Round 2 reused round 1's worktree** rather than cutting a fresh one:
  `git checkout -b … main` inside it, then `git worktree move` to rename. Kept
  the 51G warm target and started hot.
- **Item 38 ran here, not in a worktree**, for the handoff's stated reason and it
  is a good one: it touches `gui/`, a fresh worktree has no `gui/node_modules`,
  and `web-check`/`routes` degrade to a stated `SKIPPED:` there. A worker would
  have "passed" both without running either.
- **Workers were gated on `make fmt lint build-check test ts-check`**, never
  `make ci`, for the same reason. The full gate ran here after every merge.

## Measurements, since the handoff's estimates were off

- **APFS clone: 70 seconds per worktree**, not the ~10 minutes budgeted.
  `target/debug` was 59G of which `incremental/` was 29G; stripping it and
  cloning the rest moved total free disk 56Gi → 55Gi, i.e. the copy-on-write is
  real. A cold `cargo check -p readingbuddy` in a cloned worktree finished in
  **15.8s**, so fingerprints survive the path change — worth knowing, because the
  opposite would make the whole exercise pointless.
- **Disk did drift** as the three diverged: 56Gi → 39Gi free before I removed the
  two merged worktrees. Three is comfortable on this machine; four would not be.
- **Round 1 wall clock**: ~24 min, ~38 min, ~38 min for the three workers.

## What the workers overturned

Four of five pushed back, and every one was right. They are in `decisions.md`;
the orchestration point is that **each refusal was invited by name in its prompt
file**, in a "Push back rather than comply" section naming the two or three
places the prompt was most likely wrong. Three of the four refusals landed on a
place the prompt had flagged. That is a cheap paragraph to write and it is doing
most of the work.

The one I would have gotten wrong on my own: item 34 refusing to rank notes and
highlights on a shared bm25 score. My prompt asked for one ranked list and
flagged the question; the worker measured that fts5 computes rank from each
index's own corpus statistics, so the two numbers are incommensurable, and merged
by within-source position instead.

## Gotchas

- **`git merge-tree --write-tree --name-only` is the safe dry run.** Used it
  before the last two merges; it reported "NO CONFLICTS" for item 36 and was
  right. Should have used it — and read it whole — for all four.
- **`grep --include=*.rs` needs the glob quoted in zsh**, or the shell expands it
  and the grep silently searches nothing. Cost one round of "no matches found"
  that read like "no such code exists".
- **`cat ~/.claude/skills/…` with a `find /` fallback** backgrounded a whole-disk
  scan. The skills were in `.claude/skills/` (project), not `~/.claude/`.
- **21 committed PNGs moved** when item 38's fixture changed. `make routes` fails
  on a 1% pixel diff, which is correct; regenerating with `make shots` and then
  *reading* them is the step `gui/CLAUDE.md` asks for and the step that is easy to
  skip. The library grid is what showed `The Claw Of The Conciliator` finally
  rendering `Reading · 33%`.

## Left for later

Everything is in `docs/next-thread-handoff.md`. The two that are the user's
rather than a worker's:

- **The border-median accent arithmetic**, duplicated between `images.rs` and
  `render3d/texture.rs`. Now carried unresolved across three handoffs. The two
  measure different things (original file vs scaled texture) so they can
  legitimately differ, and the renderer is frozen — deleting its copy is a
  decision about what it draws. It wants an answer more than an owner.
- **The GUI half — 26, 27, 28, then 23 on `0017`** — in sequence and never in
  parallel. Whether a shelf reads as *a place* is not an agent's call.
