# 2026-09-02 — the minimal pass

Worktree session (`.claude/worktrees/gui-minimal-aesthetic`, branch
`worktree-gui-minimal-aesthetic`), based on `main` @ 854f797. Frontend only —
no engine change, no migration, no `API_VERSION` move, no new request.

Settled account: `docs/decisions.md` entry **57**. Routing: `gui/CLAUDE.md`.

## Decisions locked

- **The rule is Ousterhout's deep modules, applied to screens** — the user's
  own framing, chosen over "hide it behind a disclosure" and "delete it". A
  screen is a module, its controls are its interface, its capability is its
  depth; a screen with twelve controls is *shallow*. Four corollaries are now
  review rules in `gui/CLAUDE.md`: one question per surface; no pass-through
  controls; a door not a duplicate; the instrument belongs to the work.
- **Quantity is a property of a page, not of the app.** `/library` and `/life`
  stay dense; a page that is not *about* quantity may not carry its controls.
- **`/cards` splits.** Quiet page (six most recent reads, one request, no
  count, fixed 3-col grid, one door) + `/cards/history`, which took every
  filter/sort/pager/tally unchanged. History gets **no nav entry** — arriving
  through the door is what makes its count something you asked for.
- **`/book/[id]` is one work surface.** Reverses entry 53's three columns, on
  the user's explicit pick. `Rail.svelte` deleted.
- **`/notes` loses its source switch.** Judged a pass-through control.
- **A fill is an action; a state is an outline.** One accent fill per surface.
- **The card keeps its box** — the pass unboxed it, the user said the structure
  was better, and the argument for putting it back is the durable half: a card
  is a *composite* of five unlike things, and whitespace separates repeating
  shapes rather than bounding a ragged one. What stayed removed is the
  shouting (brass state word, `No passage from this read` × 24).

## Technical gotchas

- **`grim` captures a screen *region*, not a window.** `driver.sh shot` reads
  the GUI's geometry from `hyprctl` and then grabs those coordinates — so if
  the window is not on top, you capture whatever is. It twice grabbed
  something else on the user's desktop (a browser, then the agent's own
  terminal). Relaunching per shot (`stop` → `gui`) is more reliable than
  `shot` alone, because the window is focused right after it maps, but it is
  still a coin flip under a tiling WM. **Look at every capture before trusting
  it, and delete it if it is not the app.** This should probably be in
  `run-readingbuddy`'s Gotchas.
- **A worktree branches from `origin/main`, not local `main`.** `worktree.baseRef`
  defaults to `fresh`; local `main` was 2 ahead. `git reset --hard main` first —
  which is the check CLAUDE.md already tells every worker to run.
- **`$derived` cannot narrow a `$state` it closes over.** `box === null ? null :
  (t) => insert(box, t)` fails svelte-check: the closure re-reads the state, so
  narrowing at construction says nothing about call time. `$derived.by` with a
  local binding is the fix, and the local is also the *correct* semantics — the
  element captured is the one that was mounted when the writer was made.
- **eslint catches what svelte-check does not.** Removing the right rail left
  `readLines` derived and unused; `svelte-check` and `tsc` were both green and
  `@typescript-eslint/no-unused-vars` was the only thing that said so.
- **The temporary screenshot harness** (a `SHOT` const + a `goto` in the root
  layout) is the only way to see a route other than `/`, since nothing can
  click the GUI. It must be reverted before committing — `git checkout
  src/routes/+layout.svelte` plus `rm src/routes/shot.ts`. It cannot carry a
  query string: the effect compares `pathname`, so `?note=1` loops for ever.
  To see the editor, seed `openNoteId` in the page instead and revert.

## Verification

- `make ci` — exit 0. fmt, clippy `-D warnings`, `cargo check --workspace`,
  `ts-check`, 615 Rust tests (574 engine + 30 + 6 + 5), and `web-check`:
  svelte-check 442 files / 0 errors, tsc, eslint, **314 vitest**, build.
- Looked at, against `dev-data/`: `/` (entrance), `/library`, `/cards`,
  `/book/3` (passages), `/book/3` with a note open (editor + `Notes` lit in
  the selector). Before-and-after for the first three.
- **Not looked at**: `/notes`, `/cards/history`, `/reading`, `/devices`,
  `/life`, and the editor's `Connections` block below the fold — the window
  was too short and the screenshot path was grabbing the wrong window. Covered
  by checks and by reading, not by pixels.

## Deferred

- A `pnpm tauri dev` pass over the six unphotographed surfaces.
- `/notes`' scope chips are the most reversible call in the pass: if they turn
  out to be load-bearing, they come back and nothing else changes.
- Note **kind** filtering on `SearchMarks` is still an engine item, and it is
  what a legitimate `/notes` filter would be built on.
- The book page's work surface is capped at `--passages` (44rem) and sits
  flush left in a 1400px shell, so a wide window has a lot of empty right.
  Deliberate (a measure is a property of what is being read) but worth a look.
