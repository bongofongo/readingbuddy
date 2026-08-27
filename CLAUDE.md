# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**readingbuddy** (formerly "reading_card"/"BookBuddy") is a Rust reading companion: federated book-metadata search (OpenLibrary + Google Books), epub import, KOReader highlight/note import, a markdown note vault with zettelkasten links, flashcard capture, all persisted to local SQLite. Two frontends share the engine: a thin CLI, and a ratatui TUI (designed to live in a tmux pane) whose centrepiece is a ray-traced 3D book rendered in Unicode block-glyph cells (octant/quadrant). A third seam, `readingbuddy-api`, carries the same surface as serializable DTOs, with `readingbuddyd` serving it over a unix socket for the GUIs still to come. An animated pixel companion is still to come.

See `plan.md` for the original design journal and `~/.claude/plans/` history for the 2026-07 engine redesign rationale.

**Product direction lives in `docs/decisions.md`** — what is settled, no reasoning; read it before proposing features. `docs/ux-positioning.md` is the argument behind every line of it, and `docs/spec-engine-01-03.md` + `docs/prompts/` carry the next three engine items. The design axiom is **"a place, not a tool"**: state persists and is visible, nothing is modal-by-default or a dead end, idle is not blank, and there is **no task-completion framing** — places you can go, never a badge counting what you haven't done. Data ownership is **per-field with provenance recorded**: readingbuddy keeps a durable local copy of everything but is not the *origin* of what it copies (KOReader owns highlights and reading state, calibre owns files, providers own bibliographic metadata), and conflicts resolve toward the origin.
## Where things are — go here first

This file is the whole-repo context: the axiom, the conventions, the engine's
quality bar. **The detail for each crate lives in that crate's own
`CLAUDE.md`**, which loads when you work there. Do not read them all; route.

| what you are doing | read |
|---|---|
| **anything at all** | this file — then exactly one row below |
| a schema change | [`crates/engine/migrations/CLAUDE.md`](crates/engine/migrations/CLAUDE.md) **before writing the file** |
| SQL, `Storage`, readings, merges | [`crates/engine/src/storage/CLAUDE.md`](crates/engine/src/storage/CLAUDE.md) |
| metadata search, providers, "is this the same book" | [`crates/engine/src/providers/CLAUDE.md`](crates/engine/src/providers/CLAUDE.md) |
| KOReader, calibre, Goodreads, owned files, the vault | [`crates/engine/CLAUDE.md`](crates/engine/CLAUDE.md) |
| a TUI screen, layout, help pages | [`crates/tui/src/ui/CLAUDE.md`](crates/tui/src/ui/CLAUDE.md) |
| the 3D book, glyphs, kitty, terminal caps, perf | [`crates/tui/src/render3d/CLAUDE.md`](crates/tui/src/render3d/CLAUDE.md) |
| the TUI event loop, deferred work, ambient layer | [`crates/tui/CLAUDE.md`](crates/tui/CLAUDE.md) |
| the API surface / DTOs / wire protocol | [`crates/api/CLAUDE.md`](crates/api/CLAUDE.md) |
| the daemon | [`crates/daemon/CLAUDE.md`](crates/daemon/CLAUDE.md) |
| a CLI subcommand or its output | [`crates/cli/CLAUDE.md`](crates/cli/CLAUDE.md) |
| test fixtures / the corpus | [`crates/corpus/CLAUDE.md`](crates/corpus/CLAUDE.md) |
| cutting a release | [`docs/releasing.md`](docs/releasing.md) |
| **the GUI** | [`gui/CLAUDE.md`](gui/CLAUDE.md) for the frontend rules; [`docs/gui/`](docs/gui/) for the vision, the build spec (items 17–28), the testing layers and the Claude Code plan |
| the generated TypeScript / the Tauri seam | [`gui/CLAUDE.md`](gui/CLAUDE.md) and `scripts/gen-ts.sh` — **never hand-edit `gui/src/lib/api/bindings.ts`** |

Cargo **workspace**, seven crates: `engine` (the library, package
`readingbuddy`), `tui` and `cli` (the two terminal frontends, both linking the
engine directly), `api` (the versioned surface) and `daemon` (its transport),
`gui/src-tauri` (package `readingbuddy-gui` — the third frontend, which links
**`api` and deliberately not the engine**), plus `corpus` (fixture generation,
not shipped).

The GUI's membership in the workspace is doing a job: it puts the Tauri backend
inside `cargo check --workspace`, which is the build where the engine's
`internals` feature is off. Reaching past the API is therefore a CI failure and
not a decision somebody makes at 11pm.

**Two rules about this split, so it does not rot.** A fact that governs the
whole repo belongs in *this* file; a fact about one crate belongs in that
crate's file and nowhere else. And when a crate file grows a second subject
worth its own routing row, it gets a child `CLAUDE.md` rather than a longer
file — which is how `storage/`, `providers/`, `ui/` and `render3d/` got theirs.

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test --workspace` (real suite: ~80 unit tests, all offline)
- Lint: `cargo clippy --workspace --all-targets`
- Run CLI: `cargo run -p readingbuddy-cli -- <subcommand>` (`--help` for the tree; `repl` for interactive mode)
- Run TUI: `cargo run -p readingbuddy-tui` (`--book <selector>` opens straight into a book)
- Run the daemon: `cargo run -p readingbuddyd -- --data-dir .` — listens on `<data-dir>/readingbuddyd.sock`. Poke it with `printf '{"id":1,"request":{"method":"list_books","params":{"limit":5}}}\n' | nc -U ./readingbuddyd.sock`. One JSON object per line, each reply carrying the id it answers.
- Bulk/standardized runner: **`Makefile`** at repo root — `make test` (workspace), `make test-engine` (engine only — CI's macOS leg, and the fast inner loop), `make test-import` (KOReader harness only), `make golden` (regenerate import snapshots), `make synthetic` (regenerate the tier-1 hostile fixtures + goldens), `make goodreads` (regenerate the committed Goodreads export fixture), `make corpus` / `make corpus-check` (tier-2 Gutenberg corpus), `make lint`, `make build-check` (plain `cargo check --workspace` — see below, it is not what `lint` does), `make fmt`, `make dev-db` (a seeded ~220-book library at `dev-data/` worth rendering), `make ts` / `make ts-check` (regenerate and gate the GUI's TypeScript from the API's own DTOs), `make check` / **`make ci`** (both now: fmt + lint + build-check + **ts-check** + whole-workspace test + **web-check** + **routes** — `ci` reproduces the CI gate exactly). Wraps `cargo-nextest` when installed, else falls back to plain `cargo test`, so nextest is never hard-required. `make help` lists targets.
- CI: `.github/workflows/ci.yml` gates PRs — fmt, `clippy --workspace --all-targets --locked -D warnings` (whole workspace, so an engine change cannot silently break CLI/TUI), nextest, and cargo-deny. The test matrix is **asymmetric on purpose**: ubuntu runs `--workspace`, macOS runs `-p readingbuddy`. The TUI suite used to be ungated on the argument that the engine is what CI gates — that only held while every TUI change was written and run by hand on the dev machine, and it contains `every_screen_draws_at_every_size`, the one thing between a layout bug and a panic in the user's pane. macOS stays engine-only because it earns its slot for the two C deps that build differently there (vendored lua54, libsqlite3-sys), not for running the ray tracer twice. `--locked` everywhere: `epub = "=2.1.4"` is pinned exactly *because* a lockfile once picked up a breaking patch release.
  - The check job also runs a plain **`cargo check --workspace --locked`**, which is *not* redundant with the clippy line above it. `--all-targets` resolves dev-dependencies, and `crates/tui`'s switch on the engine's `internals` feature — so under clippy the `Engine::storage()` escape hatch exists for every target in the graph, shipped binaries included. This is the build where it does not, and therefore the only thing standing between item 14's closed seam and a frontend quietly reopening it.
  - **Every Linux job that resolves the whole workspace must `apt-get install` the GTK3/WebKit headers first** (`libwebkit2gtk-4.1-dev` and friends). That is the bill for the GUI's membership in the workspace: `readingbuddy-gui` links Tauri, Tauri renders on Linux through GTK3 + WebKitGTK, and the runner image ships none of it — so `glib-sys` fails its build script and the job dies before touching this repo's own code. Four jobs pay it (`check`, ubuntu `test`, `scheduled`'s `floating-stable`, `release`'s `verify`); the macOS legs do not, since they build `-p readingbuddy` or link WKWebView. Add the step to any new job that says `--workspace`.
  - A fourth job, **`migrations`**, runs on PRs only and refuses a migration that was modified, deleted or renamed (`git diff --diff-filter=MDR` against the merge base). That is CLAUDE.md's "never edit an applied migration" as a check. Its sibling is `migration_versions_are_contiguous_from_one` in `tests/migrations.rs`: parallel branches are how two threads both claim `0008`, and a duplicate version is not a git conflict — the filenames differ past the number.
  - `scheduled.yml` carries what must not gate a PR: advisories, floating-stable clippy, and **the tier-2 corpus** (`make corpus` with `READINGBUDDY_REQUIRE_FIXTURES=1`, `corpus/epub` cached on the manifest hash). That job is overdue rather than optional — `corpus/generated` is gitignored, so on a runner every test in `tests/corpus.rs` takes its `SKIPPED:` path and asserts nothing, which is exactly how the corpus convergence bug survived item 2 and was found weeks later by a hand-run `make corpus`. Scope `READINGBUDDY_REQUIRE_FIXTURES` to that job only: set globally it fails on `real/` and on `partial_md5.rs`'s opportunistic `personal_data/` checks, both absent by design.
  - A fifth job, **`frontend`**, arrived with the GUI: `pnpm install --frozen-lockfile`, `make web-check`, and `make routes` on Playwright's WebKit. `make ts-check` lives in the **`check`** job instead, since it needs cargo and no node — it regenerates `bindings.ts` into a temp dir and diffs, so a DTO change that skipped `make ts` fails the gate rather than shipping a blank panel in a webview.
  - `fuzz.yml` runs weekly. The toolchain is pinned in `rust-toolchain.toml` and `Cargo.lock` is committed.
- **Shipping it: `release.yml` + `install.sh`.** `git tag v0.2.0 && git push origin v0.2.0` is the whole ritual; `make dist` builds *this* machine's archive the same way, which is how the packaging is checked without a tag. The detail moved to [`docs/releasing.md`](docs/releasing.md).
- **Run the GUI**: `make dev-db`, then `cd gui && READINGBUDDY_DATA_DIR=$PWD/../dev-data pnpm tauri dev`. The data dir **must be absolute**: `cover_path` is stored as `images_dir.join(name)`, so a relative root yields a relative `cover_path` and a webview has no working directory to resolve one against. `pnpm install` in `gui/` once first.
  - GUI-only aids: `make shots` (render every route at three viewports into `gui/tests/shots/`, then *look at the PNGs*), `make routes` (the same suite as a gate — fails on a diff), `make web-check`, `make e2e` (pre-PR only). The visual gate is Playwright's **WebKit**, because `tauri-driver` cannot run on macOS at all and WKWebView is what the app ships inside; see [`docs/gui/testing.md`](docs/gui/testing.md).
- TUI-only aids — `--dump-frame`, `--probe`, `print_layout`, `make bench`, `make perf` — moved to [`crates/tui/CLAUDE.md`](crates/tui/CLAUDE.md).

## Agents, skills and hooks

Four agents, and all four report **failures only** — never a wall of passing
names. Launch them; do not reimplement what they do.

| agent | for |
|---|---|
| `cargo-tester` | fmt + clippy + tests. After touching any Rust, before committing. |
| `web-checker` | svelte-check + tsc + eslint + vitest + build. The `cargo-tester` twin for `gui/`. |
| `screenshot-reviewer` | renders routes and **looks at them** — the only check here that can see. Catches "it renders and it is wrong". |
| `api-surface-auditor` | **before** building a GUI feature: can the API serve it? A gap is an engine item, never a frontend workaround. |

### Running a wave as worktree threads — two rules that cost a session each

- **Make every worker check its base before it writes a line.** `git log
  --oneline -1` and `ls crates/engine/migrations/ | tail -2`, then
  `git reset --hard main` if wrong. Four of six worktrees in the GUI wave were
  created ~80 commits behind `main` with migrations stopping at `0010`; every
  thread caught it *only because it was told to look*, and one would otherwise
  have written a migration into a five-version gap.
- **Two threads appending to one file merge cleanly into nonsense.** Git aligns
  a *shared tail*, so two tests that both end in
  `.dispatch(Request::X { .. }).await` get interleaved around it and deleting
  the conflict markers **compiles** — into two tests asserting something neither
  thread wrote. When both sides only appended, do not resolve in place: rebuild
  the file from each side's own block (`git show <side>:<path>`, sliced past the
  merge-base's line count). Generated files are not merged at all; regenerate
  them. This is "never read a piped report" in a third costume — the cheap
  resolution looks right and is not.
- **A subagent with no `SendMessage` stalls the agent that spawned it.** A
  worker's `cargo-tester` reported its PASS to the *orchestrator* and the worker
  sat completed-but-unfinished until it was relayed by hand. If a thread goes
  quiet after its tests would have finished, that is the first thing to check.
- **A worker cannot gate on `make ci`, so an engine change that breaks the
  frontend passes.** A fresh worktree has no `gui/node_modules`, so `web-check`
  and `routes` print `SKIPPED:` and the worker "passes" them. Gate workers on
  `make fmt lint build-check test ts-check`; the **orchestrator** runs the full
  `make ci` from the main checkout after each merge. The specific trap:
  **`ts-rs` emits a new field as required in TypeScript however
  `#[serde(default)]` the Rust is**, so adding a field to an existing `Request`
  variant breaks `gui/src/lib/api/client.ts` — invisible to the worker, and
  caught only on main. **Prefer adding a new request over changing one**, and
  when you must change one, say so in the report.

Three skills:

| skill | for |
|---|---|
| `new-wave-item` | starting a numbered item — pre-allocate the migration, write the prompt file, then build. Opens a session. |
| `gui-component` | a new Svelte component or route, to one dialect rather than twelve. |
| `wrap-session` | verify → session log → commit. Closes a session. |

Two hooks (`.claude/settings.json`):

- **`SessionStart`** warms a cloud container — toolchain sync, `cargo fetch`,
  workspace build. No-ops on a local machine.
- **`PostToolUse`** on every edit runs the cheapest check for *that file's*
  package and prints what it says. It **never blocks** — a multi-file refactor
  passes through legitimately-broken states — and it is bounded at 45s, because
  a hook that hangs after a one-line edit is worse than no hook. It is a smoke
  alarm; `make check` is the gate.

## Working in a cloud session

`.claude/hooks/session-start.sh` (registered in `.claude/settings.json`) warms a fresh container: `rustup show` to force the pinned-toolchain sync, `cargo fetch --locked`, then `cargo build --workspace --tests`. It no-ops off `CLAUDE_CODE_REMOTE`. Without it the first `cargo` call also pays a toolchain download and a from-scratch build of vendored Lua and libsqlite3.

Two constraints that are worth knowing before you plan work there, because both cost a session to rediscover:

- **The sandbox proxy blocks gutenberg.org and its mirrors** (CONNECT 403), so `make corpus` — the tier-2 fixture build — cannot run in a cloud session at all. A hosted CI runner is the only machine that can build that tier; it is a `scheduled.yml` job for exactly this reason. crates.io works fine. `get.nexte.st` does not, which is why the hook does not install nextest and why the Makefile's degradation to plain `cargo test` matters.
- **The TUI suite is fully headless** and needs no terminal: it renders through ratatui's `TestBackend`. So do `--dump-frame [--dump-png]` and the `--ignored` `print_layout` / `print_lists` aids, which is how a cloud thread shows what a layout change looks like. Only `make bench`, `make bench-box` and `--probe` need a real, active pane.

## Runtime data (all gitignored)

- Data root: `--data-dir` flag or `READINGBUDDY_DATA_DIR` env, default current dir → `database/app.db` (created + migrated on startup), `database/images/`, `database/files/` (owned ebook files, content-addressed), `vault/`.
- Sample `.epub` fixtures in `epubs/` (used by engine unit tests, which skip if absent).

## Conventions

- Every ISBN entering the system goes through `normalize_isbn` — no exceptions.
- Provider failures must degrade (warning), never abort a search/lookup.
- New schema changes = new numbered file in `crates/engine/migrations/`; never edit an applied migration.
- Engine tests use `sqlite::memory:` (Storage caps the pool at 1 connection for in-memory URLs — don't "fix" that). The TUI's `app.rs` tests do the same, and `every_screen_draws_at_every_size` renders every screen from 120x40 down to 1x1 — keep it passing, since a layout panic wrecks the user's tmux pane.

## Engine standards

The engine is held to a stricter bar than the TUI, and CI enforces it. These are
the rules, and the reasons — the reasons are the part that matters.

- **Degradations are typed, not stringly.** A partial failure returns a
  `Diagnostic` (`diagnostic.rs`) carrying the provider/path plus an
  `ErrorClass`, never a pre-formatted `String`. Its `Display` reproduces the old
  CLI text byte-for-byte, so a change there is a change to user-visible output.
  `Diagnostic` deliberately does **not** hold the source `EngineError`: that
  would cost `Clone`/`Eq`, which the TUI's status buffer and the golden harness
  both need. Add to `ErrorClass` instead.
- **`EngineError::Other` is last-resort.** If a caller might branch on it, it
  deserves a variant.
- **Derived facts live here, phrasing does not** (item 17). "The engine does no
  terminal I/O" is right and had been over-read as "the engine does no
  derivation", which made a second frontend a *re-derivation* of the app rather
  than an extension of it. The test is: a `Progress` enum is not terminal I/O;
  `"p.42"` is. So sorting keys, progress arithmetic, author-name parsing
  (`names.rs`), row-state joins and selection predicates are the engine's, and
  pluralisation, wording and layout are a frontend's. `progress.rs` and
  `names.rs` are the pattern to copy; `docs/decisions.md` entry 17 records the
  four things that were deliberately **not** moved and why, which is the half a
  later thread is likeliest to re-open by accident.
- **No silently-skipping tests.** A test that `return`s when its fixture is
  absent is green without asserting anything — `epub.rs` had two of those for
  months. Every skip prints `SKIPPED:` and honours
  `READINGBUDDY_REQUIRE_FIXTURES=1`, which turns it into a failure. The nightly
  job sets it, so a broken fetch cannot masquerade as a passing build.
- **Properties where an invariant exists**, in inline `mod props`. Prefer them
  over more examples when the rule is general (checksums, round-trips,
  partitions, orderings). Scope them honestly: `dedup`'s `prefer` fields are
  order-independent and its `fill` fields are not, so only the former is
  asserted permutation-invariant. Asserting something false and then weakening
  it later is worse than asserting less.
- **No network in tests, ever.** The fan-out is tested with a mock
  `MetadataProvider`; timeouts use `#[tokio::test(start_paused = true)]`, which
  is why `PROVIDER_TIMEOUT` must not become a config knob just to be testable.
  `wiremock` is only for real status codes (404/429/500/truncated).
- **Tracing: the engine emits, frontends subscribe.** The engine must never
  install a subscriber. Two hard rules: nothing that can carry an API key goes
  into a field without `googlebooks::scrub_key`, and **highlight text, note
  bodies and search queries are the user's private reading — never above
  `trace!`**. Assert on `Diagnostic`s, not log output; the one exception is
  `tests/tracing_redaction.rs`.
- **Fixtures are generated, not hand-written** (`cargo run -p corpus`). The
  generator does not depend on `readingbuddy` — reusing the engine's own
  parsing to build its fixtures would bake any bug straight into the goldens.
  Tier 1 (`gen-synthetic`) is committed and covers *shape*; tier 2
  (`gen-corpus`, gitignored, `make corpus`) covers *scale and realism*. Shape
  coverage must never sit behind a download.
  Tier 2 emits **one subtree per sidecar layout** — `corpus/generated/modern/`
  and `.../legacy/` — and that split is load-bearing, not tidiness
  (`GENERATOR_VERSION = 2`). Both encode the *same* highlight set of the same
  books, so side by side they share `doc_props.title` (both match one library
  book) and share `datetime`/`pos0`/`text` (their annotations share an
  `identity_hash`) while carrying *different* payloads — page `3i` against
  `7i`, notes only in the legacy one. Import then refreshes every row toward
  whichever sidecar it read last, for ever: every pass reports `updated`, never
  `skipped`, and idempotency can never be observed. No device produces that (a
  mid-migration file carries both layouts in **one** file, where `annotations`
  wins), so the corpus must not either. `the_whole_corpus_imports_idempotently`
  imports one tree per pass and asserts `updated == 0` as well as the skip
  count.
- **Corpus determinism**: `ChaCha8Rng` only — not `StdRng`, whose algorithm may
  change between `rand` versions. Fixed date epoch, never `now()`. Epubs pinned
  by sha256; output pinned by `corpus.lock.json`.
- **`epub` is pinned `=2.1.4`.** It shipped a breaking metadata API change in a
  *patch* release, so a caret constraint is not safe. It is also GPL-3.0 (see
  `deny.toml`), which makes a distributed binary GPL-3.0 as a whole — replacing
  it with `zip` + `quick-xml` would settle both points.
- **`lopdf` is pinned `=0.44.0` for the same reason, and is MIT.** Item 22's PDF
  metadata reader. The licence gate (`cargo deny check bans licenses sources`)
  was run **before** the crate was chosen, not after — which is what `deny.toml`
  asks for, and it matters more here than anywhere: the engine already links
  GPL-3.0 `epub`, so a second copyleft reader would make that situation worse
  rather than merely unchanged. `pdf` 0.10 (pdf-rs, also MIT) was evaluated and
  lost on weight. The exact pin is `epub`'s lesson applied: this is a *metadata*
  surface, and metadata is the surface `epub` broke in a patch release.
- **Fuzzing needs `-s none`** — ASAN cannot survive mlua's longjmp error
  propagation here; measured, with numbers, in `fuzz/README.md`. Any crash found
  gets minimized into `fuzz/seeds/`, where `tests/fuzz_seeds.rs` replays it on
  stable on every PR. That replay is what makes fuzzing pay off.
- **The panic hook is installed before `setup_terminal`.** `set_hook` chains via
  `take_hook`, so the last hook installed runs first; this ordering gives
  TUI hook → `restore_terminal()` → crash log → default. The hook must never
  take a lock the panicking thread might hold, which is why the crash report
  records the log file's *path* rather than a ring buffer of events.
