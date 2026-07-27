# 2026-07-27 — engine bulletproofing: diagnostics, tracing, corpus, CI

Ten-phase hardening pass over `crates/engine`, plus the repo's first CI. Started
as a testing iteration; became a bug-fix iteration — the exploration found ten
real defects by reading, and the new tests found two more. Engine tests **49 →
136**, all offline. TUI rendering deliberately untouched except one warning fix.

Work is on branch **`engine-hardening`** (11 commits, one per phase), not main —
the user chose per-phase commits on a branch when asked. **Not merged, not
pushed.**

## Decisions locked

- **Corpus is two tiers.** Tier 1 (`corpus gen-synthetic`) is committed and
  covers *shape*; tier 2 (`gen-corpus`, gitignored) covers *scale and realism*
  from real Gutenberg epubs. User initially chose "generated, not committed",
  then on a follow-up chose to commit the tier-1 sidecars so CI replays them
  offline. Shape coverage must never sit behind a download — otherwise the ISBN
  branch is untested on every fresh checkout.
- **Generator does not depend on `readingbuddy`.** Reusing the engine's own
  `normalize_isbn`/parser to build fixtures would bake any bug straight into the
  goldens. It stays an independent oracle; ISBNs are hardcoded.
- **Typed `Diagnostic` over `Vec<String>`**, user chose this over
  tracing-only. `Display` reproduces the old strings byte-for-byte, so no CLI or
  TUI call site changed and all five goldens passed unchanged — that was the
  evidence the refactor was behaviour-preserving.
- **`Diagnostic` does NOT carry the source `EngineError`.** It would cost
  `Clone`/`PartialEq`/`Eq`, which the TUI's status buffer and the golden harness
  both need, for a capability nothing branches on. `ErrorClass` is the 90%.
- **CI: engine strict + whole-workspace build check.** User's choice. Clippy runs
  `--workspace --all-targets -D warnings` (so an engine API change can't silently
  break CLI/TUI); tests only `-p readingbuddy`. TUI's 177 render tests stay out.
- **`Cargo.lock` committed**, toolchain pinned in `rust-toolchain.toml`.
- **Corpus tests run in CI offline**; the Gutenberg fetch is local-only. User
  explicitly rejected network in CI.

## Bugs found

All pre-existing unless noted. Each has a test that fails without the fix.

- **One unreadable sidecar aborted the entire library import.**
  `read_to_string(&path)?` propagated, while the *parse* failure two lines below
  correctly degraded to a warning. A stray non-UTF-8 byte cost you the other 400
  books.
- **A hole in `annotations` silently truncated the import.**
  `sequence_values::<Table>()` stops at the first missing index, so `{[1],[3],[4]}`
  yielded one highlight and dropped the rest. KOReader produces exactly that
  shape after a sync conflict. `parse_legacy` already did it right (`pairs` +
  sort). Silent data loss on someone's reading notes.
- **No Lua instruction budget.** `StdLib::NONE` removes the stdlib but not
  looping: `return (function() while true do end end)()` in a file off a device
  hung the import forever.
- **`collect_sidecars` followed directory symlinks with no depth cap** — a link
  to an ancestor was unbounded recursion.
- **`lookup_isbn` discarded provider errors entirely** (`Err(_) => {}`): no
  warning, no log, nothing.
- **The TUI dropped every search warning** — took `outcome.results`, ignored
  `outcome.warnings`. Dead provider = short list, no explanation.
- **`frontmatter_and_body` mistook `----` for the closing fence.** A bare
  `find("\n---")` matches a markdown horizontal rule, so part of the note body
  was swallowed into the header — and `update_note_body` then wrote the mangled
  split back to disk.
- **Empty frontmatter (`---\n---`) read as body.** Found by the partition
  property shrinking to a 9-byte input; the closing fence sits on line one where
  a `"\n---"` search cannot see it. Not a regression from the `----` fix.
- **`image_from_url` derived a filename from a remote URL** and joined it onto a
  directory. `Url::parse` normalizes `..` away so it appears safe, but that is
  safety-by-dependency-behaviour; now guarded and tested.
- **A bare ISBN-13 never resolved.** 13 digits parses as an `i64`, so
  `resolve_books` took the id branch, looked up row `9781784161880`, found
  nothing, returned empty. Only *hyphenated* ISBNs worked (they fail to parse as
  an integer and fell through). ISBN is now tried first, and every branch falls
  through on a miss instead of returning early.

## Technical gotchas

Highest-value section. These cost real time.

- **ASAN cannot be used with mlua here.** Vendored Lua propagates errors with
  `longjmp`, which ASAN's interceptors don't survive on aarch64-darwin: the fuzz
  target pinned a core and made *zero* forward progress. `cargo fuzz run -s none`
  → `parse_sidecar` 741,558 execs / 26s, `epub_info` 109,615. Diagnosed only by
  running it; an unverified fuzz setup would have shipped broken. Numbers are in
  `fuzz/README.md`.
- **A fuzz instruction budget must bite under *instrumentation*, not just in
  release.** `LUA_INSTRUCTION_BUDGET` was 50M — 0.12s in a normal build, but
  minutes under sancov, which made the fuzzer useless. Now 5M (~50x headroom over
  the 5000-highlight fixture, which is ~10^5 instructions).
- **`epub` 2.1.5 is a *patch* release with a breaking API change** — metadata map
  values went `String` → `MetadataItem`. Our caret `"2.1.4"` accepted it, so any
  `cargo update` broke the build. Found because the fuzz crate resolves its own
  lockfile. Now pinned `=2.1.4`.
- **`epub` is GPL-3.0**, and the engine links it — a distributed binary is
  GPL-3.0 as a whole regardless of what this repo says. Allowed in `deny.toml`
  with the reasoning written down rather than silently. Replacing it with `zip` +
  `quick-xml` would settle both this and the pin.
- **`#[tokio::test(start_paused = true)]` makes the 5s `PROVIDER_TIMEOUT`
  testable in zero wall time** — tokio auto-advances its clock when all tasks are
  idle with a timer pending, and `join_all` + `timeout` compose with it. This is
  why `PROVIDER_TIMEOUT` must NOT become a config knob just to be testable.
  Needs the `test-util` tokio feature.
- **`publish = false` in `[workspace.package]` is NOT inherited.** Each member
  needs `publish.workspace = true`, or `cargo-deny`'s `private.ignore` still
  reports them unlicensed. Same for `rust-version`.
- **`cargo-deny`'s `allow-wildcard-paths` doesn't apply to "public" crates** —
  intra-workspace `{ path = "../engine" }` reads as a wildcard until the crate is
  marked unpublished.
- **GitHub Actions does not support YAML anchors.** A `&docs`/`*docs` alias for
  a shared `paths-ignore` fails to parse; duplicate the list.
- **Use `paths-ignore`, not a `paths` allowlist.** With branch protection an
  allowlist leaves required checks stuck "pending" forever on a docs-only PR.
- **`prop_assume` dies on the reject cap** if the condition is rare. The ISBN-13
  five-gap transposition case rejected ~90% and aborted with "Too many global
  rejects" after 91 successes, never reaching the assertion. *Construct* the
  qualifying case instead of filtering for it.
- **ISBN transposition detection is asymmetric.** Mod-11 ISBN-10 catches every
  adjacent transposition; ISBN-13's 1-3-1-3 weighting provably **cannot** catch
  one of digits differing by 5 (the sum moves 2×5 ≡ 0 mod 10). Both directions
  are asserted, the second specifically so nobody later "strengthens" the first
  into something false.
- **`rand`'s `StdRng` algorithm may change between versions** — the classic way a
  "reproducible" corpus quietly stops being reproducible. Use `ChaCha8Rng`.
- **Gutenberg's `pg{id}.epub3.images` cache path now 404s.**
  `https://www.gutenberg.org/cache/epub/{id}/pg{id}.epub` is the working direct
  URL; `/ebooks/{id}.epub3.images` and `.epub.noimages` redirect and work as
  fallbacks.
- **Fixture discovery is a non-recursive `read_dir`**, which is load-bearing: it
  is why `synthetic/variants/Pachinko-Superset.sdr` is invisible to the golden
  loop and needs no golden of its own.
- **`Lua::set_hook` returns `()` in mlua 0.10**, not `Result`.
- **`EpubDoc::spine` is `Vec<SpineItem>`** (use `.idref`), and `BytesText` has
  `.unescape()`, not `.decode()`, in quick-xml 0.37.
- **Slug uniqueness needs the id.** Every non-Latin title (Japanese, Chinese,
  Russian) slugifies to nothing, so corpus dirs are `{gutenberg_id}-{slug}-{kind}`.
- **Both corpus layouts must share ONE highlight set.** Drawing separately gave
  the modern and legacy sidecars different passages and made the differential
  test meaningless — caught by the test itself.

## Verification

- `make ci` green: fmt, `clippy --workspace --all-targets -D warnings`, engine
  tests. **136 engine tests**, 0 failures, 1 ignored (the scale check).
- **Behaviour preservation**: all 5 import goldens passed unchanged through the
  diagnostics refactor; the only golden delta was the deliberate `matched_by`
  field.
- **CLI output unchanged**: default run emits **0 bytes** to stderr; `-vv` shows
  per-provider spans; stdout stays pipeable.
- **Crash hook**: `--panic-now` writes a symbolized report to
  `<data_root>/logs/crash.log` and echoes the path, after the terminal is
  restored.
- **Corpus generator determinism**: same seed → byte-identical `corpus.lock.json`
  sha; different seed → different. Validated against 4 real books (English,
  French, Japanese): 160 highlights, all 4 corpus tests green.
- **`Gen-Isbn-Match` reports `matched_by: "isbn"`** — the sibling-epub branch had
  never executed in a test before.
- **Scale**: 5000 highlights import in 136ms (~37k/s), release.
- **Fuzz**: both targets run clean, 0 crashes; 27 seeds replay on stable in 110ms.
- **`openssl`/`native-tls` confirmed gone** from the lock tree after the rustls
  switch.

## Deferred

- **Branch not merged or pushed.** `engine-hardening`, 11 commits. Merging to
  main is the user's call.
- **Corpus manifest has 30 books but only 4 fetched/checksummed.** Deliberate —
  fetching all 30 would hammer a volunteer-run site for no benefit here. Run
  `scripts/fetch-corpus.sh --record` once and commit the manifest to pin the rest.
- **`epub` crate not replaced.** Both the GPL-3.0 exposure and the patch-release
  instability point at dropping it for `zip` + `quick-xml`; out of scope here.
- **`identity_hash`'s `|`-delimiter collision left as-is.** `(datetime="a|b",
  pos0="c")` and `(datetime="a", pos0="b|c")` collide. Fixing it rewrites every
  hash in every existing user database, so the next import re-inserts everything
  — churn on real data far exceeding a collision that needs a `|` inside a
  KOReader datetime or an xpointer. Documented as known-accepted.
- **`dry_run` flashcard counts still differ from the real path** (no per-book word
  dedup). The corpus emits repeated single-word highlights so this is visible in
  a golden rather than a footnote, but the discrepancy itself is unfixed.
- **CLI/TUI have no tests of their own** — out of scope for an engine iteration.
