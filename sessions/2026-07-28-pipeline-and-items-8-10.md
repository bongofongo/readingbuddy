# 2026-07-28 — the gate, the workflow tests, and build items 8–10

Picks up from `2026-07-28-engine-04-07-thread-plan.md` (items 4–7 planned) and
`303320a` (items 1–7 merged). Two phases: **Wave 0**, hardening CI and adding a
workflow-test layer; then **items 8, 9 and 10**, run as six parallel threads in
git worktrees, each merged by PR.

Ends with `79c9f1d` on `main`: migrations `0001`–`0009`, six PRs merged (#4, #5,
#7, #8, #10, #11), one docs PR open (#9).

## Decisions locked

- **CI gates the whole workspace on ubuntu, engine-only on macOS.** User's
  choice over "keep engine-only" and over "workspace on both". Reverses a
  documented decision: the TUI suite was ungated on the argument that the engine
  is what CI gates, which held only while every TUI change was written and run by
  hand on the dev machine. macOS stays engine-only because it earns its slot for
  the two C deps that build differently there, not for running the ray tracer
  twice.
- **PR + green CI + human review. Nothing auto-merges.** User's choice over
  auto-merge for engine-only items.
- **Planning horizon: the next wave only.** User's choice over "8–13 detailed" and
  "all of 8–16". Items 11–16 get re-planned against what this wave produced.
- **Test layers built: engine narrative workflows + CLI subprocess.** User
  declined the tier-3 device-scale generator and TUI scripted-session tests.
  Sample data is therefore the committed tier-1 corpus staged into a tempdir.
- **The local database does not matter.** User said so explicitly. Retires the
  item-4-era constraint that a destructive migration be verified against a copy
  of a real `database/app.db`. Migration *numbering* discipline survives and is
  now mechanised.
- **Item 8b absorbs item 7's deferred TUI half.** Item 8's action is "open the
  reflection" and the TUI had no reflection surface at all, so the two are one
  change or they are two passes over the same four exhaustive matches.
- **Keys left as each thread chose them** (`e` reflect, `w` review, `L` links).
  They do not collide; a keybind pass comes later.

## The four spec errors the threads found

Written by this session, caught by the threads executing them. **Three of the
four would have shipped green**, which is the point.

| Item | The error | Why it was invisible |
|---|---|---|
| 8a | `WHERE cur.finished_at IS NULL` | `BOOK_FROM` is a **`LEFT JOIN`**, so a book with *no* reading has every `cur` column NULL and satisfies the predicate. The whole library reads as "currently reading". Correct filter: `cur.id IS NOT NULL AND cur.finished_at IS NULL`. Found by writing the empty-library test first. |
| 9a | `CREATE INDEX … ON note_links(target_title)` | BINARY collation, while `write_links` back-resolves with `= ? COLLATE NOCASE`. **SQLite ignores an index whose collation differs**, so the plan stays `SCAN note_links` — an index that exists, reads correctly in the schema, and does nothing. Ships as `ON note_links(target_title COLLATE NOCASE)`. |
| 10 | `Read Count > 1` → "the rest get NULL dates" | **Not representable.** `finished_at IS NULL` *is* what open means and `idx_readings_one_open` permits one per book, so N−1 of them is a constraint violation. Resolution keeps the principle (never invent a date): `started_at` NULL on all, earlier readings closing at the date the row carries — a true upper bound. |
| 9b | "show your work with `print_layout print_lists`" | Neither dev aid ever rendered the book view's **entered** Notes section, so a straight run would have displayed nothing of the change. The thread extended `print_layout` rather than reporting a dump that proved nothing. |

Errata are recorded inline in `docs/spec-08-10.md` and in each prompt, marked
`Corrected during/after implementation (PR #n)` rather than silently edited —
the prompt is what a thread was actually handed.

## Bugs found

- **`a_glyph_spin_stays_inside_its_byte_budget` needed a terminal it claimed not
  to need.** Pre-existing; caught by the widened gate on its *first* CI run.
  Three byte-budget tests use a real `CrosstermBackend` (counting wire bytes is
  the point; `TestBackend` writes none) and set a fixed `Viewport` with a comment
  saying the terminal never asks the OS for a size. **`redraw` itself calls
  `terminal.size()` every frame** — the resize hook that invalidates a
  transmitted kitty image — and a fixed viewport changes what *ratatui* asks for,
  not what `redraw` asks for. On a runner with no `/dev/tty` the ioctl fails
  `NotFound` and the test dies on its first draw. Fixed with `FixedSize<B>`,
  which answers `size`/`window_size` from the test's arguments and delegates the
  rest. **Mechanism inferred, not reproduced**: it is the only OS-touching call
  between a `Terminal` that constructed fine and a `draw` that never ran, but
  this sandbox always has a `/dev/tty` and masking it with a bind mount did not
  reproduce. CI going green was the evidence.
- **Two threads independently added an identical `screen_text` test helper**, in
  different regions of `app.rs`. Git merged both with **no conflict at all**; it
  surfaced only as a duplicate-definition build error during the #11 merge.
- **The `cargo-tester` agent did not exist.** `CLAUDE.md` and the wrap-session
  skill both instructed the reader to launch it. Now defined — and then found to
  be unlaunchable by subagents anyway (below).

## Technical gotchas

- **Subagents cannot spawn subagents.** All five implementation threads were told
  to run the `cargo-tester` agent; all five independently discovered there is no
  tool to launch it with, and all five ran its procedure by hand. Every prompt now
  names the fallback (`make check`) *and* asks which procedure was actually run —
  running it by hand weakens the agent's "call out `SKIPPED:` lines" instruction,
  because cargo captures stderr on passing tests.
- **`sqlx::migrate!` is a compile-time macro.** Dropping a new `.sql` into the
  directory does not invalidate the test binary, so
  `migration_versions_are_contiguous_from_one` can appear to pass locally until
  something else forces a rebuild. CI always compiles from a fresh checkout.
- **`READINGBUDDY_REQUIRE_FIXTURES` was set nowhere in the repo.** The guard that
  turns a `SKIPPED:` into a failure existed and had never once fired. Now set on
  the scheduled corpus job, and scoped to it: set globally it fails on `real/`
  and on `partial_md5.rs`'s opportunistic `personal_data/` checks, both absent by
  design.
- **The tier-2 corpus ran on no trigger anywhere.** `crates/corpus/src/main.rs`
  says "nightly only" and no workflow referenced it. That — not the gitignore
  alone — is why the corpus convergence bug survived item 2.
- **gutenberg.org and its mirrors are blocked by the cloud sandbox proxy**
  (CONNECT 403); crates.io is fine, `get.nexte.st` is not. `make corpus` can only
  ever run on a hosted runner, and nextest cannot be installed from its prebuilt
  binary in a session.
- **`config_file::config_path` falls back to `home_dir()/.config`** when
  `XDG_CONFIG_HOME` is unset. A CLI subprocess test that pins only XDG leaves
  `config set google-api-key` able to **write the developer's real key file**.
  Pin `HOME` too.
- **`active_rating_scale()` was `ORDER BY created_at DESC`**, so seeding *any*
  new scale silently changes what `rating show` and `set_rating` default to.
  Migration `0009` adds `is_default` for that reason alone, back-filled to
  whichever scale the old ordering would have picked — not to `default` by name.
- **`clippy --all-targets -D warnings` reaches `tests/common/mod.rs`**, which is
  compiled into every test binary that declares it. A helper used by one file is
  dead code in the others: `#![allow(dead_code)]` with the reason written down.
- **TUI threads do not need a terminal.** The suite is `TestBackend`;
  `--dump-frame`, `print_layout` and `print_lists` are all headless. The item-4
  spec claimed otherwise, and that claim was the only thing that would have kept
  8b/9b off the cloud.

## What was built

**Wave 0** (#4): ubuntu `--workspace --locked` / macOS `-p readingbuddy
--locked`; a PR-only `migrations` job refusing any modified, deleted or renamed
migration (`--diff-filter=MDR`); `migration_versions_are_contiguous_from_one`;
the tier-2 corpus on `scheduled.yml` with `REQUIRE_FIXTURES=1` and an epub cache
keyed on the manifest hash; `.claude/hooks/session-start.sh` (toolchain warm,
`cargo fetch --locked`, workspace+tests build, off `CLAUDE_CODE_REMOTE`);
`.claude/agents/cargo-tester.md`; `crates/engine/tests/common/mod.rs`;
`crates/engine/tests/workflows.rs` (7 narrative stories crossing build items);
`crates/cli/tests/cli.rs` (7 subprocess stories, zero new deps beyond `tempfile`).

**Items 8–10**: `list_open_readings` + `currently_reading` + `NoteRecord`-returning
reflection accessors (#5); `Storage::backlinks`/`outgoing_links`, migration `0008`,
`rb links` (#7); Goodreads CSV both ways, migration `0009`, `book_tags`,
`external_ids`, `rating_scales.is_default`, `corpus gen-goodreads` (#8); the
backlinks pane on the note list (#10); `Screen::Home` + the reflection/review TUI
(#11).

## Two behaviour changes worth knowing

- **`Esc` on the menu used to quit the app.** With the menu no longer the front
  door that is a trap — a key that exits from one screen in. It now returns home;
  `Home`'s Back arm quits; `q` still quits everywhere.
- **The expanded key bar now cuts before `m menu` at 110 columns**, having gained
  three pairs. Truncation is by design (`break` on overflow) and the *collapsed*
  bar — the default — is four pairs and always carries `m` and `q`, so nothing is
  unreachable. The collapsed bar does not advertise `e`/`w`/`L` at all.

## Verification

- `make ci` and `cargo test --workspace --locked` green on every merge, run
  against the **merged** tree rather than either parent for #8 and #11.
- Final state: 203 engine unit, 206 TUI, 11 CLI, 11 migrations, plus every
  integration suite. Clippy `--workspace --all-targets --locked -D warnings`
  clean. `cargo deny check bans licenses sources` ok.
- **Both new CI guards were verified by making them fail on purpose**, not by
  watching them pass: a second `0007` reds the contiguity test; the byte-budget
  fix was confirmed by CI rather than locally, and that limit is recorded.
- After each hand-resolved merge, the signature tests of *both* threads were run
  individually rather than trusting the aggregate count — a spliced test is
  exactly where an assertion stops asserting without saying so.

## Deferred

- **Tier-3 device-scale corpus** (`docs/decisions.md` item 1's follow-up).
  Declined this session. It is the only thing that would reach `is_koreader_mount`
  with a satisfying fixture, `.lua.old` at real density, and sidecars with zero
  annotations — the majority case on a real device, of which tier 1 has one.
- **TUI scripted-session tests.** Declined. `every_screen_draws_at_every_size`
  proves nothing panics, not that anything is *reachable*.
- **The same-titled-note gap** (9a, pinned not fixed): `notes.title` is not
  unique, so an edge resolved to one of two same-titled notes dangles again if
  that one is deleted. Deliberately not unioned into `backlinks` — the two
  directions must read one edge set from opposite ends, or the pane claims an
  inbound link the linking note denies writing. The fix is write-side.
- **Keybind pass**: `e`/`w` are unspent letters rather than mnemonic ones. 8b
  chose them believing shifted keys were unavailable; 9b then proved they are by
  binding `L`. `R`/`W` are free.
- **Shelf state does not survive a Goodreads round trip** into a fresh library —
  our export carries no `Exclusive Shelf` column because Goodreads' importer does
  not read one. Inherent to the eight columns.
- `cargo test --doc`, an MSRV job, and a notifier for a red scheduled run.
- Items 11–16 — planned at roadmap level in `docs/spec-11-16.md`.
