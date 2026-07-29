# 2026-07-29 — the TUI halves of items 10 (Goodreads) and 13 (Calibre)

Picks up from `6422d12` on `main`. Both items had landed engine + CLI with their
TUI halves **never specced** — `docs/spec-08-10.md` deliberately ended item 10 at
the engine, and item 13 never named a frontend at all. So this is the same
situation as item 7's deferred TUI half, and like that one it is folded in here
rather than given a new number.

Asked for as "an interface to connect calibre and goodreads to the TUI". One
session, no worktree, direct to `main`.

## Decisions locked

- **Calibre is a shelf screen, not a preview-then-apply dialog** (user chose,
  from two mocked options). Reason: calibre is another system that *owns* books,
  and `docs/decisions.md` names it an origin for curated metadata — the way to
  meet one is to be shown its shelf and choose, the same argument that made the
  device screen a shelf rather than a file picker.
- **Full scope**: Goodreads import, Goodreads export, calibre library import,
  calibre status + `ebook-convert`.
- **Goodreads is deliberately *not* a shelf.** A CSV is matched against the
  library as a whole and the engine has no per-row import to stand on, so the
  screen is the dry run's own report and `s` applies the lot.
- **One `LinkPicker` for all three importers**, keyed by a new `LinkTarget`
  (`Sidecar` / `Calibre{uuid}` / `Goodreads{external_id}`). The widget, keys and
  band are identical and only the engine call behind `enter` differs; three
  copies would be three places for the band to drift from what the CLI offers.
  This renamed `App::device_link` → `link_picker` and moved the widget plus
  `clip`/`DETAIL_MAX` from `ui/device.rs` into `ui/mod.rs`.
- **Each screen claims its own key set, not one shared override table.** They
  overlap by four and differ by two, and the differences are the point (below).
- **calibre's rating stays unimported** and the shelf does not offer to. The
  reason is structural (a rating anchors to a review → a reading, which calibre
  knows nothing about) and a screen is not where that changes.

## What was built

- **`crates/tui/src/ui/calibre.rs`** — `Screen::Calibre`, `MenuItem::Calibre`.
  One row per `calibredb list` book in the state a **dry-run import** computed:
  `in library` / `new` / `maybe` / `unreadable`. `Enter` imports that row, `x`
  marks, `s` sweeps, `l` opens the candidate band, `n` is `--new` per row, `c`
  converts, `r` re-reads, `/` picks another library.
- **`crates/tui/src/ui/goodreads.rs`** — `Screen::Goodreads`,
  `MenuItem::Goodreads`. `/` reads a CSV as a dry run, `s` applies, `n` applies
  with `--new`, `l` links an undecided row, `x` writes the export.
- **`ui/mod.rs`**: `shelf_frame` (the shrink-wrapped box + key bar in the bottom
  border + empty state + the 1×1 zero-size guard), `link_picker`, `clip`,
  `DETAIL_MAX`, `candidate_hint`. `device.rs` shrank by ~110 lines onto them.
- **`ui/settings.rs`**: one calibre line — `converting and library import` /
  `converting only` / `library import only` / `not on this machine`.
- **Three deferred queues** beside `pending_scan`/`pending_pull`:
  `pending_calibre`, `pending_calibre_import`, `pending_goodreads`, all drained
  one unit per loop iteration by `pump_deferred`.
- **`Action::CreateAnyway`** and **`Action::Convert`**; `map_calibre_key` and
  `map_goodreads_key` beside `map_device_key`.
- **`Confirm::OverwriteConversion`**, four new `InputContext` variants.

## The engine gap the shelf shape exposed

The shelf could not be built on the existing surface. Four additions, all things
the CLI never needed because it imports all-or-nothing:

| Added | Why the CLI never needed it |
|---|---|
| `CalibreBookReport.calibre_id` | A report line named its row only by **title**, so a shelf could not tie the line back to the row it came from — and two editions of one title tie to each other. `uuid` will not do: it is `Option`, and the rows without one are exactly the rows that most need identifying. |
| `calibre::ImportOptions.only: Vec<i64>` | `Enter` on one row. Empty = all, so every existing caller is unchanged. Filters the **listing**, not the report, so `rows` and every count off it stay what the import considered — importing one book of four hundred must not report reading four hundred. |
| `Engine::link_calibre_book(uuid, book_id)` | — |
| `Engine::link_goodreads_row(external_id, book_id)` | — |

**The link methods are the load-bearing one.** Before them, neither importer had
any equivalent of `link_sidecar`: the only escape hatch for an undecided row was
`--new` followed by `merge_books`, which creates a duplicate *on purpose* in
order to fold it back in, and leaves the far side's id pointing at whichever of
the two the merge happened to delete. `l` on an unmatched row would have been a
dead end, which `docs/decisions.md` bans by name. The data was already there —
`UnmatchedCalibreBook.uuid` and `UnmatchedRow.external_id` are both carried and
both were previously unprinted — and `Storage::link_external_id` already
repoints on conflict. Only the facade method was missing.

Both go through one private `Engine::link_foreign_record`, kept **separate from
`Storage::link_external_id`** because that is a bare upsert: `external_ids.book_id`
references `books(id)`, so a stale candidate comes back as a raw foreign-key
error naming a constraint. A frontend offering a candidate list can always name a
book another pane has since deleted, so it gets `EngineError::NotFound`. An empty
id is refused rather than linked — calibre ids are per-library and reused after a
delete, and our own Goodreads export carries no `Book Id` column at all.

Mirrored in `crates/api`: DTO field, `only` on the request (`#[serde(default)]`,
so an older client's JSON still means the whole library), two typed methods, two
dispatch arms returning `Response::Unit`.

## Technical gotchas

Highest-value first. Each of the first three would have shipped looking fine.

- **`DETAIL_MAX = 56` clipped the row's *next move* off the end.** It was sized
  for a sidecar parse error, where clipping the tail is right. A candidate line
  composes a **data-derived title** and *then* a fixed-length hint
  (`maybe “…” (71%) — l to link, n if not`), so the clip ate `n if not` — a row
  saying what it might be and nothing about what to do, i.e. exactly the dead end
  the candidate band exists to prevent. First fix attempt (a second constant,
  `CANDIDATE_TITLE_MAX = 30`) *also* failed: 7 + 30 + 10 + 28 = 75 > 72. Two
  constants that each look fine are how they add up past the budget together.
  Real fix: `ui::candidate_hint` measures the line **with an empty title** and
  budgets the title against what is left, so the hint can never be what gives
  way. `DETAIL_MAX` raised 56 → 72; `LIST_MAX_COLS = 100` was always the real
  guard on box width.
- **A test that passed on CI and failed on this machine.** `test_app` leaves
  `EngineConfig::calibre_bin_dir` unset, so `Calibre::detect` falls through to
  `PATH` — and the dev machine has calibre where CI does not. An assertion that
  opening the calibre screen queues nothing is therefore machine-dependent.
  `opening_the_calibre_screen_reads_the_library_or_says_why_not` now **branches
  on `can_read_library()`**, with the absent branch's *wording* pinned separately
  as a pure function. That is the same split the CLI already makes in
  `an_absent_calibre_is_reported_and_never_prescribed`, and for the same reason:
  on a machine with calibre that branch is unreachable from outside the process.
- **`cargo test` was running `calibredb list` against the user's real calibre
  library.** `a_calibre_sync_...` pumps the deferred queue, and with
  `calibre_library: None` that means calibre's *default* library. Read-only, but
  nondeterministic and impolite. Fixed by pointing `calibre_library` at a
  directory with **no `metadata.db`**, so `library_root` refuses *before* the
  binary runs (item 13's own guard, reused) — hermetic on both machines, and the
  claim under test (the queue drains one row per pump) holds whether each import
  succeeds or reports.
- **`pending_calibre` is `Option<Option<PathBuf>>`.** The inner `None` is an
  *answer* — calibre's own configured library — not a missing value. Same reason
  `InputContext::CalibreLibrary` must be handled **before** `commit_input`'s
  empty-text check, alongside `NotePage` and `ReviewRating`: an empty answer there
  means "use mine", and falling into the generic empty branch would make it
  indistinguishable from changing one's mind.
- **`MENU`'s length is in its type** (`[(MenuItem, &str, &str); 10]`). Adding two
  rows means editing the `12`. Tests find rows via `menu_row(MenuItem::X)`, never
  a literal index — a literal passes for the wrong reason after the next row.
- **`tempfile` is not a `crates/tui` dev-dependency**, and adding it would churn
  `Cargo.lock` under CI's `--locked`. Used the `std::env::temp_dir()` +
  process-id + atomic-counter pattern `test_app` already uses (`fn scratch`).
- **An absent calibre must not be reported as a failure.** `finish_calibre_scan`
  first wrapped every error as `couldn't read the library: {e}`, which frames
  `CalibreMissing` as something to fix — `docs/decisions.md` says absence is a
  first-class answer. Split out into `describe_calibre_failure`, with a
  forbidden-word assertion (`install`/`download`/`brew`/`http`…) on the absent
  branch and a check that a mistyped path *still* reads as a real failure.
- **Pre-existing clippy warning fixed in passing**: an empty line after the doc
  comment on `app::tests::screen_text` (`empty_line_after_doc_comments`). Present
  before this session; `git diff` on the pristine tree confirms it.
- **`print_lists` leaked the `LinkPicker` across iterations**, drawing the
  chooser over every screen after the device one. The dev aid now clears it per
  iteration — an aid that obscures what it is showing is not one.
- `Block::inner` on a 1×1 rect has no inside, and `Paragraph` into a zero rect
  panics. `shelf_frame` carries the guard once, for all three shelves.

## Verification

- `make ci` (fmt + `clippy --workspace --all-targets -D warnings` +
  `cargo check --workspace --locked` + `cargo test --workspace`): **pass**.
- **711 passed / 0 failed** workspace-wide. Engine 394 (calibre 18, goodreads
  18), api 33, cli 13, tui 260, daemon 10, corpus 2.
- New tests: 7 engine (calibre `only`/`calibre_id`/linking), 3 engine
  (goodreads linking + cross-source key collision), 3 api, 3 event-map,
  2 `ui/mod` properties, 7 `ui/calibre`, 8 `ui/goodreads`, 12 `app`.
- `every_screen_draws_at_every_size` extended: both new screens, both populated
  *and* empty, the calibre candidate chooser, and the overwrite confirm — down to
  1×1, at every ambient motif.
- Rendered both screens through `print_lists` (96×16) and read them; extended
  that aid to cover them permanently.
- Smoke-checked that the CLI is unchanged: `goodreads import --dry-run` against
  the recorded fixture (5 rows, correct outcomes), `calibre status`,
  `--dump-frame`.

## Deferred

- **The hint truncates below ~100 columns** — the box clips the line's tail, and
  `row N` on Goodreads rows costs it 8 more. Judged acceptable rather than
  reordered: the **key bar in the bottom border** always carries `n as new`, which
  is precisely why it rides there. Reordering to put the hint before the authors
  would make matched and undecided rows inconsistent.
- **No per-row Goodreads import.** Engine shape, not an oversight: the matcher
  runs a row against the library, so there is nothing to stand on. A Goodreads
  link therefore re-runs the *preview* rather than importing one row, which is
  what makes the row visibly leave "unmatched".
- Calibre tier (iii), device push — still out of scope.
- `Storage::book_for_external_id` is still not on the facade; the TUI does not
  need it (the dry run answers the same question), but a "which system knows this
  book?" surface would.
- **`logs/tui.log` is still tracked** although `/logs` was added to `.gitignore`
  (a change already in the tree at session start). Left unstaged; making the
  ignore effective needs `git rm --cached logs/tui.log`, which is the user's call.
