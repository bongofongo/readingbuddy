# 2026-07-28 — planning build items 4–7 and the threads that carry them

No code. Produced `docs/spec-engine-04-07.md`, five paste-ready thread prompts
(`docs/prompts/04`–`07`), and two errata in `docs/decisions.md`'s build order.
Picks up where item 3 (`881f60f`) left off.

## Decisions locked

- **Item 4 is a hard move, no compat mirror.** `current_page`, `finished`,
  `date_started`, `date_finished` leave `books` in migration `0005`.
  User chose this over a dual-write mirror or a table-only first step —
  two sources of truth would have to be unpicked later anyway.
- **`Book` keeps those four fields as read-only projections** of the active
  reading, selected through a `LEFT JOIN` in `BOOK_COLUMNS`. That is what
  leaves `row_to_book`, `render.rs`, `library.rs::progress_tag`,
  `book.rs::progress_text` and both note page auto-anchors untouched. The
  alternative — deleting the fields and updating every consumer — is the same
  feature with a five-times bigger diff.
- **`upsert_book`'s `finished = MAX(excluded.finished, books.finished)` is
  retired, not ported.** It only ever existed because reading state lived on
  `books`; a provider upsert has no business touching it. The tests assert its
  *absence*, or it comes back.
- **Item 5 rescoped.** It was written as "partialMD5 + the device→book mapping
  table"; `device_books` landed with item 1b and sidecars already carry
  `partial_md5_checksum`. The remaining content is computing the hash
  **ourselves over a local file** so our identity agrees with the device's.
  `book_files` stays item 12.
- **Item 6 split** into 6a (engine scan + sidecar cache, migration `0006`) and
  6b (TUI screen). The halves don't depend on each other, so 6a runs beside
  item 4 instead of queueing behind it — that is what makes wave 1 three
  threads wide.
- **Item 7 is engine + CLI only**; its TUI section is a later thread.
- **Reflection and Review are notes with a new `kind`**, not new tables, plus
  `rating_scales` / `rating_map` / `review_ratings` / `citations` beside them.
  The reflection is meant to be the graph hub and `note_links` *is* the graph —
  a parallel table would need its own edges, its own FTS and its own editor
  path, and the backlinks pane would then have to learn two vocabularies.
  `notes.kind = 'final'` is superseded by `'reflection'` (answers Q16).
- **Threads run in local git worktrees, in parallel**, merged by PR. Not
  sequential, not cloud.

## Technical gotchas found while planning

- **The TUI has no task channel.** Every engine call is `.await`ed inline in the
  key handler inside `tokio::select!` (`app.rs:1421-1461`) — no `tokio::spawn`,
  no mpsc anywhere in `crates/tui/src`. A device scan or sync-all would freeze
  the draw loop *and* the 20 fps ticker. The only existing mitigation is the
  deferred-work field: `pending_verify` is set by the handler, the loop draws
  the "verifying…" frame, and the work is drained **after** the redraw
  (`:1454-1458`). Item 6b's sync-all must drain one book per loop iteration with
  a redraw between, which is also what gives it per-book progress.
- **`BookImportStats` is `Debug`-only, not `Clone`** (`koreader.rs:403`), so a
  TUI that buffers per-book rows cannot hold one. `Diagnostic` *is* `Clone`
  deliberately (`diagnostic.rs:31-36`) — item 6a's `DeviceBook` must be too.
- **Migration numbers are a cross-thread resource.** Pre-allocated `0005` T4 /
  `0006` T6a / `0007` T7, and branches must merge in numeric order: sqlx applies
  by filename order but records what it applied, so slipping `0005` in after
  `0006` has already run against a real `database/app.db` is a footgun it will
  not save you from.
- **`0005` is the repo's first destructive migration.** `ALTER TABLE … DROP
  COLUMN` needs SQLite ≥ 3.35 (sqlx's bundled lib is past it), and every prior
  migration was additive. Must be verified against a *copy* of a real
  `database/app.db`, not only `sqlite::memory:`.
- ~~**`util.partialMD5`'s first offset is 256, not 0.**~~ **Wrong, and corrected
  by the implementation (PR #2): the first offset is 0.** `lshift` in `util.lua`
  is LuaJIT's BitOp, which takes its shift count modulo 32, so `lshift(1024, -2)`
  is `lshift(1024, 30)` — `2^40` truncated to 32 bits, which is `0`. It is not an
  arithmetic shift and a negative count is not a right shift. Three checksums
  KOReader itself wrote reproduce from 0 and none from 256, and only a genuinely
  *empty* file hashes to the MD5 of nothing. Struck through rather than deleted:
  this is a handoff document that later threads read, so the correction is worth
  more than a tidy record. See `docs/koreader-format.md` §5. Still true as
  written: the Lua loop breaks on a `nil` read, not a short one, so a partial
  read at EOF **is** hashed.
- **Pinning `partial_md5` with a golden string would be circular.** The two
  tests that actually prove something are the three KOReader-produced checksums
  recorded in `docs/koreader-format.md` §5, and a property that flipping a byte
  outside the twelve windows leaves the hash alone while flipping one inside
  changes it.
- **A device-scan cache must cache the parse, never the verdict.** New /
  Unchanged / Updated depends on the library too (a book deleted here, a link
  made since), so `sidecar_seen` stores only what the *file* said and the state
  is recomputed from the DB each scan.
- `.lua.old` backups sit in 9 of 10 real `.sdr` dirs; `is_sidecar_file`
  (`koreader.rs:515`) excludes them only incidentally, by suffix. Importing one
  would resurrect deleted highlights, so item 6a makes that explicit and tests
  it.
- **`Engine::sidecar_candidates` / `link_sidecar` take a file path, not a `.sdr`
  directory**, though the CLI help for `ko pull` / `ko link`
  (`cli/src/main.rs:167,175`) says either works. Pre-existing, not touched here.
- `koreader::find_sidecars` and `parse_sidecar` are **not re-exported at the
  crate root**, so item 6a exposes `scan_device` / `sync_device` on the `Engine`
  facade rather than making the TUI reach into the module.
- Adding a TUI screen touches four exhaustive matches plus the `MENU` array's
  length literal (`app.rs:56`) and both test sweeps (`:2002-2008`, `:2667`).
  Listed file-by-file in the 6b prompt so a fresh thread doesn't rediscover it.

## Open question answered by fiat (recorded so it isn't an accident)

Q27 in `docs/ux-positioning.md` — "does a reading need to exist before
highlights can import?" — was never answered. Item 4's spec says: an import
**opens a reading when the sidecar carries device state** (`summary` or
`percent_finished`), `source = 'koreader'`, started at the earliest
`ko_datetime` seen; a sidecar with neither opens nothing and its highlights stay
unattributed. One function to change if that's wrong.

## Bug found and fixed — the tier-2 corpus could never converge

Wrap-up verification caught `the_whole_corpus_imports_idempotently` failing:
`re-import did not skip every entry it saw — left: 80, right: 0`. Pre-existing,
introduced by item 2 (`0d91fda`), and **invisible to CI** because
`/corpus/generated` is gitignored so the test takes its `SKIPPED:` path on the
runners. It only fails on a machine where `make corpus` has been run.

- **Cause, and it was the fixture not the engine.** `gen-corpus` emitted
  `<slug>-modern.sdr` and `<slug>-legacy.sdr` side by side in one tree from
  **one** highlight set. Same `doc_props.title` → both match the same seeded
  book; same `datetime`/`pos0`/`text` → same `identity_hash`; but different
  payload — modern page `(i+1)*3`, legacy page `(i+1)*7` (the legacy layout's
  page *is* the `highlight[page]` key), and notes only on the legacy side. So
  each import refreshed every row toward whichever sidecar it read last: all 80
  entries counted `updated`, `skipped` stayed 0, for ever.
- Before item 2, `ON CONFLICT DO NOTHING` swallowed the disagreement and the
  test passed. `refresh_device_fields` did not break idempotency — it made an
  impossible fixture visible.
- **Fix: the generator writes one subtree per layout** (`modern/`, `legacy/`),
  `GENERATOR_VERSION` 1 → 2. A real device carries one layout; a mid-migration
  device carries both in *one file*, where `annotations` wins
  (`koreader.rs:87-94`), never as two sidecars.
- The test now imports one tree per pass, and **also asserts `updated == 0`** on
  re-import — without that, the skip-count sum can be satisfied by rows drifting
  into `updated`, which is exactly how this hid.
- Missing-subtree case goes through `skipped()` (loud, `REQUIRE_FIXTURES`-aware)
  rather than `continue`: a corpus generated before the split has no subtrees,
  and silently covering nothing is the failure mode that rule exists to prevent.
- The modern/legacy differential test is unchanged in intent — it still pairs
  the same slug across the two trees and asserts identical highlight text *and*
  identical titles.
- Recorded in `CLAUDE.md` under *Fixtures are generated, not hand-written*.

## Verification

- `cargo clippy --workspace --all-targets`: clean.
- `cargo test --workspace`: **347 passed, 0 failed, 10 ignored** after the fix
  (was 141/1 failed at first wrap-up attempt, before the corpus was regenerated
  — the earlier count reflects a partial run).
- `cargo run -p corpus -- gen-corpus --seed 42` regenerated 8 sidecars (v2) from
  the 4 epubs present locally; the corpus tests pass against them, 318
  highlights parsed, 4 modern/legacy pairs compared, 159 highlights imported per
  tree.

## Deferred

- No code written. The five prompts are the handoff.
- TUI surface for Reflection/Review (would be 7b).
- Reading the device's `statistics.sqlite3` — `docs/koreader-format.md` never
  inspected its schema, so per-page durations and total read time remain
  unspecified. Wanted eventually for "reading sessions as narrative".
- Provider enrichment on device pull, PDF highlight import (`pos0` is a table,
  not an xpointer), and everything else already under *Out of scope for now*.
