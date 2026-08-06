# 2026-08-06 — the sort keys get an index

Item 35 of the non-GUI wave, alone in a worktree on `feat/engine-sort-keys`.
Migration `0016`: four indexes, one new column, and the answer to a column that
had been in the schema since `0001_init.sql` without ever being computed.

The item was specified as "indexes, plus a writer for `sort_title`, plus
`sort_author`". It came out as one thing rather than three, because the writer is
what makes the index worth having and the encoding is what makes the writer
honest.

## Decisions locked

- **Four indexes, and the fifth sort cannot have one.** `last_modified`,
  `COALESCE(sort_title, title) COLLATE NOCASE`, `sort_author`, `publish_year`.
  `BookSort::Progress` is a computed ratio across a `LEFT JOIN`; its plan still
  says `USE TEMP B-TREE FOR ORDER BY` and that is kept as a **live control**
  rather than tidied away — it is what proves the other four assertions are ones
  this suite can fail. `0008` bought that proof by running the plan before its
  migration; a control that goes on being true is stronger.
- **`sort_title` got a writer rather than a delete**, and the delete was a real
  contender. Nothing had read it in the life of the schema and `books.title` is
  what a user sees. It lost on scope: the delete reaches `BookDto` and
  `bindings.ts`, which item 34 owns this wave, and a half-delete (column gone,
  DTO field always null) is a *fourth* state and worse than the third.
- **…and then it left `MERGE_RULES`.** This is the decision the prompt did not
  ask for and the one the item turned on. See below.
- **`sort_author` is not on `Book`.** It is the engine's filing key, not a fact
  about the book. Keeping it off the domain type keeps it off the wire — no DTO
  field, no `dto.rs` edit in a wave where somebody else owns that file, and no
  frontend that can render a string full of control characters.
- **`BookSort::Author`'s Rust arm is deleted, not supplemented.** It used to read
  the whole library, sort with `names::sort_key` and *then* truncate, so page 40
  cost what page 1 did. It is an ordinary indexed `ORDER BY` now, and
  `order_by_is_a_total_order` no longer skips it.
- **`rb sort-keys` is the door**, wired into `make dev-db` beside `rb covers`.
  Work list `sort_author IS NULL`; idempotent; wording obeys both absence rules.
- **The article rule is English-only and narrow.** `books.language` is NULL for
  most of a real library, so `der`/`el`/`la`/`le` would be applied on a guess and
  would file *Das Kapital* under "Kapital". `pdf.rs`'s reason.
- **`crates/corpus`' seed still writes `sort_title` as NULL, deliberately.** The
  door fills it on every `make dev-db`, which is what *exercises* the door — the
  same arrangement that keeps `rb covers` honest. A seed stating the key would be
  a fixture agreeing with the engine by copying it.

## The encoding, which is where the time went

`names::sort_key` returns `(u8 rank, String last, String whole)`. A column is one
TEXT value. The claim the column has to make is that `ORDER BY sort_author`
reproduces that tuple's order **exactly** — "nearly" is worse than no column,
because both exist and only one is read.

The naive `format!("{rank}|{last}|{whole}")` is wrong and wrong *silently*. For
`a ⧺ SEP ⧺ b` to compare like `(a, b)`, the separator has to be strictly less
than every byte a component can contain. With `SEP = '|'`, `("", "z")` and
`("!x", "a")` come out in the wrong order and nothing anywhere says so.

So: `SEP = \u{1}`, and every component goes through an escape that maps
`\u{0} → \u{2}\u{2}`, `\u{1} → \u{2}\u{3}`, `\u{2} → \u{2}\u{4}`. That code is
**monotone** (the images are in source order) and **prefix-free** (two bytes
starting `\2`, or one byte `≥ \3`), which is exactly what makes concatenation
order-preserving. UTF-8 is itself prefix-free and monotone and the three
rewritten chars are single-byte ASCII, so the composition is monotone on whole
strings.

Two facts had to be checked rather than assumed, and both hold: SQLite's BINARY
collation is `memcmp`, and Rust's `Ord for String` is byte-wise. So the two ends
compare the *same* way rather than similarly, and a proptest comparing the two
encodings in Rust is comparing what the database will compare.

`\u{0}` was rejected as the separator on a practical ground rather than a
theoretical one: SQLite stores an embedded NUL happily and compares past it
correctly, but every `sqlite3_column_text` C-string consumer truncates at it —
including the `sqlite3` shell `make dev-db` runs, i.e. the tool you would use to
check the column.

The escape only ever fires on control characters in an author's name, which is
never. It is there because the property is asserted **unconditionally**, and
scoping it honestly (`prop_assume!` no control chars) would have been asserting
less about the exact thing the column claims.

## `sort_title` leaving `MERGE_RULES` — the correction the item forced

Giving the column a writer made a contradiction visible that had been invisible
while it was NULL.

`MERGE_RULES` governs what a **record** can carry, with a source to attribute.
`sort_title` sat in it as `Federated::Local` — "ours, derived" — while nothing
derived it. The moment something did:

- `a_record_claims_exactly_the_fields_it_supplies` failed on the first run, with
  `a title record moved sort_title`. Correct: the derivation moves it. But the
  sweep was reporting a real thing, not a stale expectation.
- Worse, and not caught by any test: the upsert stamped `field_provenance` for
  whatever `fields_said` returned, so a record carrying a `sort_title` got a
  claim naming *googlebooks* for a value `refresh_sort_keys` had already
  replaced. A `field_provenance` row naming an origin that supplied nothing.

Both are migration `0014`'s argument one column over — "a record-shaped writer
able to move `cover_width` without moving `cover_path`" is "able to move a filing
name without moving the title it comes from". So `sort_title` takes the cover
metrics' shape instead: one writer, no claim, ignored by every merge.
`only_our_own_columns_sit_out_the_federated_merge` is down to `["cover_path"]`.

The cost, stated because it is a real loss: there is no longer a door for a user
to file a book under a name of their own. Nothing had ever opened it — `rb set`
has no flag for it — and re-opening it is a `Rule` and a flag, not a migration.

## Derive from the row, not from the record

The first design bound the computed keys beside the record's other columns. It is
wrong on a **partial** record, which is the ordinary case:

```
title = CASE WHEN excluded.title != '' THEN excluded.title ELSE books.title END
```

A record silent about the title keeps the row's — so a key bound from the record
describes a title the row does not have. `refresh_sort_keys` reads the merged row
back instead, which makes the two coherent through the merge rule, the user guard
and `merge_books`' `dst`-wins inversion alike, without re-spelling any of them.
That is `invalidate_cover_metrics`' rule reached by a different route: the
companion value comes from the column's own *stored* value, never the caller's
copy of it.

## Gotchas

- **`make test | tail` reports tail's exit code.** Every run here redirected to a
  file and read `$?` from the un-piped command. Worth the habit.
- **An expression index *is* usable for `ORDER BY`** — measured before writing
  any Rust, with `sqlite3` and `EXPLAIN QUERY PLAN`. It was not obvious and the
  whole `COALESCE(sort_title, title)` design depends on it.
- **`books.id` needs no column in any index.** It is the `INTEGER PRIMARY KEY`,
  i.e. the rowid, and SQLite appends the rowid to every index entry — so the
  item-18 tie-break comes free off the same scan in either direction.
- **`EXPLAIN QUERY PLAN` has to be filtered by `parent`.** `BOOK_FROM`'s
  correlated subquery has an `ORDER BY` of its own and therefore a temp b-tree of
  its own, for ever and by design. Asserting over the whole tree either passes
  vacuously or fails on a line unrelated to the sort key.
- **Deleting `list_books_by_author` orphaned `BookQuery::skip`/`take`.** They
  existed to teach the Rust slice SQLite's "negative means no limit" convention;
  with one arm left, the convention is the database's alone. Deleted, and the
  test rewritten to assert what actually survives.
- **The upsert's column list and its row of `?`s were hand-written beside a
  clause generated from `MERGE_RULES`**, and the three `UPDATE`s named `?20`/
  `?21` — four numbers decided by that table's length. Removing a column is
  exactly what breaks them. Computed now.
- **The TUI's title sort was a second opinion** (`b.display_title().to_lowercase()`)
  that agreed with the database only because `sort_title` was NULL everywhere. It
  calls `sort::title_key` now, the way its author arm already called
  `names::sort_key`. Item 17's finding, arriving a second time for titles.
- **`migration_versions_are_contiguous_from_one` fails on this branch**, and is
  expected to: `0015` is item 34's and has not merged. It is the one red test in
  the gate and it goes green on the orchestrator's merge. Nothing else fails.

## Files

- `crates/engine/migrations/0016_sort_key_indexes.sql` — new
- `crates/engine/src/sort.rs` — new; the two derivations, the encoding, the props
- `crates/engine/src/storage/books.rs` — `order_by`, `refresh_sort_keys`,
  `MERGE_RULES` (19 → 18), `tail_params`, the plan tests
- `crates/engine/src/storage/query.rs` — the module header's keyset tally,
  `BookQuery::limit`'s doc, `skip`/`take` deleted
- `crates/engine/src/search.rs` — the `Federated::Local` pair is now one column
- `crates/engine/src/lib.rs` — `pub mod sort`, `Engine::rebuild_sort_keys`
- `crates/engine/tests/migrations.rs` — the before/after and the near-miss test
- `crates/cli/src/commands/sort_keys.rs` — new; the door
- `crates/cli/src/{main.rs,commands/mod.rs}`, `crates/cli/tests/cli.rs`
- `crates/tui/src/ui/library.rs` — one line, outside the prompt's file list
- `Makefile` — `dev-db` runs the door
- `docs/decisions.md` — appended
