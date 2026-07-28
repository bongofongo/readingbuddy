# Prompt — Item 7: Reflection and Review

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/engine-reflection-review`).

---

Read `docs/spec-engine-04-07.md` (item 7) and `docs/decisions.md` (the
*Reflection and Review*, *Ratings* and *Vault* sections) before starting.
`CLAUDE.md`'s **Engine standards** section is binding.

**Depends on item 4** — a reflection anchors to a reading, and that is a foreign
key, not a preference. Owns migration **`0007`**, merged after `0005`.

**Engine + CLI only.** The TUI surface is a later thread; do not add a book-view
section here.

## The two objects

|  | Reflection | Review |
|---|---|---|
| Audience | private | public |
| Purpose | personal agglomeration; ties books together | rating + prose for others |
| Graph role | **the hub** — cites highlights, links notes and other reflections | exportable artifact |
| Rating | none | yes |
| Timing | **openable mid-book, accretes** | written at the end |

Book-to-book connection runs **reflection-to-reflection**. That is the whole
reason this item exists, and it is what the cross-referencing work later hangs
off.

## Reuse `notes`; do not build a parallel vault

Both are markdown in the vault with a DB record, both anchored to a reading, and
only Review carries a rating. `notes` already supplies all of that machinery:
`slugify`, `frontmatter` / `frontmatter_and_body`, `extract_wikilinks`
(`notes.rs:69`, `:94`, `:106`, `:173`), the `note_links` edge table with
dangling targets held as text and back-resolved (`storage/notes.rs:105-123`),
the FTS5 body cache, and `Engine::update_note_body` / `delete_note` /
`refresh_note_from_disk`.

The deciding argument is the graph: a reflection is meant to be **the hub**, and
`note_links` *is* the graph. A separate table would need its own edges, its own
FTS index and its own editor path — and the backlinks pane (item 9) would then
have to learn two vocabularies for one idea.

So: reflections and reviews are notes with a new `kind`, plus three small side
tables for the things a note has no room for.

## Migration `0007_reflection_review.sql`

Full SQL in the spec. In outline: `notes.reading_id` (nullable, `ON DELETE SET
NULL`); `kind` gains `'reflection'` and `'review'`; `UPDATE notes SET kind =
'reflection' WHERE kind = 'final'` (the old `final` is superseded — this answers
Q16 in `docs/ux-positioning.md`); partial unique indexes giving **one reflection
and one review per reading**; then `rating_scales`, `rating_map`,
`review_ratings` and `citations`.

## Constraints that must survive the implementation

These are settled decisions, not suggestions:

- **No shared body.** A review is written separately. It is never derived from
  the reflection — not by a `public:` frontmatter key, not by a `--- public ---`
  divider. A public review is a rewrite for a different audience, not a subset
  of private thinking. Deriving it sounds elegant and produces bad reviews.
- **Reflections are openable mid-book and accrete.** Created with the reading,
  not at the end. That is what gives the currently-reading home screen (item 8)
  a natural action: *open the reflection for what you're reading*.
- **Rating lives on the Review only.** A reflection that wants a private score
  can have one later; it is not the same number and must not share a column.
- **Store the raw value plus the scale id, never only the mapped value.** The
  map is user-editable, so the mapping has to stay re-derivable.
- **The Goodreads mapping is an explicit lookup table, never a formula.**
  Formulas are always wrong at the ends.
- Goodreads' CSV `My Rating` is **integer 0–5** (0 = unrated), no halves.
  Say so when a scale value has no mapping; never silently round.
- **Numeric scales only for v1** (`min`, `max`, `step`). Ordered-label scales
  are explicitly out.
- The vault is ours; Obsidian compatibility is a courtesy. Keep `[[wikilinks]]`
  and plain markdown, and do not make an Obsidian-only construct load-bearing.

## Engine

Keep `notes::create_note`'s file-and-frontmatter path as the **single writer** of
vault files — frontmatter gains `reading:` and the new `kind` values, emitted in
the existing key order (`notes.rs:106-140`).

```rust
pub async fn open_reflection(&self, book_id: i64, reading_id: Option<i64>) -> Result<CreatedNote>;
pub async fn open_review(&self, book_id: i64, reading_id: Option<i64>) -> Result<CreatedNote>;
pub async fn set_rating(&self, note_id: i64, value: f64) -> Result<()>;
pub async fn cite(&self, note_id: i64, highlight_id: i64) -> Result<()>;
pub async fn citations_for(&self, note_id: i64) -> Result<Vec<Highlight>>;
pub async fn goodreads_rating(&self, note_id: i64) -> Result<Option<u8>>;
```

`open_*` create on the first call and return the existing record after — that is
what "accretes" means in the API. `reading_id: None` means the book's active
reading; open one if there is none and the caller asked for a reflection
(mid-book is the normal case).

`goodreads_rating` returns the mapped integer, or a typed diagnostic when the
value is unmapped. **`EngineError::Other` is last-resort** — if a caller might
branch on "no mapping for this value", it deserves its own variant or a
`DiagnosticKind`.

Citations are **by reference** (`citations(note_id, highlight_id)`), so a review
stays live when a highlight's device-owned fields are refreshed, and so
"which highlights did I actually use?" is a query.

## CLI

`rb reflect <book> [--reading N]`, `rb review <book> [--rating X]`,
`rb cite <note> <highlight-id>`, `rb rating scale --min --max --step`,
`rb rating map <value> <0-5>`.

Body capture reuses `prompt::edit_in_editor` (`cli/src/prompt.rs:38`), the same
path `rb note` already takes. Output through `render.rs`-style plain builders.
**No CSV export** — Goodreads in/out is item 10.

## Tests

All `sqlite::memory:`, all offline.

- Opening a reflection twice returns the same note and the same file.
- The partial unique indexes refuse a second reflection/review per reading.
- A reread gets its own pair; the first survives with its dates.
- `kind = 'final'` rows migrate to `'reflection'` and keep their `file_path`.
- **Wikilinks in a reflection land in `note_links` and back-resolve.** This is
  the graph claim; assert it rather than assuming it.
- Citations survive a device refresh — highlight ids are stable across
  `refresh_device_fields`, which `highlight_ids_are_stable_across_refresh`
  already guards — and cascade on highlight delete.
- Rating: the raw value round-trips; an unmapped value returns the diagnostic
  rather than a rounded integer.
- Property: once every point on a scale is mapped, the map is a total function
  on that scale. Prefer this over more examples — the rule is general.

## Constraints

- Engine + CLI. **No TUI.**
- No network in tests, ever.
- Typed `Diagnostic`s, never pre-formatted strings.
- Never edit an applied migration. `0007` is yours; merge after `0005`.
- **Note bodies are the user's private reading — never above `trace!`.** A
  reflection body is the most private text in the system; a review's is not far
  behind.
- The vault file is the source of the body; the DB row is metadata plus the FTS
  cache. Do not start storing the body twice.

## Done when

`make ci` green; a reflection can be opened mid-book and reopened unchanged; a
review carries a rating on a user-defined scale that maps to a Goodreads
integer, and an unmapped value reports rather than rounds; a `[[wikilink]]` from
a reflection to a note resolves. Run the `cargo-tester` agent before committing.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
