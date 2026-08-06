# Prompt — Item 34: the sort keys get an index, and a writer

Paste into a fresh session at the repo root, on branch `feat/engine-sort-keys`,
branched from `main` at the head of the 2026-08-06 non-GUI wave.

---

Read `docs/decisions.md` (items 17, 18 and 20 — all three touched this and all
three deferred part of it), `crates/engine/src/storage/CLAUDE.md`, and
`crates/engine/migrations/CLAUDE.md` **before you write the migration**.
`CLAUDE.md`'s **Engine standards** section is binding.

**Owns migration `0016`.** Engine + one CLI door. No API, no TUI, no GUI.

## What

Indexes on `books.last_modified`, `books.title COLLATE NOCASE` and
`books.publish_year`; a **writer** for `sort_title`; and `sort_author` — which
lands only inside this item or not at all.

## Why these together

There is no index on any sort key today (`grep "CREATE INDEX" migrations/` — the
seven that exist are all foreign keys). So `ORDER BY title` sorts the whole table
however you paginate, which is what makes a deep page expensive, and it is what
turns item 18's `books.id` tie-break from insurance into load-bearing.

And `sort_author` was refused twice, at items 20 and 18, on one ground —
`storage/books.rs:42` records it: SQLite cannot parse a human name, so the column
is NULL for every existing row and `ORDER BY sort_author` is silently *wrong*
until a back-fill nobody has run runs. It only pays **inside** this item, where
the back-fill and the index arrive together.

**The refusal has expired for a second reason.** Every database in play is
disposable, stated by the user on 2026-08-06: `dev-data/` is seeded, gitignored
and rebuilt by `make dev-db`, and there is no durable library yet. So a shape
change needs no data migration. Still write the door — see below — but it does
not gate the index.

## The trap this item is really about

`sort_title` has **never been computed by anything**.

It is in `0001_init.sql:4`. It is a `MERGE_RULES` column (`books.rs:369`) and
`Federated::Local` — `books.rs:3305` asserts the local set is exactly
`["sort_title", "cover_path"]`, and `search.rs:474` records that those two are
the columns `merge_into` never touches. It is on `Book` (`book.rs:9`) and on
`BookDto` (`dto.rs:80`) and in the generated TypeScript. The upsert binds it
(`books.rs:823`). `gen-devdb` writes it as literal `NULL` (`devdb.rs:295`).

And `BookSort::Title` orders by `books.title COLLATE NOCASE`, because the column
it should have used is NULL everywhere.

**A sort-key column added without a writer looks answered and is not.** This item
either gives `sort_title` a writer or deletes it. Leaving a third state is what
produced the trap, and shipping `sort_author` in the same third state would be
doing it twice.

## The pattern to copy

Item 20's `invalidate_cover_metrics`: a companion clause **generated from another
column's value expression**, bound from Rust because SQL cannot derive it, so it
cannot fall out of step with the merge rule, the user guard, or `merge_books`'
`dst`-wins inversion. Find it and read it before you write anything.

`crates/engine/src/names.rs` already holds the name arithmetic, so the derivation
exists; what is missing is the wiring and the back-fill.

**The encoding problem, which is the hard part.** `names::sort_key`
(`names.rs:174`) returns **`(u8, String, String)`** — a rank (0 = has an author,
1 = authorless, which is what puts authorless books last), the lowercased last
name, and the lowercased whole name as a tie-break. A column is one TEXT value.

So you have to choose an encoding and make `ORDER BY` on the stored string
reproduce that tuple's order **exactly**, including the authorless-last rank and
including whatever your separator does when a name contains it. A column that
*nearly* agrees with the function is worse than no column, because both exist and
only one is read. A property test over arbitrary author lists asserting
`stored_order == tuple_order` is the right shape here; `names.rs`'s existing
`mod props` is the pattern.

`BookSort::Author` is currently the only arm not ordered by `ORDER BY` — it reads
the whole library, sorts in Rust and *then* truncates. Replacing that is the
point. Its doc comment at `books.rs:33-46` is the thing to rewrite honestly.

## The back-fill needs a door

`rb covers` (landed 2026-08-06, `crates/cli/src/commands/covers.rs`, 78 lines) is
the shape: a CLI verb, idempotent, whose work list is "the rows that have not got
one yet", whose wording says `every … is already …` rather than a bare zero
(**absence is not zero, anywhere**), and which `make dev-db` runs. Read it.

A back-fill with no door is a function nothing ever calls, which is how
`measure_stored_covers` sat unexercised for a wave.

## Done when

- `EXPLAIN QUERY PLAN` shows the index used for **each** sort, and you **assert
  it**. A behavioural test cannot see an index. This is not theoretical:
  removing item 18's `books.id` tie-break leaves the behavioural partition test
  **green** — measured — because SQLite's sorter is deterministic for one plan
  over one set of rows, and that determinism belongs to the query plan and not
  to the schema. `order_by_is_a_total_order` reads the SQL for exactly this
  reason. Copy the trick.
- Every write path that changes a title or an author list moves the sort key
  with it. Enumerate them: the upsert, `enrich_book`'s `UPDATE`, `merge_books`'
  `dst`-wins fill, `rb set`, every importer. `MERGE_RULES` generates five things
  now — check all five.
- Back-fill door exists, is idempotent, and `make dev-db` runs it.
- `docs/decisions.md` **appended**.

## Must not

- Add `ORDER BY sort_author` as a *second* arm beside the slow one. It replaces
  or it does not land.
- Ship a column without the back-fill.
- Touch the API, the DTOs or `bindings.ts`. `BookDto.sort_title` already exists
  and its meaning changes, which is worth a sentence in `decisions.md` — but the
  wire shape does not, and item 33 owns that file this wave.

## Files

`crates/engine/migrations/0016_*.sql`, `crates/engine/src/storage/books.rs`
(`BookSort`, `MERGE_RULES`, `BOOK_COLUMNS`, the upsert), `crates/engine/src/names.rs`,
`crates/engine/src/storage/query.rs` if the ORDER BY lives there, a new CLI
command under `crates/cli/src/commands/`, `Makefile` (`dev-db`),
`crates/corpus/src/devdb.rs` if the seed should stop writing `NULL`.

**Collides with nothing else in round 1** — items 33 and 36 do not touch
`books.rs`, `names.rs` or the CLI.

## How you are gated

**Not `make ci`** — a fresh worktree has no `gui/node_modules`, so `web-check`
and `routes` print `SKIPPED:` and you would "pass" them without running them.

Run **`make fmt lint build-check test ts-check`**, and read the exit code
properly: never `make test | tail -25`, which reports *tail's* status. Redirect
to a file and read `$?`.

## The one guaranteed conflict

`docs/decisions.md` — **append** your entry and restructure nothing. The file is
in **build order, not numeric order**, deliberately.

## Report the corrections this forced

In the shape `docs/decisions.md`'s existing entries use.

**Push back rather than comply.** Two places this prompt may be wrong: deleting
`sort_title` may genuinely be the better answer than writing it (nothing has
read it in the whole life of the schema, and `books.title COLLATE NOCASE` is
what a user actually sees), and a stored `sort_author` may not be encodable as
one TEXT column without a lie — in which case say so and ship the three indexes
alone rather than shipping a column that nearly agrees.

> **Note on `cargo-tester`.** If you are a subagent you cannot launch it —
> subagents cannot spawn subagents. Run its procedure directly:
> `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
> `cargo test --workspace`. Say which you ran.
