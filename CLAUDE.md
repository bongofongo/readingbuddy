# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**readingbuddy** (formerly "reading_card"/"BookBuddy") is a Rust reading companion: federated book-metadata search (OpenLibrary + Google Books), epub import, KOReader highlight/note import, a markdown note vault with zettelkasten links, flashcard capture, all persisted to local SQLite. Two frontends share the engine: a thin CLI, and a ratatui TUI (designed to live in a tmux pane) whose centrepiece is a ray-traced 3D book rendered in quadrant-block cells. An animated pixel companion is still to come.

See `plan.md` for the original design journal and `~/.claude/plans/` history for the 2026-07 engine redesign rationale.

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test --workspace` (real suite: ~80 unit tests, all offline)
- Lint: `cargo clippy --workspace --all-targets`
- Run CLI: `cargo run -p readingbuddy-cli -- <subcommand>` (`--help` for the tree; `repl` for interactive mode)
- Run TUI: `cargo run -p readingbuddy-tui` (`--book <selector>` opens straight into a book)
- Inspect the 3D renderer without a TTY: `cargo run -p readingbuddy-tui -- --dump-frame 100x30 [--book X] [--pose YAW,PITCH] [--dump-png out.png]`. The PNG paints each cell exactly as the terminal would (glyph + its two colors), so it shows the quantization rather than flattering past it.
- See the composed TUI layouts as text: `cargo test -p readingbuddy-tui -- --ignored --nocapture print_layout`

There is a `cargo-tester` agent configured to run `cargo test` and report only failures — use it before committing or after touching Rust code.

## Architecture

Cargo **workspace**, three crates:

- **`crates/engine`** (lib, package name `readingbuddy`) — the engine. **Zero terminal I/O**: no `println!`, no stdin. All user interaction belongs in frontends. Errors are a typed `thiserror` enum (`EngineError`); `anyhow` is CLI-only.
  - `lib.rs` — `Engine` facade (owns `Storage`, provider list, shared `reqwest::Client`, `EngineConfig`). Frontends call this.
  - `book.rs` — `Book` domain struct + `normalize_isbn`/`isbn10_to_13`. **ISBNs are TEXT**, checksum-validated (i64 storage was a bug: leading zeros, X check digit). Identity: internal `id` canonical, ISBNs unique lookup keys.
  - `storage/` — concrete `Storage` struct over `SqlitePool`. This struct is the deliberate swap boundary (no trait until a second backend exists). `sqlx::migrate!` runs `crates/engine/migrations/` on connect; foreign keys ON per connection. Upserts keep the isbn_10 → isbn_13 → neither `ON CONFLICT` branching with `COALESCE(excluded.x, books.x)` no-clobber merging; `finished` merges with MAX.
  - `providers/` — `MetadataProvider` trait (`async_trait`, boxed): `openlibrary.rs` (search.json + /isbn/{}.json edition lookup with concurrent author-key resolution), `googlebooks.rs` (volumes?q= with `intitle:/inauthor:/inpublisher:`; API key optional via `GOOGLE_BOOKS_API_KEY`, keyless 429s degrade to warnings). Language codes normalized MARC↔BCP-47 in `providers/mod.rs`.
  - `search.rs` — federated fan-out (5s per-provider timeout, failures become warnings), dedup (canonical ISBN-13 key, else fuzzy title+author fingerprint, jaro-winkler), field-wise merge (OL wins isbn/pages, GB wins description/language), pure `rank()` (exact ISBN 1000 ≫ title 40 > author 25 > publisher 10...).
  - `koreader.rs` — parses `.sdr` sidecars by evaluating the Lua chunk in a sandboxed `mlua` VM (`StdLib::NONE`); handles modern `annotations` and legacy `highlight`+`bookmarks` formats. Imports are idempotent via sha256 identity hash. Single-word highlights auto-become flashcard candidates.
  - `notes.rs` — vault of markdown files (`vault/<book-slug>/<timestamp>-<slug>.md`, hand-rolled frontmatter, Obsidian-openable). DB keeps metadata + `note_links` (wikilink edges, dangling targets kept as text and back-resolved) + FTS5 index (ordinary fts5 table as searchable body cache; delete+insert on save).
  - `epub.rs`, `flashcards.rs` (Anki TSV export), `images.rs`.
- **`crates/tui`** (bin `readingbuddy-tui`) — the ratatui frontend. Screens: menu → library list → single-book view. `m` returns to the menu from anywhere; the book view's key bar is always present (`o` expands it from four keys to the full set — never hide it entirely, or the view becomes a dead end).
  - `render3d/` — the book object. **No rasterizer**: the book is one cuboid, so `scene.rs` builds a camera ray per subpixel, pulls it into local space with the transposed rotation, and runs a slab intersection — the winning axis names the face, the other two local coords are the UV. Faces: cover texture / accent spine + back / cream page edges striped along local Z. `Model` (in `mod.rs`) takes the front face's aspect from the cover image (so covers are never stretched) and the thickness from `page_count`. Lighting is key + fill + ambient — the fill exists so the back cover isn't a black hole every time the 360° spin brings it round.
  - The book turns about its own centre, ~63s per revolution. `Pose::default` tips it back (negative pitch) so the cover faces upward and the bottom page edges show; positive pitch would face the cover down instead.
  - `camera_distance` fits by projecting the 8 corners over a full yaw sweep, so scale stays constant through a whole turn rather than breathing in and out. `scene::fill_for(rows)` slides the fill fraction from 0.88 on a small pane down to 0.60 on a big one: this caps the object's *absolute* size instead of letting it track the window. Raising it makes the book too big on a full terminal — that was fixed once already.
  - `blit.rs` quantizes **four** subpixels (2x2) into one cell from the sixteen block glyphs in U+2580..U+259F. A cell holds two colors, so it tries all sixteen splits and keeps the lowest squared error; where a cell straddles the silhouette the split is forced by coverage, which is what buys quarter-cell edges instead of whole-column stair-steps. `None` subpixels emit `Color::Reset`, so the pane keeps the user's terminal background.
  - Block glyphs are the **only** presentation path, deliberately: kitty-graphics/sixel don't survive tmux. The framebuffer is always `cols * 2` x `rows * 2`, and because a cell is twice as tall as it is wide, `primary_ray` takes the *physical* aspect (`cols / (rows * 2)`), not the sample grid's.
  - `Scene` caches the decoded cover (per book + column count) and the last frame (per pose, quantized); the 20fps tick only re-traces when something moved. A 200x55 trace costs ~1.7ms in release and ~30x that in debug — judge performance on a release build. Cover paths are stored relative to the data root, so `Scene` resolves them against `EngineConfig::images_dir`.
  - `ui/mod.rs::book_layout` holds the responsive breakpoints — `Wide` (info panel beside the object), `Compact` (title/author/summary header above it; the intended small square tmux pane), `Bare` (object only). Text uses `Color::Reset`/`DarkGray` and `REVERSED` selection so it reads on light and dark terminals; only accents are hard-coded RGB.
- **`crates/cli`** (bin `readingbuddy`) — clap derive subcommands (`search`, `add`, `epub`, `list`, `show`, `rm`, `progress`, `note`, `notes`, `highlights`, `ko import`, `cards`, `config`, `repl`). All printing/prompting lives here (`prompt.rs`, `render.rs`).
  - Google Books key: precedence `--google-api-key` flag > `GOOGLE_BOOKS_API_KEY` env (clap merges these) > `~/.config/readingbuddy/config.toml` (`config_file.rs`, written mode 600, XDG-aware). `config set google-api-key` uses a hidden rpassword prompt; `--verify` live-checks via `readingbuddy::verify_google_key`. `config` runs WITHOUT opening the engine (must not create `database/` as a side effect). Keys are redacted from all error/warning output via `googlebooks::scrub_key` — keep it that way for any new provider error path.

## Runtime data (all gitignored)

- Data root: `--data-dir` flag or `READINGBUDDY_DATA_DIR` env, default current dir → `database/app.db` (created + migrated on startup), `database/images/`, `vault/`.
- Sample `.epub` fixtures in `epubs/` (used by engine unit tests, which skip if absent).

## Conventions

- Every ISBN entering the system goes through `normalize_isbn` — no exceptions.
- Provider failures must degrade (warning), never abort a search/lookup.
- New schema changes = new numbered file in `crates/engine/migrations/`; never edit an applied migration.
- Engine tests use `sqlite::memory:` (Storage caps the pool at 1 connection for in-memory URLs — don't "fix" that). The TUI's `app.rs` tests do the same, and `every_screen_draws_at_every_size` renders every screen from 120x40 down to 1x1 — keep it passing, since a layout panic wrecks the user's tmux pane.
- Truecolor in tmux needs `set -ga terminal-overrides ",*:RGB"`; without it the object's colors quantize to 256.
