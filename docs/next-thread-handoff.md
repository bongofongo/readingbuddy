---
title: Handoff — where the engine stands after items 29–32, and what to do next
date: 2026-08-05
source: sessions/2026-08-05-items-29-32-engine-wave.md for how it was built;
        docs/spec-engine-29-32.md for the wave itself
---

# Handoff

Paste this into a fresh session at the repo root. It assumes nothing about the
previous conversation.

## Where things stand

Green: `make ci` exit 0 (engine lib 323, TUI 291), `cargo deny check bans
licenses sources` ok. Working tree clean, no unpushed remote (nothing is pushed;
this repo commits straight to `main`).

**Updated 2026-08-05**: options B and C below are done — the wave is surfaced
and the `MERGE_RULES` debt is closed. **A is what is left.**

The last wave — items **29–32 plus 21** — is about what the engine *keeps*
rather than what it acquires. What landed:

| item | migration | what exists now |
|---|---|---|
| 21 | `0011` | `reading_events(book_id, day, minutes, pages, source, confidence)` + three device-free fillers + period aggregates |
| 29 | `0012` | `field_provenance(book_id, field, source, fetched_at)`, generated from `MERGE_RULES` |
| 31 | — | `Engine::import_device_statistics` — KOReader `statistics.sqlite3` → measured minutes |
| 30 | — | `Engine::enrich_book_from_providers`, `rb enrich`, `rb set` |
| 32 | `0013` | `books.subjects` / `series` / `series_index`, `Engine::table_of_contents` |

Read `docs/decisions.md`'s build order (entries 21, 29, 30, 31, 32) before
touching any of it — every correction the wave forced is recorded there, and
several are load-bearing.

## The next migration number is `0014`, and it belongs to GUI item 20

`0011`–`0013` are taken. **`0014` → item 20** (cover dimensions + accent),
**`0015` → item 23** (moments). Those were reshuffled from an earlier
pre-allocation; `docs/gui/spec-gui-17-28.md` and the `new-wave-item` skill both
record it.

`migration_versions_are_contiguous_from_one` fails on a **gap** as well as a
duplicate. A branch holding `0015` before `0014` merges is red until its
predecessor lands — expected; rebase, never renumber.

## What is left

### A. The GUI wave, items 17–28 — **the remaining path, and now the only one**

`docs/gui/spec-gui-17-28.md` is written and unstarted except for 21. **Item 17
is first and is the highest-value item in that wave** — the derived-facts layer,
without which every frontend re-derives the app. Items 17, 18, 19, 20, 22 and 24
share no files and can run concurrently; **26, 27 and 28 must not** (three agents
there produce three dialects of one app).

Two things that wave should now know, which its spec predates:

- Item 21 is **done**, so items 23 and 28 are unblocked, and item 22's
  "feed `reading_events` on each progress update" writes into a real table with a
  real merge (`(book_id, day, source)`, no-clobber, `confidence` ratchets to
  `measured` and never back — do not delete-then-insert scoped by source).
- Item 20 rewrites cover storage. `Engine::enrich_book_from_providers` now calls
  `download_cover`, so item 20 must not break that path — and note the bug this
  wave fixed there: `download_cover` used to write back through `upsert_book`,
  whose no-ISBN branch is an unconditional insert ignoring `Book::id`, which
  produced a *second book row* instead of a cover.

### B. Surface the wave — **done**, 2026-08-05

Everything from items 21, 29, 30, 31 and 32 now has a CLI and an API. `rb toc`,
`rb activity [--book|--days|--refill]`, `rb ko stats`, `rb set
--subject/--series/--series-index`; on the API, `table_of_contents`,
`import_device_statistics`, the activity log's four, and item 30's
`enrich_book`/`set_book_fields`/`field_provenance` — which the original version
of this list missed, and which were engine-and-CLI-only. `subjects`/`series`/
`series_index` cross in `BookDto`, so timestamps are the only field the DTO
round trip deliberately drops. No migration; nothing new the engine could not
already do.

### C. The `MERGE_RULES` debt — **done**, same pass

`search::merge_into` is generated. `Rule::federated` is the sixth thing the
table produces and it makes the arrangement total: a new book column does not
compile without saying how it merges in a federated search. See
`docs/decisions.md` entry 33 and `crates/engine/src/storage/CLAUDE.md`.

## Traps that will bite a thread touching engine internals

- **`MERGE_RULES` (`crates/engine/src/storage/books.rs`) generates six things**:
  the upsert's `ON CONFLICT`, `enrich_book`'s `UPDATE`, `merge_books`' `dst`-wins
  fill, the `field_provenance` stamps, `Rule::show`, and — since the surfacing
  pass — `Rule::federated`, which is `search::merge_provider_record`. Plus `PROBES` in
  `tests_support`, whose column list is *asserted* equal to `MERGE_RULES`' in
  order — a new column fails those sweeps with a message rather than going
  quietly uncovered. That is deliberate; extend it.
- **`upsert_book`/`enrich_book`/`fill_book` take a required `Option<Source>`.** A
  test seed passes `None` (it has no origin to name). `save_book` stamps `None`
  on purpose — `search::merge_provider_books` discards which provider supplied
  each field before it arrives.
- **A `user` claim protects a field *pair*** (`isbn_13`/`isbn_10`,
  `series`/`series_index`) in both the merge clause and the stamp. Adding a new
  pair means using `Rule::pair`, not writing a second guard.
- **`DiagnosticKind` is mirrored in full in `crates/api/src/dto.rs`** with an
  exhaustive match. A new variant is never engine-local — it will refuse to
  compile the API crate, which is correct.
- **Absence is not zero, anywhere.** Aggregates return absent minutes for a month
  with no device data; a measured twenty-second session records `Some(0)`.
- **No task-completion framing, ever** — no counts of what the user has not done,
  no streaks, no badges. `docs/decisions.md` bans it by name and there are tests
  asserting it against drawn buffers.
- **Provider enrichment stays off the device pull path.**
  `import_book_from_sidecar` is fully offline by design.

## If you run this as a multi-worker wave again

The mechanics worked; four things are worth repeating and three cost time.

Repeat: a **git worktree per item** with an APFS-cloned `target/`
(`cp -Rc`, ~90s, parallel builds with no cold start); one **prompt file per
thread** under `docs/prompts/`; **`make ci` run directly on every branch after
rebase and on main after every merge**, never trusting a worker's own report;
and **feeding each landed item's corrections into the next item's brief** before
launching it — that is what stopped item 31 writing a delete-then-insert that
would have wiped item 21's highlight-filler days.

Avoid: creating a worktree from `main` **before committing** the prompt edit it
needs (the worker reads the committed tree, not yours); trusting `claude -p`'s
shell exit code (it is the pipeline's — a transport drop reports
`subtype=success` with `is_error=true`; `rb-wt/run-item.sh` and
`resume-item.sh`, if still present, read `is_error`); and assuming a clean
textual rebase is a clean rebase — **parallel worktrees produce semantic
conflicts git cannot see**, and this wave hit one (a signature change against a
sibling's test seeds) that only `cargo test` catches, because
`cargo check --workspace` does not resolve dev-dependencies.

Also: `.claude/agents/` has `cargo-tester`, `api-surface-auditor`,
`web-checker` and `screenshot-reviewer`; `.claude/skills/new-wave-item` is the
ritual for starting any numbered item and `wrap-session` for closing out.
