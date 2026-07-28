# Prompt — Item 5: `partialMD5`, computed over our own files

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/engine-partial-md5`).

---

Read `docs/spec-engine-04-07.md` (item 5) and `docs/koreader-format.md` §5
before starting. `CLAUDE.md`'s **Engine standards** section is binding.

**Independent of every other thread.** No migration. Merge whenever it is green.
You are the only thread that touches `Cargo.toml`.

## What this item is — and is not

`docs/decisions.md`'s build order calls item 5 "`partialMD5` + the device→book
mapping table". **The mapping table already exists** — `device_books` landed
with item 1b (`migrations/0003_device_books.sql`), and a sidecar's own
`partial_md5_checksum` is already parsed into `KoSidecar.partial_md5`
(`koreader.rs:52`) and already used as `match_book`'s first branch.

What has never been built is computing the same hash **ourselves, over a local
file**. That is what makes our file identity agree with the device's, and it is
this item's whole content. Owned files and `book_files` remain item 12.

## The function

New `crates/engine/src/partial_md5.rs`:

```rust
pub fn partial_md5(path: &Path) -> Result<String>;
```

> **Corrected after this prompt was executed (PR #2).** It said the first offset
> is 256. It is **0** — see `docs/koreader-format.md` §5, which reproduces three
> device-written checksums from offset 0 and none from 256. The bullets below are
> rewritten; the correction is left visible because this is the prompt a thread
> was actually handed.

`docs/koreader-format.md` §5 quotes `frontend/util.lua:1111-1128`: MD5 over up
to twelve 1024-byte samples at offsets `1024 << (2*i)` for `i = -1..10` — **0,
1 Ki, 4 Ki, 16 Ki, 64 Ki, 256 Ki, 1 Mi, 4 Mi, 16 Mi, 64 Mi, 256 Mi, 1 Gi**.
Output is 32-char lowercase hex.

Three details that will be implemented wrong if you do not hold them in mind:

1. **The first offset is 0, not 256.** `lshift` in `util.lua` is LuaJIT's BitOp:
   32-bit, shift count taken **modulo 32**. `lshift(1024, -2)` is therefore
   `lshift(1024, 30)` = `2^40` truncated to 32 bits = `0`. It is not an
   arithmetic shift and a negative count is not a right shift.
2. **Break on a zero-length read, not a short one.** Lua's `file:read(size)`
   returns the partial string at EOF and that string *is* hashed; the loop ends
   on the *next* iteration when the read returns `nil`. In Rust: read up to 1024
   bytes (looping over short reads), and `break` only when 0 bytes came back.
   A seek past EOF is legal and yields 0 bytes.
3. **Only a genuinely empty file hashes to the MD5 of nothing** —
   `d41d8cd98f00b204e9800998ecf8427e`. Under the 256 misreading every file below
   256 bytes would have, which is the tell that the misreading is wrong.

Write down in the module doc-comment that this is a **sampling hash, not a
content hash**: two files identical at those twelve windows collide. That is
fine for its three jobs — dedup, sidecar↔book matching, and the
`statistics.sqlite3` join — and not fine as a content address, where
`docs/decisions.md` already assigns sha256.

Dependency: `md-5 = "0.10"` — RustCrypto, sibling of the already-vendored `sha2`
(`Cargo.toml:56`), shares the `digest` traits. **Check `deny.toml` accepts its
license before writing code.**

## The hook

`Engine::import_epub` (`lib.rs:202`) hashes the file it imported and records
`device_books(partial_md5, book_id, linked_by = 'auto')` through
`Storage::link_device_book` (`storage/device_books.rs:54`) — the write that
**never repoints**. Do not use `set_device_link` (`:84`): that one is reserved
for a decision the user made by hand, and a scan must never relabel it.

The payoff is concrete: an epub imported here is matched by `MatchMethod::Md5`
the first time its sidecar arrives from the device — no fuzzy title guess, and
no dependence on the sidecar living beside its book (KOReader's
`DocSettings:getSidecarDir` also supports `dir` and `hash` modes, where the
sibling-epub ISBN branch cannot work at all).

Export `partial_md5` from `lib.rs` so item 12 has one implementation to call.

## Tests

- **Against real KOReader output — this is the important one.**
  `docs/koreader-format.md` records three checksums KOReader itself produced:
  `8cb32bca81b36ca0816851073e5661d3` (*To the Lighthouse*),
  `a5b01da92a68bbbb6d88c12483cf3b56` (*1Q84*),
  `25dc3d7e5bd746db64267cff902d3edd` (*Rust for Rustaceans*, PDF). Look for the
  matching files in `epubs/` and the gitignored
  `crates/engine/tests/fixtures/koreader/real/`, and assert we agree. Every
  other test proves we agree with ourselves; only this one proves we agree with
  the device. It must print `SKIPPED:` when a file is absent and honour
  `READINGBUDDY_REQUIRE_FIXTURES=1` — a test that silently returns is green
  without asserting anything.
- **Property — the hash sees only its twelve windows.** Generate a file, flip a
  byte outside every window: hash unchanged. Flip one inside: hash changes.
  This pins the algorithm without pinning it to our own output, which a golden
  string would not.
- Boundary sizes: 0, 255, 256, 257, 1024, exactly-on-a-sample-offset, and one
  file just over 64 Ki so more than one sample participates. Deterministic bytes
  (`ChaCha8Rng` only — never `StdRng`, whose algorithm may change between `rand`
  versions) written to a temp dir. No committed binary fixture.
- Importing the same epub twice writes one `device_books` row, and does not
  repoint an existing `manual` link.

## Constraints

- Engine only. No TUI. A CLI surface only if one is natural.
- No network anywhere, tests or otherwise.
- Typed `Diagnostic`s, never pre-formatted strings. An unreadable file is an
  `EngineError`, not a panic.
- No migration — `device_books` already exists. Do not take a migration number.
- Do not read a >1 GiB file into memory; seek and read 1024 bytes at a time.

## Done when

`make ci` green; the real-file checksums match where the files exist; an epub
imported here is later matched by `md5` rather than `title`. Run the
`cargo-tester` agent before committing.
