---
title: GUI vision, the CLAUDE.md split, and a bespoke agent/skill/hook ecosystem
date: 2026-08-04
---

# Session log

Planning session. **No Rust changed.** Output is `docs/gui/` (four documents),
the `CLAUDE.md` split into a routed hierarchy, and the `.claude/` tooling for
the GUI wave.

## Decisions locked

- **Transport: the GUI links `readingbuddy-api` in-process, behind a swappable
  client trait — not the daemon.** The daemon does not fix the two-writer
  problem while the TUI keeps its direct engine link; covers and paths cross as
  filesystem strings so it buys no remoteness; it has no push channel; and iOS
  forces the in-process path to exist regardless. It arrives with item 15.
- **Rewards: tracking is total, presentation is retrospective.** The rule —
  *the app tells you what you did, it never tells you what you have left.*
  **No goals; decided against, not deferred.** No live streak counters, no
  number on a home surface. Three registers: finishing, thinking, returning.
- **The chain is moment → card → shelf**, and the card is **per reading, not per
  book**, so a reread mints a second one beside the first.
- **TUI and GUI are peers, developed independently.** Constraint on the
  *engine*: anything both need moves below both (spec item 17). The TUI migrates
  on its own schedule or never.
- **"Shelf view" leaves *Out of scope for now*.** The original ruling was against
  a shelf grouped by *collection*; collections stay deferred and this shelf
  groups by nothing. "Author/corpus view" partially moves with it; graph view and
  orphan queue stay out.
- **New scope: a local-reading source.** Attach a PDF, type progress, take notes.
  The first *reading state* readingbuddy originates (it already originates the
  vault). **No embedded PDF viewer** — explicitly out of scope.
- **KOReader `statistics.sqlite3` waits for item 15** (the plugin wave).
  `reading_events` (item 21) is built now as a source-agnostic log so it arrives
  as one more filler and changes no query and no pixel.
- **Framework: Tauri + Svelte, re-confirmed** after weighing iced, Dioxus and
  Leptos. Layout `gui/` + `gui/src-tauri/` (package `readingbuddy-gui`), **pnpm**,
  post-edit hook warns rather than blocks.

## Bugs found (all pre-existing)

- **Cover filename collision is real, not theoretical.** `filename_from_url`
  (`images.rs:17`) names the file from the URL's last path segment and falls back
  to the literal `"cover.jpg"`; `image_from_url` writes `images_dir.join(fname)`.
  Two books whose provider URLs end in the same segment overwrite each other, and
  the fallback makes it reachable. The path-traversal guard beside it is careful;
  the collision is unguarded. → spec item 20, migration `0011`.
- **The engine stores no cover dimensions or accent colour.** `Cover.aspect` and
  `accent_from_border` are computed by *decoding the image* in the TUI
  (`texture.rs:67-73`, `:91`); no migration defines a width/height/accent column.
  A shelf of 300 spines would decode 300 images per render. → item 20.
- **`crates/cli/src/render.rs:56` has no `total > 0` guard**, so `page_count = 0`
  renders `[12/0]`. *Not* a divide-by-zero — it never computes a percentage; the
  two TUI sites that do divide both guard correctly.
- **`refresh_note_from_disk` is fully wired and never called automatically.** It
  has an API facade method (`api/src/lib.rs:365`), a request variant, and a test
  — but no frontend issues it and nothing watches the vault, so an Obsidian edit
  leaves `notes_fts` stale indefinitely. → item 24, and it is a *watcher* job,
  not a wiring job.
- `CLAUDE.md` said "five crates". There are six.

## Technical gotchas

- **`tauri-driver` does not support macOS.** Verbatim from the v2 docs: *"only
  Windows and Linux are supported on desktop, as macOS has no WKWebView driver
  tool available"*. macOS E2E goes through `tauri-plugin-wdio-webdriver` (an
  embedded provider, debug builds only) or a third-party driver. Plan E2E as a
  **Linux CI job**.
- **iced's `text_editor` has no undo/redo.** The `Edit` enum is exactly `Insert`,
  `Paste`, `Enter`, `Indent`, `Unindent`, `Backspace`, `Delete`. IME/input-method
  support *did* land in 0.14. Accessibility (issue #552) has been open since
  October 2020 with no implementation.
- **Dioxus desktop is not the middle ground** it looks like. Their own platform
  guide: *"browser APIs are not available, so rendering WebGL, Canvas, etc is not
  as easy as the Web"*. It trades Tauri's typed IPC seam for an untyped `eval`
  seam and lands the cost on the shelf, which is the centrepiece.
- **Tauri's `mockIPC` matches on command *strings*.** A renamed command breaks the
  app while every test mocking it keeps passing. Prefer a typed client seam and
  keep `mockIPC` for one file that tests the client itself.
- **`readings.source` is plain `TEXT NOT NULL DEFAULT 'manual'`** with the
  vocabulary in a *comment*, not a `CHECK` (`0005_readings.sql:19`). So
  `source = 'local'` needs **no migration**.
- **`epub::extract_cover` exists and IS wired** (`epub.rs:41` ← `lib.rs:484`).
  Recorded so it is not re-investigated as a gap.
- **`BookSort::Progress` orders by a computed value across a `LEFT JOIN`**
  (`storage/books.rs:323`) — not a stable keyset cursor key. Item 18 has to
  decide this explicitly.
- **The TUI's 200-row fetch is deliberate and documented at the call site**
  (`app.rs:1006`): a SQL `LIMIT` would make the sort key decide *which* 200 books
  are on screen, so `s` would swap the list's contents rather than reorder them.
  Naive pagination reintroduces exactly that bug.
- **Svelte 4 idioms still compile under Svelte 5.** Silent dialect divergence,
  and the likeliest defect in agent-written frontend code since the training mass
  is Svelte 4. Hence the eslint rules in `gui/CLAUDE.md`.
- **`device_bash` cannot unlink files.** A `git status` run through it left a
  stale `.git/index.lock` that blocked git entirely; it had to be `mv`'d aside.
  Worth knowing before running git through that path again.

## Verification

- **The `CLAUDE.md` split was verified line-by-line.** All 242 lines of the
  original accounted for across the new files. 8 non-matches, each confirmed: 3
  replaced headings (`## Architecture`, `## Releasing`, the "five crates" line)
  and 5 stripped `- **crates/x** —` list prefixes whose bodies survived. **One
  genuine loss was caught and fixed** — the truecolor/tmux `terminal-overrides`
  note, now in `crates/tui/src/render3d/CLAUDE.md`.
- Every markdown link in every `CLAUDE.md` resolves (checked mechanically).
- Root `CLAUDE.md`: **109,569 → 17,316 bytes** (~27,400 → ~4,300 tokens).
- `Makefile` parses; `make -n check` and `make -n help` OK; `web-check`, `shots`
  and `e2e` each print a stated skip with no `gui/` present.
- Hook: `bash -n` clean, `settings.json` valid JSON, smoke-tested on a real
  payload, and package routing verified across 12 representative paths.
- **NOT verified: `cargo test` / `cargo clippy`.** No Rust toolchain in the
  sandbox this session ran from, and its shell caps at 45s — a workspace test
  cannot run there. **No Rust source was changed**, so the exposure is the
  `Makefile` edit alone, which was dry-run. Run `make ci` before pushing.

## Deferred

- **`docs/decisions.md` still needs editing**: the "Shelf view" reversal
  (`:230`), and the settled block at the bottom of `docs/gui/gui-vision.md`
  folded in. Both currently live only in `docs/gui/`.
- `gui/` scaffold — spec item 25. `gui/CLAUDE.md` exists ahead of it on purpose.
- The eslint config is documented as a block in `gui/CLAUDE.md` rather than a
  file, since it depends on packages not yet installed.
- Whether an abandoned reading appears on the shelf — decided against a real
  shelf, not in advance.
