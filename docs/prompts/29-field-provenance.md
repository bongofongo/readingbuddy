# Prompt — Item 29: where each field came from

Paste into a fresh session at the repo root, on branch `feat/engine-field-provenance`.

---

Read `docs/spec-engine-29-32.md` (item 29), `docs/decisions.md` (the **Data
ownership** section — this item is that section made true), and
`crates/engine/src/storage/CLAUDE.md`. `CLAUDE.md`'s **Engine standards** section
is binding, and `crates/engine/migrations/CLAUDE.md` before you write the
migration.

**Engine only. Owns migration `0012`.** No CLI, no TUI, no API. Item 30 consumes
this immediately afterwards and is what makes it visible.

`docs/decisions.md:37` says *"Authority is per-field. Provenance is recorded."*
**The second sentence is not true.** `external_ids` and `book_tags` record
provenance about rows; nothing anywhere says where `page_count` came from or
when. You are closing that gap.

## Migration `0012_field_provenance.sql`

```sql
CREATE TABLE field_provenance (
  book_id    INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  field      TEXT NOT NULL,
  source     TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (book_id, field)
);
```

**Your branch's `migration_versions_are_contiguous_from_one` fails until item 21
merges `0011`.** That failure is expected and named in
`docs/spec-engine-29-32.md`. Rebase onto main after it lands; **do not renumber
to make it green.** Everything else must be green.

Sources: `openlibrary`, `googlebooks`, `calibre`, `epub`, `koreader`,
`goodreads`, `user`. Vocabulary in a comment, not a `CHECK` — `readings.source`
set that precedent in `0005` deliberately.

Consider whether a back-fill is honest. Every existing book has fields from
*somewhere* and the row does not say where; `openlibrary_key` /
`googlebooks_id` / `external_ids` / `device_books` are circumstantial evidence
and guessing from them is inventing provenance, which is the one thing this table
exists to stop. Leaving history unattributed and saying so may well be right —
make the call and defend it in the migration comment.

## Written from `MERGE_RULES`, never beside it

`crates/engine/src/storage/books.rs:146` has a sixteen-entry `MERGE_RULES` table
that already generates **both** the upsert's `ON CONFLICT` clause and
`enrich_book`'s `UPDATE`, precisely so the two cannot disagree about what merging
a partial record means.

Provenance is a **third consumer of that same table**, not a fourth
hand-maintained list of field names. A field present in one list and absent from
the other is the bug this arrangement makes unrepresentable — and it is the bug
that will otherwise arrive with item 32, which adds three more fields.

Every writer that currently touches those columns must stamp provenance:
`upsert_book`, `enrich_book`, the epub path, the calibre path, the Goodreads
path. Find them all; a writer that does not stamp is a field that silently claims
whatever it claimed last.

## `user` outranks everything, and that is the point

The `COALESCE` no-clobber pattern is right for partial provider records and
**cannot distinguish "the user typed this" from "a provider guessed it"**.
Without this, item 30's first run silently overwrites hand corrections. That is
the reason this item goes first and the reason it exists at all.

So: a field whose provenance is `user` is not overwritten by a provider merge.
Decide where that rule lives — the SQL generated from `MERGE_RULES`, or the
caller — and make it one place, not two.

There is no user-edit path in the engine yet for most of these fields. Build the
rule and a test for it anyway; the alternative is discovering it is missing from
inside item 30.

## Must not

- **No `value_hash` column** unless you can show it earns its place in this item.
  `docs/spec-engine-29-32.md` names it as an open question for the *next* item to
  answer, and a column added speculatively is a column with no reader.
- **No provider changes.** Item 30 and item 32 are in `providers/`; stay out.
- No API or DTO surface. `crates/api` mirrors the domain deliberately and a
  half-designed field there is a public promise.

## Files

`crates/engine/migrations/0012_field_provenance.sql`,
`crates/engine/src/storage/books.rs`, a new provenance module or a section of
`storage/provenance.rs` (it already exists for `book_tags`/`external_ids` — decide
whether this belongs there and say why), `lib.rs`, tests. Item 21 is adding
`storage/reading_events.rs` concurrently — no overlap expected.

## Done when

`make ci` is green **except** the named contiguity failure, the `cargo-tester`
agent reports clean, and there is a property asserting that the provenance
writers and `MERGE_RULES` cannot drift apart —
`the_two_statements_cannot_disagree` in `storage/books.rs` is the shape to copy.

**Push back rather than comply.** If per-field provenance turns out to want a
different grain (per source rather than per field — a book can carry a page count
from OpenLibrary *and* a different one from calibre, and this schema keeps only
the winner), say so in the PR before building it. That is a real design question
and this prompt has picked one answer.

**Report the corrections this forced**, in the shape `docs/decisions.md`'s
existing entries use.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
