# Prompt — Item 6a: scanning a mounted device (engine half)

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/engine-device-scan`).

---

Read `docs/spec-engine-04-07.md` (item 6a) and `docs/decisions.md` (the
*Device linking* section) before starting. `CLAUDE.md`'s **Engine standards**
section is binding.

**Runs in parallel with items 4 and 5. Blocks item 6b** (the TUI screen). Owns
migration **`0006`**, and must merge *after* `0005` — branches merge in numeric
order, because an out-of-order apply against a real `database/app.db` is a
footgun sqlx will not save you from.

## The problem

`koreader::import` walks a tree and `dry_run` already previews it, so a scan
looks nearly free. It is not, for two reasons:

1. `dry_run` **evaluates every sidecar's Lua in mlua** and then runs book
   matching. On a device with several hundred books that is several hundred VM
   evaluations *every time the screen opens*.
2. Its output is an `ImportReport`, which answers "what would import" — not
   "what is the state of each book on this device", which is what the screen
   shows.

## Where the code goes

New `crates/engine/src/device.rs`. **Do not grow `koreader.rs`** — it is 1758
lines, and item 4 is editing its import path in a parallel worktree. This module
consumes `koreader::{find_sidecars, parse_sidecar, match_candidates}`, all
already public, plus `Engine::pull_book_from_sidecar` for the sync path.

```rust
pub enum DeviceState {
    New { candidates: Vec<MatchCandidate> },
    Unchanged,
    Updated { new_highlights: usize, refreshed: usize },
    Unreadable(Diagnostic),
}

#[derive(Clone)]                 // the TUI buffers these, and
pub struct DeviceBook {          // BookImportStats is Debug-only
    pub path: PathBuf,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub partial_md5: Option<String>,
    pub book_id: Option<i64>,
    pub matched_by: Option<MatchMethod>,
    pub state: DeviceState,
    pub ko_percent: Option<f64>,
    pub ko_status: Option<KoStatus>,
}

pub async fn scan_device(storage: &Storage, root: &Path) -> Result<DeviceScan>;
pub async fn sync_device(storage: &Storage, paths: &[PathBuf]) -> Result<Vec<PullReport>>;
pub fn candidate_mounts() -> Vec<PathBuf>;    // /Volumes/*, /run/media/$USER/*, /media/$USER/*
pub fn is_koreader_mount(p: &Path) -> bool;   // `koreader/` present with its expected contents
```

Expose both async functions on the `Engine` facade (`lib.rs`), so the TUI never
reaches into the `koreader` module directly — today `find_sidecars` and
`parse_sidecar` are not even re-exported at the crate root, and item 6b should
not be the reason they become so.

The four states are the ones `docs/decisions.md` specifies. **`Unreadable`
carries the existing vocabulary** — `DiagnosticKind::SidecarUnreadable` /
`SidecarUnparsable` (`diagnostic.rs:84`) — rather than inventing a parallel
error taxonomy.

`is_koreader_mount` is not cosmetic. The KOReader-plugin decision requires
refusing to write to an unrecognised volume, and this is the function that will
gate that later. Get it right once, here, while it is still read-only.

## The pre-filter cache — migration `0006_sidecar_cache.sql`

```sql
CREATE TABLE sidecar_seen (
    path        TEXT PRIMARY KEY,   -- absolute path of the metadata.*.lua
    size        INTEGER NOT NULL,
    mtime       INTEGER NOT NULL,
    partial_md5 TEXT,
    entry_count INTEGER NOT NULL,
    last_seen   INTEGER NOT NULL
);
```

`stat` first. When `(size, mtime)` match the cached row, skip the parse
entirely.

**Cache the parse, never the verdict.** A book deleted here, or linked since,
changes a sidecar's state without touching the file — so the row stores only
what the *file* said, and `New` / `Unchanged` / `Updated` is recomputed from the
database on every scan. That distinction is the whole reason the cache cannot
start lying after an unrelated library edit.

`.lua.old` backups exist in 9 of 10 real `.sdr` directories
(`docsettings.lua:340` writes one on every flush) and must never be scanned or
cached — they are a previous state of the same annotations, and importing one
would resurrect highlights the user deleted. `is_sidecar_file`
(`koreader.rs:515`) already excludes them by suffix, incidentally and correctly.
Make it explicit and test it.

## CLI

- `rb ko scan [path]` — one line per book with its state. Plain builders in the
  style of `crates/cli/src/render.rs`; warnings to stderr, as `commands/ko.rs:8`
  already does.
- `rb ko sync <path> [--all | --book …]` — drives `sync_device`, one report per
  book.

Keep the existing tone of `commands/ko.rs`: when something is unmatched, print
the next move (`ko pull` / `ko link`) rather than a bare count. Unmatched is a
decision, not a dead end.

## Tests

All offline, `sqlite::memory:`, against generated fixtures — new synthetic cases
via `crates/corpus/src/synthetic.rs` (`make synthetic`) if the existing corpus
cannot produce a state.

- Each of the four states is produced by a fixture that deserves it.
- **A second scan of an unmodified tree parses zero sidecars.** Assert the
  count — a parse counter, or a `tracing` span — never a timing.
- Touching a sidecar's mtime forces a re-parse; changing its bytes moves the
  state to `Updated`.
- A `.lua.old` sibling is never scanned and never cached.
- `is_koreader_mount` rejects an ordinary directory and a look-alike volume.
- A sidecar that fails to parse yields `Unreadable` with the existing
  diagnostic, and does not abort the scan.

## Constraints

- Engine + CLI. **No TUI** — that is item 6b, a separate thread.
- Stay out of `koreader.rs`'s body; item 4 is editing it in parallel. Adding a
  `pub use` or a re-export there is fine, rewriting `import_into` is not.
- No network anywhere, tests or otherwise.
- Typed `Diagnostic`s, never pre-formatted strings.
- Never edit an applied migration. `0006` is yours; merge after `0005`.
- Highlight text and note bodies never go above `trace!`.

## Done when

`make ci` green; `rb ko scan` against the fixture tree shows all four states;
the second scan parses nothing; `rb ko sync` pulls a selection with one report
per book. Run the `cargo-tester` agent before committing.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
