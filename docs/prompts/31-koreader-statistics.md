# Prompt — Item 31: reading time, from the device's own statistics

Paste into a fresh session at the repo root, on branch `feat/engine-ko-statistics`,
**branched from main after item 21 has merged.**

---

Read `docs/spec-engine-29-32.md` (item 31), `docs/gui/spec-gui-17-28.md` (item
21 — the table you are filling), `docs/koreader-format.md`, and
`crates/engine/CLAUDE.md` (the `device.rs` and `partial_md5.rs` sections).
`CLAUDE.md`'s **Engine standards** section is binding.

**Engine only. No migration** — you write into item 21's `reading_events`. If
that table is not on main yet, stop and say so rather than minting your own.

## The gap

`partial_md5.rs`'s own doc comment names *"the `statistics.sqlite3` join"* as one
of the three things it exists for, and **nothing has ever done it**.
`device_books` already holds the keys. This is the richest signal in the app —
measured time on a real device — and the only one the user cannot get anywhere
else.

KOReader keeps `koreader/settings/statistics.sqlite3`: a `book` table keyed by
its own `md5` (which is `partial_md5_checksum`, per `docs/decisions.md` —
**`stats.md5` does not exist** on any 2024.11+ device) and a `page_stat_data`
table of per-page durations. **Verify that against a real device before building
on it** — the schema here is stated from documentation, not from observation, and
the corpus contains no statistics DB at all.

## It is one more filler, and that is the whole design

Day, minutes, pages, `source = 'koreader'`, `confidence = 'measured'`, into
`reading_events`. **No view, no query and no line of GUI changes when it
arrives** — that is item 21's entire point, and if you find yourself wanting a
`reading_sessions` table or a KOReader-shaped struct that reaches the facade, the
design has gone wrong.

Refilling must be idempotent. A device is scanned repeatedly; the same day must
not accumulate.

**Three things item 21 settled that bind you**, discovered while building it and
not in this prompt's first draft:

- The primary key is **`(book_id, day, source)`** and you write `source =
  'koreader'` — the *same* rows the highlight-day filler already writes. You are
  upgrading its inferred days with measured minutes, not adding a parallel set.
- The upsert is a **no-clobber merge**, not last-writer-wins: `COALESCE` the
  fields you have no opinion about, and `confidence` ratchets `inferred` →
  `measured` and never back. Follow it. **Do not delete-then-insert scoped by
  source** — that wipes the highlight filler's days, and it is the one way to
  break item 21's promise that a later filler changes no query.
- `reading_id` is an **attribution**, not part of the key. Leave it NULL when the
  day's evidence does not settle on one read.

## Three things it must get right

- **Read-only, copy-then-read.** It is the user's device and a live SQLite file
  that KOReader may be writing to. Copy it out first. Never open it read-write,
  never `VACUUM`, never write to the volume — the same discipline `scan_device`
  already holds.
- **Absence is ordinary.** No statistics DB, an unrecognised schema version, a
  book with no row, a page with no duration — all `Diagnostic`, never an error,
  never a zero. *A month with no device data returns absent minutes, not zero.*
  **Zero is a claim.** Add an `ErrorClass`/`DiagnosticKind` variant rather than a
  formatted `String`; `CLAUDE.md`'s engine standards require it.
- **Attribution into readings** uses the same window logic as
  `attribute_highlights`, and **leaves `reading_id` NULL when no window holds the
  day.** KOReader's statistics are per-file and know nothing about rereads. The
  first cut of `attribute_highlights` got this wrong by `COALESCE`ing a missing
  bound to ±infinity, which silently gave the newest read every earlier read's
  data with nothing on screen looking wrong. Read that code before writing this.

## Must not

- **Not the plugin.** Item 15 is the KOReader plugin and this is not it. You read
  a file off a volume the user has already mounted, which is what `scan_device`
  does today: no plugin, no write to the device, no network.
- **Not in `sync_device`'s default path** without the user asking for it.
  `docs/decisions.md` makes arrival read-only, and a scan that silently starts
  importing months of timing data is not read-only in spirit.
- **No per-page detail in the domain.** `reading_events`' grain is a day. Storing
  every page turn is a different item and a much bigger one; if you think the
  grain is wrong, argue it in the PR rather than widening it here.
- **Reading time is the user's private reading.** `CLAUDE.md`'s tracing rule:
  nothing above `trace!`.

## Files

A new `crates/engine/src/ko_statistics.rs` (or a section of `device.rs` — decide
and say why; `device.rs` exists because *a scan is not an import*, and the same
argument may apply again), `crates/engine/src/lib.rs`, `diagnostic.rs`, tests and
a fixture. Item 30 is running concurrently in `providers/` and the CLI — no
overlap expected.

**The fixture is the interesting problem.** There is no statistics DB in the
corpus and the user's own device is personal and uncommittable. Build one:
`crates/corpus` generates fixtures and **does not depend on `readingbuddy`**, so
that reusing the engine's own parsing to build its fixtures cannot bake a bug
into the goldens. Follow that rule. If you conclude the fixture must instead be
*recorded* from a real device — the documented exception the Goodreads and
calibre `recorded/` fixtures take — argue it.

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, no test touches the
network or a real device, and the absent-vs-zero distinction has a property
rather than an example. The PR body says what the real schema turned out to be.

**Push back rather than comply.** If the statistics schema does not carry what
this prompt assumes, report what it *does* carry and stop — a filler built
against a guessed schema is worse than no filler.

**Report the corrections this forced.** This item has more unknowns than any
other in the wave, so its correction paragraph is the most valuable one.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
