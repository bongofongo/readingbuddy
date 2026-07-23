# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**readingbuddy** (formerly "reading_card"/"BookBuddy") is a Rust reading companion: federated book-metadata search (OpenLibrary + Google Books), epub import, KOReader highlight/note import, a markdown note vault with zettelkasten links, flashcard capture, all persisted to local SQLite. A TUI (ratatui, designed to live in a tmux pane, with an animated pixel companion) is the planned frontend; today the frontend is a thin CLI.

See `plan.md` for the original design journal and `~/.claude/plans/` history for the 2026-07 engine redesign rationale.

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test --workspace` (real suite: ~40 unit tests, all offline)
- Lint: `cargo clippy --workspace --all-targets`
- Run CLI: `cargo run -p readingbuddy-cli -- <subcommand>` (`--help` for the tree; `repl` for interactive mode)

There is a `cargo-tester` agent configured to run `cargo test` and report only failures — use it before committing or after touching Rust code.

## Architecture

Cargo **workspace**, two crates:

- **`crates/engine`** (lib, package name `readingbuddy`) — the engine. **Zero terminal I/O**: no `println!`, no stdin. All user interaction belongs in frontends. Errors are a typed `thiserror` enum (`EngineError`); `anyhow` is CLI-only.
  - `lib.rs` — `Engine` facade (owns `Storage`, provider list, shared `reqwest::Client`, `EngineConfig`). Frontends call this.
  - `book.rs` — `Book` domain struct + `normalize_isbn`/`isbn10_to_13`. **ISBNs are TEXT**, checksum-validated (i64 storage was a bug: leading zeros, X check digit). Identity: internal `id` canonical, ISBNs unique lookup keys.
  - `storage/` — concrete `Storage` struct over `SqlitePool`. This struct is the deliberate swap boundary (no trait until a second backend exists). `sqlx::migrate!` runs `crates/engine/migrations/` on connect; foreign keys ON per connection. Upserts keep the isbn_10 → isbn_13 → neither `ON CONFLICT` branching with `COALESCE(excluded.x, books.x)` no-clobber merging; `finished` merges with MAX.
  - `providers/` — `MetadataProvider` trait (`async_trait`, boxed): `openlibrary.rs` (search.json + /isbn/{}.json edition lookup with concurrent author-key resolution), `googlebooks.rs` (volumes?q= with `intitle:/inauthor:/inpublisher:`; API key optional via `GOOGLE_BOOKS_API_KEY`, keyless 429s degrade to warnings). Language codes normalized MARC↔BCP-47 in `providers/mod.rs`.
  - `search.rs` — federated fan-out (5s per-provider timeout, failures become warnings), dedup (canonical ISBN-13 key, else fuzzy title+author fingerprint, jaro-winkler), field-wise merge (OL wins isbn/pages, GB wins description/language), pure `rank()` (exact ISBN 1000 ≫ title 40 > author 25 > publisher 10...).
  - `koreader.rs` — parses `.sdr` sidecars by evaluating the Lua chunk in a sandboxed `mlua` VM (`StdLib::NONE`); handles modern `annotations` and legacy `highlight`+`bookmarks` formats. Imports are idempotent via sha256 identity hash. Single-word highlights auto-become flashcard candidates.
  - `notes.rs` — vault of markdown files (`vault/<book-slug>/<timestamp>-<slug>.md`, hand-rolled frontmatter, Obsidian-openable). DB keeps metadata + `note_links` (wikilink edges, dangling targets kept as text and back-resolved) + FTS5 index (ordinary fts5 table as searchable body cache; delete+insert on save).
  - `epub.rs`, `flashcards.rs` (Anki TSV export), `images.rs`.
- **`crates/cli`** (bin `readingbuddy`) — clap derive subcommands (`search`, `add`, `epub`, `list`, `show`, `rm`, `progress`, `note`, `notes`, `highlights`, `ko import`, `cards`, `config`, `repl`). All printing/prompting lives here (`prompt.rs`, `render.rs`).
  - Google Books key: precedence `--google-api-key` flag > `GOOGLE_BOOKS_API_KEY` env (clap merges these) > `~/.config/readingbuddy/config.toml` (`config_file.rs`, written mode 600, XDG-aware). `config set google-api-key` uses a hidden rpassword prompt; `--verify` live-checks via `readingbuddy::verify_google_key`. `config` runs WITHOUT opening the engine (must not create `database/` as a side effect). Keys are redacted from all error/warning output via `googlebooks::scrub_key` — keep it that way for any new provider error path.

## Runtime data (all gitignored)

- Data root: `--data-dir` flag or `READINGBUDDY_DATA_DIR` env, default current dir → `database/app.db` (created + migrated on startup), `database/images/`, `vault/`.
- Sample `.epub` fixtures in `epubs/` (used by engine unit tests, which skip if absent).

## Conventions

- Every ISBN entering the system goes through `normalize_isbn` — no exceptions.
- Provider failures must degrade (warning), never abort a search/lookup.
- New schema changes = new numbered file in `crates/engine/migrations/`; never edit an applied migration.
- Engine tests use `sqlite::memory:` (Storage caps the pool at 1 connection for in-memory URLs — don't "fix" that).
