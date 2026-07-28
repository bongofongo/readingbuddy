# 2026-07-28 — build item 13, Calibre tiers (i) and (ii)

Picks up from `2026-07-28-pipeline-and-items-8-10.md` and `58ac5ee` on `main`
(items 1–10, migrations `0001`–`0009`). One item, one worktree
(`worktree-item-13-calibre`), one PR.

Item 13 as `docs/spec-11-16.md` specifies it: **(i) `ebook-convert` format
conversion, (ii) `calibredb list --for-machine` library import** — feature
detected, never a hard dependency, and confirmed against a real calibre install
as the spec asks. Tier (iii), device push, stays out of scope.

**No migration and no new crate.** `external_ids(source, external_id, book_id)`
was made general in `0009` for exactly this, and the matcher, `partial_md5` and
`book_tags` were all already here. The only manifest change is tokio's `process`
feature on the engine, which adds `signal-hook-registry` to an existing crate's
feature list and no package to the tree.

## What was built

- **`crates/engine/src/calibre.rs`** — detection, both tiers, the import.
- **`Storage::enrich_book`** and a shared `MERGE_RULES` table in
  `storage/books.rs` (see *the engine correction* below).
- **`EngineConfig::calibre_bin_dir`**, `EngineError::CalibreMissing` /
  `::Calibre`, three `DiagnosticKind` variants.
- **`Engine::calibre()` / `convert_ebook` / `calibre_library` /
  `import_calibre_library`** on the facade.
- **`readingbuddy calibre status | convert | import`**.
- **`crates/engine/tests/calibre.rs`** (10 stories against fake binaries), a
  recorded fixture + README, one new CLI subprocess story, and the subcommand
  golden updated.

## The three traps, all found by running calibre 7.26

Not by reading about it. Each would have shipped green, and each is now a
fixture shape or an assertion.

| What calibre actually does | What the obvious code does | Where it is pinned |
|---|---|---|
| `"authors": "Min Jin Lee & Deborah Smith"` — a **`&`-joined string**, while `tags`, `languages` and `formats` are JSON arrays | Deserializes `authors` as `Vec<String>`, fails on the first book with a serde error nobody can act on | `StringOrList` accepts either shape; `authors_parse_from_a_joined_string_or_a_list` |
| `"pubdate": "0101-01-01T00:00:00+00:00"` on an undated book — calibre's `UNDEFINED_DATE` | Publishes every undated book in **the year 101**. Green tests, wrong shelf | `year_of` filters it; `calibres_undefined_date_is_not_a_publication_year` |
| `calibredb --with-library /a/typo list` **creates a library there** (`metadata.db`, `.calnotes/`) and reports `[]` with exit 0 | A mistyped path is indistinguishable from an empty library, having scribbled a new calibre library on the user's disk on the way past | `library_root` refuses a directory with no `metadata.db` **before the binary runs**; `a_mistyped_library_path_is_refused_before_calibredb_is_run` asserts the refusal happens before the fake tool records an argv |

A fourth observation that changed a decision rather than a parse: `list` reports
`rating` as **0–10 half-stars** while `set_metadata --field rating:N` takes 0–5.

## The engine correction this item forced

`upsert_book`'s third branch — no `isbn_10`, no `isbn_13` — is a **plain
unconditional insert** and ignores `Book::id` entirely. A calibre book matched by
uuid or by file hash carries no ISBN, so the first cut created a second copy of
every such book **on every run**: the report said `matched by calibre uuid`,
`created()` was 0, and the library grew from 3 to 5 anyway. Caught by asserting
the library's *size* rather than the report — the report was telling the truth
about what it matched and nothing about what it inserted.

`Storage::enrich_book` is the fix: the same no-clobber merge, by id. Both
statements are generated from one `MERGE_RULES` table (`excluded.x` in the
upsert, `?n` in the update) and `enrich_merges_a_partial_record_exactly_as_the_upsert_does`
runs the same partial record through both and compares the resulting rows —
because two definitions of "merge a partial record" is how they drift. This is
the same shape as `DEVICE_FIELDS_DIFFER` and `identity_hash_of`: one formula,
both sides through it.

The same trap is why `import_book_from_sidecar` had to key on `device_books`.
It is now the second time it has been hit, and the first time it has been named.

## Decisions

- **Merge with the *provider* no-clobber pattern, not the device's straight
  assignment** — even though `docs/decisions.md` names calibre an origin for
  curated metadata. The pattern is chosen by whether the record is *complete*,
  not by who owns the field. A sidecar is the device's complete state, so a
  missing note means the user deleted it. A `calibredb list` row carries **no
  page count at all** and an empty ISBN more often than not, so assigning it
  straight would blank fields calibre has no opinion about. Asserted:
  `an_existing_book_is_matched_by_isbn_and_never_duplicated` keeps a
  provider-sourced `page_count` across a calibre import.
- **Calibre's rating is not imported.** Structural, not a shortcut: a rating
  lives on a *review*, which anchors to a *reading*, which calibre knows nothing
  about. Importing one means fabricating reading history and then guessing at the
  explicit lookup table `docs/decisions.md` requires ratings to go through. Both
  are worse than leaving it where its origin keeps it. Series is dropped for the
  same shape of reason — no column, and `book_tags` is for shelves.
- **`external_ids` records calibre's uuid only**, never the `goodreads`
  identifier calibre may also carry in `identifiers`. That table **repoints on
  conflict**, so one system minting another's ids would silently redirect a later
  Goodreads import to the wrong book.
- **uuid, never calibre's `id`.** Ids are per-library and reused after a delete,
  and `external_ids` has no library column to tell two libraries' id 4 apart.
- **Detection is `calibre_bin_dir` → `PATH` → known install directories**, and
  the last rung is not optional: macOS's `calibre.app` puts its command line
  tools inside the bundle and adds nothing to `PATH`, so a `PATH`-only probe
  misses a perfectly ordinary install. Looking in the places calibre installs
  itself is *detection*; it is not asking the user to configure anything.
- **Two `Option`s, not one "calibre is installed" flag**, so a half install
  degrades to the half that works.
- **Resolved once, at `Engine::open`.** The spec asks for per-run caching; a
  private field plus `Engine::calibre()` gets it without interior mutability, and
  a method rather than a fourth public field because unpicking `pub storage` /
  `pub config` is most of item 14.
- **`calibre convert` refuses to overwrite** and names `--force`. The output path
  is typed by hand and losing a file is the one outcome with no undo.
- **`calibre status` exists** because feature detection is otherwise invisible:
  absent, the features are simply not there, and the user's only other way to
  find out is to try one and read a refusal.

## Testing, and one thing it took two tries to get honest

Fake binaries, never a real calibre — CI runners do not have it, and a feature
exercised only on the developer's laptop is a feature with no tests. What is
faked is the *program*; process spawn, exit codes, stdout, argv, JSON parsing,
matching, upserting, covers and `external_ids` are all the real path.

**Not a `PATH` edit.** `std::env::set_var` is `unsafe` in edition 2024 and races
every other test in the binary, so the engine suite points
`EngineConfig::calibre_bin_dir` at a tempdir of shell scripts instead. The
`PATH` half — detection finding a tool the ordinary way, which is what every
real user takes — is covered from *outside* the process in
`crates/cli/tests/cli.rs`, where the environment belongs to a child.

Two tests were **deleted for passing for the wrong reason**, both the same
mistake in different clothes: detection legitimately falls through to the real
`PATH` and then to the real install directories, so on this machine an assertion
that observed "calibre is absent" was observing `/opt/homebrew/bin` and
`/Applications/calibre.app`. The negative half of `find_tool` now asserts on
`is_executable` directly, and the CLI's absent-wording assertion moved to
`commands::calibre::availability` as a pure function — where it is deterministic
on every machine, and where the rule it protects (`docs/decisions.md`: never ask
the user to install or configure other software) is the sort that erodes by
helpfulness, one obliging edit from "get it from calibre-ebook.com".

`tests/fixtures/calibre/recorded/library.json` is **recorded from calibre
7.26** — the documented exception to "fixtures are generated", the same one the
Goodreads `recorded/` files are, because `calibredb` output is a recorded
artifact of another system. There is deliberately no `generated/` tier: the
Goodreads generator earns its place covering volume in a format with real
quoting hazards, while a JSON array of the same three shapes four hundred times
would test `serde_json`.

One report field was also corrected for honesty. `files_linked` counted files
*looked at*, so a re-import of an unchanged library announced "1 file
identified" against every book for ever. It now counts only links that are new
— the same rule the Goodreads import applies to a rating it did not rewrite —
and `importing_the_same_library_twice_changes_nothing` asserts all three counts
are zero on the second run *and* non-zero on the first, so the zeroes are the
import being quiet rather than the fields never being set.

## Verification

- `make ci` green (exit 0): fmt, `clippy --workspace --all-targets -D warnings`,
  whole-workspace tests. 216 engine unit + 10 calibre integration + 206 TUI +
  9 CLI subprocess + every other suite.
- `cargo deny check bans licenses sources` ok — the exact set the PR job runs.
  (`check advisories` fails on a yanked `spin 0.9.8` under `sqlx`; that is
  pre-existing on `main`, verified by stashing, and lives on `scheduled.yml`
  rather than the PR gate for precisely this reason.)
- **Confirmed against the real install, calibre 7.26 via homebrew**, as the spec
  asks:
  - `calibre status` → both tools found at `/opt/homebrew/bin`.
  - `calibre import --library ./callib --dry-run` then for real against a
    library built with `calibredb add` of both repo epubs → 2 books created with
    tags, covers and files identified; covers land in `database/images/` as
    `calibre-<uuid>.jpg`.
  - A second run → `nothing new (matched by calibre uuid)` on both, and the
    library stays at 2.
  - `calibre convert pachinko.epub out.txt` → real text out of real
    `ebook-convert` in 1.4s; a second run refused with `overwrite it: --force`.
- One pre-existing break found and fixed on the way: the TUI's `cfg(test)`
  `EngineConfig` literal at `crates/tui/src/app.rs:2574`. It is a struct literal
  rather than `..Default::default()`, so the new field broke the TUI *test*
  target while the normal build stayed green — the widened gate is what surfaced
  it, exactly as it did the byte-budget bug last session.

## Merging with items 11 and 12

Both landed on `main` (#12, #13) while this ran, so the branch was merged rather
than fast-forwarded. Two textual conflicts, both from three threads editing the
same insertion points, and **no semantic one**:

- `crates/engine/Cargo.toml` — tokio's feature list. Resolved to
  `["time", "sync", "process"]`: `sync` is item 11's watcher channel seam,
  `process` is this.
- `CLAUDE.md` — item 12's `files.rs` bullet and this one at the same anchor
  (kept both, in build-number order), and the `crates/cli` paragraph, where item
  11 added `ko watch` to the subcommand list and this added
  `calibre status|convert|import` (merged sentence by sentence rather than
  taking a side).

**`book_files` and the calibre import deliberately do not meet yet.** Item 12
gives readingbuddy content-addressed files it *owns*; a calibre import records
`device_books` identity for calibre's files and copies nothing. That is the
right split for now — calibre owns the file, per the ownership table, and
`docs/ux-positioning.md` still has "copy or reference in place?" open — but it
is the seam where conversion output will eventually need somewhere to live.

Verified on the **merged** tree, not either parent: `make ci` exit 0, and each
thread's signature tests run individually rather than trusting the aggregate —
item 11's 11 `watch::` tests, item 12's 15 `files::`/`book_files::` tests plus
its three workflow stories, item 13's 10 calibre stories, and the `merge_books`
set carrying both `merging_two_books_carries_their_files_across` (12) and
`enrich_merges_a_partial_record_exactly_as_the_upsert_does` (13).

## Deferred

- **Tier (iii), device push** (`calibre-smtp` / `ebook-device`).
  `docs/ux-positioning.md` already calls it probably out of scope.
- **A TUI surface.** The CLI has all three commands; the TUI has none, and
  `calibre_bin_dir` is `None` there. `calibre import` is an onboarding action and
  the menu is where onboarding actions live, so it belongs with a keybind pass
  rather than bolted on here.
- **Series.** Recorded nowhere. It has no column, and folding it into
  `book_tags` would muddy the shelf data that collections are supposed to be
  designed against.
- **Calibre's own `identifiers` beyond ISBN** — deliberate, see above.
- **`ebook-convert` options.** The argv is the two paths and nothing else.
  `--output-profile`, `--embed-all-fonts` and the per-format groups are each a
  decision about somebody's book, and there is no surface yet that asks.
- **Conversion has no home for its output.** It writes where it is told, because
  item 12 (`book_files`, owned content-addressed files) is the parallel thread
  that gives converted output somewhere to live. The two meet there.
