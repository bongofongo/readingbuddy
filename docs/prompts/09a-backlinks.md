# Prompt — Item 9a: backlinks in the engine

Paste into a fresh session at the repo root, on branch `feat/engine-backlinks`.

---

Read `docs/spec-08-10.md` (item 9a) and `docs/decisions.md` before starting.
`CLAUDE.md`'s **Engine standards** section is binding.

**Engine + CLI. Owns migration `0008`, and must merge before `0009` (T10).**
No TUI — the pane is thread 9b.

## Migration `0008_note_link_index.sql`

```sql
CREATE INDEX idx_note_links_to     ON note_links(to_note);
CREATE INDEX idx_note_links_target ON note_links(target_title COLLATE NOCASE);
```

> **Corrected after this prompt was executed (PR #7).** It gave the second index
> without a collation. `write_links` back-resolves with
> `target_title = ? COLLATE NOCASE`, and SQLite will not use an index whose
> collation differs from the comparison's — so the BINARY version leaves the plan
> at `SCAN note_links` and the migration buys nothing. Judge an indexes-only
> migration on `EXPLAIN QUERY PLAN`, never on the index existing.

Indexes only, no shape change. `note_links` has neither today
(`crates/engine/migrations/0001_init.sql:57-62`), so a backlink query is a full
scan — and so is `write_links`' back-resolution of dangling targets by title,
which runs on every note insert.

## What to build

- `Storage::backlinks(note_id) -> Vec<NoteRecord>` in `storage/notes.rs`, beside
  the existing *outgoing* `note_links(note_id)` (`:327`).
- `Engine::backlinks` and `Engine::outgoing_links`. The facade exposes **no** link
  method at all today — `note_links` is called only inside `open_anchored` to
  fill `CreatedNote.links`.
- CLI `readingbuddy links <note-selector>`: inbound and outbound, with dangling
  targets marked as text rather than silently dropped. That is what makes this
  half reviewable without waiting for the pane, and it is the same
  never-a-dead-end rule the rest of the CLI follows.

## The one thing to settle rather than assume

`write_links` back-resolves dangling edges when their target note appears, so in
principle `to_note` is always set once the target exists and `backlinks` can be a
plain `WHERE to_note = ?`.

**Write the test that proves it** — create note B, link to it from A written
*earlier*, assert the edge resolved. If it turns out there is a case where it
does not, union the dangling-by-title branch into `backlinks` and say why in the
doc comment. Do not add the union speculatively: an unexplained `OR` in a query
is the kind of thing nobody later dares delete.

## Tests

Use the shared harness at `crates/engine/tests/common/mod.rs` (`engine()`,
`seed_book`, …) rather than adding another copy of the fixture builder.

- inbound and outbound are different sets, and a note linking to itself is not
  double-counted
- a forward reference resolves when its target is created later
- deleting the target degrades the edge to dangling text rather than losing it
  (`to_note` is `ON DELETE SET NULL`, and that is deliberate)
- rewriting a body removes edges it dropped — `set_note_links` replaces rather
  than merges, and `crates/engine/tests/workflows.rs` already leans on that

Reflections are the hub the pane exists for, so at least one test should run
reflection-to-reflection rather than note-to-note.

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, and `rb links` has
been run by hand on two notes that point at each other. The PR body says what
changed and what was deliberately left out.
