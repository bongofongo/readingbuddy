# Prompt — Item 21: `reading_events`, the source-agnostic activity log

Paste into a fresh session at the repo root, on branch `feat/engine-reading-events`.

---

Read **`docs/gui/spec-gui-17-28.md` (item 21)** — that is the authoritative
design and it is short — then `docs/spec-engine-29-32.md` (why it moved into this
wave) and `docs/decisions.md`. `CLAUDE.md`'s **Engine standards** section is
binding, and `crates/engine/migrations/CLAUDE.md` before you write the migration.

**Engine only. Owns migration `0011`.** No CLI, no TUI, no API DTOs — item 18 and
the GUI wave consume this later, and a surface built now is a surface built
against a guess.

This item was pulled forward from the 17–28 wave because item 31 (KOReader's
`statistics.sqlite3`) needs somewhere to put reading time, and the settled answer
is that **no source is consumed in its own shape**. You are building the shape.

## Migration `0011_reading_events.sql`

```
reading_events(
  book_id, reading_id, day,   -- the grain: a book, a read, a date
  minutes,                    -- nullable
  pages,                      -- nullable
  source,                     -- koreader | vault | goodreads | local | …
  confidence                  -- measured | inferred
)
```

`reading_id` is nullable with `ON DELETE SET NULL` — the same call `notes` made
in `0007`. `book_id` cascades. `day` is a date, not a timestamp; say in the
migration comment which encoding you chose and why (a `TEXT` `YYYY-MM-DD` sorts
and compares correctly in SQLite and survives a timezone change; an integer day
number does not read in a `sqlite3` shell — pick one and defend it).

Vocabulary lives in a **comment, not a `CHECK`** — `readings.source` set that
precedent in `0005` deliberately, and item 22's spec entry records that `'local'`
needed no schema change because of it.

Think hard about the primary key. The same day can carry several events from
different sources, and re-running a filler must not duplicate what it wrote last
time. Idempotency here is the property, and it is the one this table lives or
dies on.

## The fillers, which are what proves the table

The spec lists four available **without touching a device**. Build them:

| filler | supplies | confidence |
|---|---|---|
| `highlights.ko_datetime` | a day you were in the book | inferred |
| `notes.created_at` | a day you thought about it | measured |
| `readings.started_at` / `finished_at` | the endpoints | measured |

The fourth (local reading, item 22) is not built yet — leave the seam, do not
stub it.

**The vault filler is the one worth dwelling on**, per the spec: notes,
reflections, annotations and citations are data readingbuddy fully originates, no
importer can fail to supply them, and the positioning is the desk rather than the
reader.

## The aggregates

There is not a single reading-stats aggregate in the engine today, only invariant
checks. Add the ones the spec names: books finished per period, activity days per
period, notes and links created per period, pages and minutes where known.

**Every aggregate must be able to say it does not know.** A month with no device
data returns **absent** minutes, not zero. This is the same discipline as
`goodreads_for` returning `None` rather than rounding, and as Goodreads import
refusing to invent a start date. **Zero is a claim.** If your return type cannot
express "no data" separately from "no reading", the type is wrong.

## Must not

- **No task-completion framing.** `docs/decisions.md`'s axiom bans it by name.
  These aggregates will end up on a screen; nothing here counts what the user has
  not done, and nothing produces a streak that can be broken.
- **No `statistics.sqlite3`.** That is item 31, running in parallel with you in
  another worktree. Leave the filler seam; do not reach for the device.
- No inventing a day. A highlight with no `ko_datetime` produces no event.

## Files

`crates/engine/migrations/0011_reading_events.sql`, a new
`crates/engine/src/storage/reading_events.rs`, `storage/mod.rs`, `lib.rs`
(facade methods), tests. Nothing else. Item 29 is editing
`storage/books.rs` concurrently — stay out of it.

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, and there are
properties (not just examples) for idempotent refill and for the absent-vs-zero
distinction. The PR body says what changed and what you deliberately left out.

**Push back rather than comply.** Four of five threads in the last wave did and
each time they were right. If the grain is wrong, if the primary key cannot be
made idempotent, or if an aggregate cannot honestly express absence — say so in
the PR rather than building it anyway.

**Report the corrections this forced.** Every item in `docs/decisions.md` records
what building it changed about the plan. Ask yourself at the end what the spec
was silent about or wrong about, and write that paragraph — it is the most
valuable thing the item produces.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
