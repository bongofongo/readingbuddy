# 2026-07-24 — KOReader import test framework + fixture corpus

Goal: real sample data + a repeatable test framework proving KOReader import is
idempotent (re-import never overwrites/dups, only adds new).

## Decisions locked

- **KOReader only** — no Kindle parser this round. Kindle's `My Clippings.txt`
  is a separate format + a real feature, deferred.
- **Sample data = synthetic corpus + drop-in dir.** No clean upstream fixture
  corpus exists (KOReader repo only documents the `.lua` format), so fixtures
  are hand-synthesized from the known format. `real/` is a gitignored slot for
  the user's own exports.
- **Harness = data-driven integration tests with golden JSON snapshots** (new
  `crates/engine/tests/` dir), not more inline unit tests.
- **Runner = cargo-nextest wrapped by a Makefile**, nextest auto-detected with
  a plain-`cargo test` fallback so it's never hard-required.

## What was built

- Corpus `crates/engine/tests/fixtures/koreader/`:
  - `synthetic/<Title>.sdr/metadata.epub.lua` × 7: Pachinko (modern), The-Trial
    (legacy), Multi-Chapter, Unicode, Empty, Malformed, Unmatched.
  - `expected/*.json` goldens, `manifest.json` (books to seed per fixture).
  - `real/` drop-in (gitignored except README + .gitkeep).
- Harness `crates/engine/tests/koreader_import.rs`, 5 tests: golden match,
  strict idempotency (rows byte-for-byte unchanged), append-only superset
  re-import, non-fatal malformed/unmatched, real-dir idempotency (skips empty).
- Root `Makefile`: test / test-import / golden / lint / fmt / check / help.

## Technical gotchas

- **Golden snapshots must not store warning text** — `koreader::import` warning
  strings embed absolute sidecar paths (`sidecar_path.display()`), non-portable.
  Goldens record `has_warnings: bool` only.
- **Fixtures without a sibling `.epub` match by fuzzy title only** — so the
  harness must `Storage::upsert_book` the expected book (title from
  `doc_props.title`) BEFORE import, or it lands in `unmatched`. That's what
  `manifest.json` drives.
- **Space-less CJK is one "word" → flashcard.** `flashcards::single_word` splits
  on whitespace, so `私はその人を常に先生と呼んでいた` (no spaces) is treated as
  a single-word highlight and becomes a flashcard candidate. Surfaced by the
  Unicode fixture; captured faithfully in its golden. Not fixed (would be an
  engine behavior change) — flagged for later.
- **Integration tests reach only the public crate API** — used
  `readingbuddy::koreader::{import, parse_sidecar, find_sidecars}` +
  `readingbuddy::{Book, Storage}` against `sqlite::memory:`; no `Engine`/
  data-dir needed. `serde_json` added to engine `[dev-dependencies]`.
- **CLI `add` takes an ISBN + network lookup**, not `--title`, so the by-hand
  offline double-import proof isn't practical; the automated
  `reimport_is_strictly_idempotent` test is the authoritative offline proof.
- Idempotency mechanism (pre-existing, now under test):
  `identity_hash = sha256(book_id | ko_datetime | pos0 | text)` +
  `ON CONFLICT(book_id, identity_hash) DO NOTHING` in `storage/highlights.rs`.

## Verification

- `cargo test --workspace`: 44 engine + 5 new harness + 61 tui, 0 fail.
- `make test-import` exercised the nextest→cargo fallback (nextest not installed
  here) — 5/5 green.
- `make golden` (`UPDATE_GOLDEN=1`) regenerated snapshots; goldens inspected by
  hand for correctness.
- clippy `--workspace --all-targets`: clean.

## Deferred

- Kindle `My Clippings.txt` import (parser + fixtures + tests).
- Decide whether space-less-CJK-as-single-word flashcard behavior is a bug.
- Pre-existing rustfmt drift in `crates/engine/src/book.rs` +
  `crates/cli/src/commands/book.rs` (newer rustfmt wants different wrapping) —
  left untouched; unrelated to this session.
