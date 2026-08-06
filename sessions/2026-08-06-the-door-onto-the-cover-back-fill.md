# 2026-08-06 — the door onto the cover back-fill

Ran `docs/next-thread-handoff.md`. Its finding 1 — "the cover back-fill has no
door", flagged **do this before item 26** — was the whole session. It was
predicted as two lines and was not, because building the door made a second
defect observable for the first time.

## Decisions locked

- **`rb covers` is the verb**, a bare subcommand with no flags. Item 20 already
  argued why it is a command and not a migration (`cover_width` is the result of
  decoding a PNG; SQLite cannot decode one). It is idempotent — the work list is
  `cover_path IS NOT NULL AND cover_width IS NULL` — so `make dev-db` can run it
  unconditionally and a second run measures nothing.
- **Its wording obeys both absence rules.** `summary()` is a pure function so
  they can be asserted: an already-measured library says
  `every stored cover is already measured.` and never `measured 0 covers`
  (absence is not zero, and after the first pass this is the line the user sees
  every time), and an unreadable file states what happened to it — nothing was
  written, the next run retries — rather than becoming a count of work not done.
- **`gen-devdb` now takes `--data-dir` beside `--out`.** They are different
  directories: `--out` is where the seed and the PNGs are written, `--data-dir`
  is where the library gets built. `GENERATOR_VERSION` → 2.
- **Did not touch the GUI half.** Items 26/27/28 and 23 stay sequential and the
  shelf's feel stays the user's call; the handoff is explicit on both.

## The bug the door exposed

`make dev-db` with the new line printed **`202 covers would not decode`** — the
whole library, on the first try.

- `gen-devdb` wrote `cover_path` as `database/images/dev-NNNN.png`, **relative**,
  with a comment defending the choice: an absolute path baked into a *committed*
  seed names one machine.
- The premise was false. `corpus/generated` is gitignored and `make dev-db`
  regenerates the seed on the machine that runs it, so the relative shape
  protected nothing.
- The cost was real and had two victims. `images::measure_stored` does
  `std::fs::read(cover_path)`, so it resolves against the *process cwd* — and
  `make dev-db` runs cargo from the repo root, not from `dev-data/`. Running the
  same binary from inside `dev-data/` measured all 202, which is how it was
  confirmed rather than reasoned. The second victim is the one that mattered
  more: a webview has no cwd at all, so **item 26's shelf could never have
  resolved a single cover** off a dev library.
- `cover_path` is a whole path in this schema — `Storage::set_cover` stores
  `images_dir.join(name)` and `gui/CLAUDE.md` says never join `images_dir` back
  on. The seed now states what the engine itself would have written.
- **Notes are the opposite and stay that way**: `notes.file_path` is
  vault-relative by the engine's own convention, so a bare filename there is
  correct. Checked rather than assumed — the two columns having different
  conventions is exactly what makes this class of drift survivable.

## Gotchas

- **A fixture can disagree with the engine about a column's shape and nothing
  notices.** No test read a file that a seeded `cover_path` names, so the drift
  was invisible until a command did. This is the handoff's finding 8 one layer
  down (`gen-devdb` vs `fake.ts`), and the same shape as its "a sort-key column
  with no writer looks answered": *the value was present and wrong*, which reads
  identically to present and right.
- **`Bash` cwd persists between calls.** A `cd dev-data` to test the cwd theory
  silently relocated the next two greps, which came back empty and looked like
  the files had moved. Use absolute paths after any `cd`.

## Verification

- `make ci` **exit 0** — fmt, clippy `-D warnings`, `cargo check --workspace`,
  `ts-check`, whole-workspace tests, `web-check`, 30 Playwright routes on WebKit.
  Run instead of the `cargo-tester` agent because it is a strict superset of it.
- New: `covers::tests` (two wording tests) and
  `the_cover_back_fill_is_reachable_from_the_binary` in `crates/cli/tests/cli.rs`
  — a process-level test, since "the engine can do it and nothing can ask" is
  precisely the failure that is invisible from inside the process. The counts
  stay `engine_facade.rs`'s business.
- `the_subcommand_set_is_what_we_decided` updated in the same commit, which is
  the point of that golden.
- Live: `make dev-db` rebuilt → `measured 202 covers.`, all 202 rows carry
  `cover_width`/`cover_height`/`cover_accent`, paths absolute; a second `rb
  covers` says `every stored cover is already measured.`
- `cover_thumb_path` is NULL for all 202, correctly: the fixture's covers are
  240×360 and below the shelf tier, so `cover_shelf_path` falls back to the
  original. A shelf tile in the GUI will therefore load full covers off
  `dev-data` — fine at this size, and worth remembering when the fixture grows.

## Deferred

- Handoff findings 2–8 all still open and still unnumbered (highlights FTS,
  sort-key indexes, `sort_author`, `MatchCandidateDto`'s missing author, the
  duplicated accent arithmetic, a real PDF sidecar, `gen-devdb` vs `fake.ts`).
- Finding 8 got *worse-shaped*, not worse: `fake.ts` still serves no covers, so
  the cover layout a real library now supports still has no headless regression
  test.
- Item 26 remains the next GUI move and remains sequential.
