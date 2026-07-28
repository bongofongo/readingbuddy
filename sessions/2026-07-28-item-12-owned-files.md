# 2026-07-28 — item 12: `book_files`, owned files, three-level dedup

One item, one worktree (`.claude/worktrees/item-12-book-files`), one PR. Picks up
from `58ac5ee` on `main` (items 1–10, migrations `0001`–`0009`) and implements
item 12 as specified in `docs/spec-11-16.md`. Engine only; migration `0010`;
offline and deterministic throughout.

## What landed

- **Migration `0010_book_files.sql`** — `book_files(sha256 PK, book_id, format,
  original_name, size, added_at)` plus `idx_book_files_book`.
- **`crates/engine/src/files.rs`** — the content store (`store`, `sha256_of`,
  `content_path`, `format_of`) and the dedup ladder (`identify`, `attach`,
  `import`).
- **`crates/engine/src/storage/book_files.rs`** — `add_book_file`, `book_file`,
  `book_files`, `delete_book_file`.
- **`EngineConfig::files_dir`** beside `images_dir`, created by `Engine::open`.
- **Facade**: `import_file`, `add_file_to_book`, `identify_file`, `book_files`,
  `file_path`, `remove_file`. `delete_book` now removes the bytes it owned.
- **`merge_books` repoints `book_files`** and reports `files_moved`.
- Tests: 6 unit + 2 properties in `files.rs`, 5 in `storage/book_files.rs`, 18 in
  `crates/engine/tests/book_files.rs`, 1 narrative in `workflows.rs`. `make ci`
  green.

## Decisions taken while building, and why

### `sha256` is the primary key, not `(book_id, sha256)`

The spec gives the columns, not the key. Making it global is what turns dedup
level 1 from a check into a constraint: identical bytes are one file, a second
import cannot write a second row, and **the same content cannot hang off two
books**. That last one reads like a limitation and is actually the level-1
answer — if one file is genuinely both books, the books are duplicates and
`merge_books` is the move, which is a thing this codebase already does well.

Two consequences fell out of it, both good:

- the merge's repoint is a plain `UPDATE` (no collision is representable, so no
  `UPDATE OR IGNORE` and no `dropped` count, unlike `book_tags`);
- `delete_book` can remove the bytes unconditionally, because nothing else can
  be holding them.

### No `partial_md5` column on `book_files`

Level 3's middle rung is `partial_md5`, and the tempting move is to store it
here. `device_books` already maps that value to a book and is read by the
sidecar matcher and by the future `statistics.sqlite3` join; a second copy would
be a second answer to one question, and the two would drift. So an import writes
`link_device_book(md5, book, Auto)` — the same hook `import_epub` has, with the
same method for the same reason (a scan must not relabel a manual link) — and
level 3 *reads* that table rather than shadowing it.

### `FileMatch` is its own enum, not a fifth `MatchMethod` variant

`MatchMethod` describes how a **sidecar** was matched and has no notion of file
content; `FileMatch::Sha256` is exactly the rung a sidecar can never take, and
adding it there would have put an unreachable arm into every frontend match on
`MatchMethod`. The three shared rungs keep the same names because they are the
same rungs — and the *matcher* is genuinely shared: level 3's last rung is
`koreader::title_scores_for` and `candidates_for_title`, unchanged, band and
all. "Do not invent a second matcher" is about the matcher, not the enum.

### The refusal shape, copied deliberately

`import_file` returns `FileOutcome::Unmatched` with the candidates and **writes
nothing** — not the row, not the bytes — when a near-miss title exists and
`new` is unset. That is `ko pull`'s shape verbatim. With no candidates at all
there is nothing to refuse over, so the book is created. A refusal is an
outcome, not an `EngineError`, because an error would throw the candidates away
and leave the user at a dead end.

### Creating a book goes through `import_epub`

Rather than a second metadata path written here. An epub therefore still gets
its OPF, its provider enrichment and its embedded cover; a format we have no
reader for gets its filename as a title and nothing more, which is honest. A
corrupt epub falls through to the filename with a `tracing::warn!` rather than
failing the import — the bytes are still the user's book and still worth owning.

### Crash-safety is one pass, not two

`store` copies and hashes in the same read loop into a temp file **inside
`files_dir`** (same filesystem, so the `rename` is atomic), `sync_all`s, and
only then renames. So the name at the content address is only ever a name for
bytes that were completely written. A plain `std::fs::copy` to the final path
would leave, after a kill or a power cut, half a file at a name claiming to be
the sha256 of the whole — and every later import of that book would see the
address occupied and skip it. Silently wrong, permanently.

The temp file is in `files_dir` rather than `$TMPDIR` on purpose: across
filesystems `rename` degrades to a copy, which is not atomic.

### `format_of` sanitizes rather than trusts

The extension becomes a path component and arrives off filenames from downloads,
devices and zip archives. Everything but ASCII alphanumerics is dropped, so `..`,
`/` and a NUL collapse to `bin` rather than to somewhere else on the disk. A
property test asserts the result is always exactly one safe component, for any
input string.

## Scope calls, stated rather than assumed

- **`import_epub` is unchanged and still does not copy the file.** The spec
  scopes item 12 to the engine and says nothing about `rb epub`, and making an
  existing command start copying every file it touches is a user-visible change
  that was not asked for. Owning a file is `import_file`'s job. The follow-up —
  wiring a CLI `rb file import` and pointing `rb epub` at it — is a small,
  separate change and belongs with whoever does item 13's CLI work.
- **No CLI or TUI surface.** Same reason. The facade is complete enough that a
  frontend is a printing exercise.
- **`identify` is public and read-only** precisely so that surface, when it
  arrives, can show the candidates before the user commits.

## Testing notes

- The suite is offline. Every book *created* in it comes from an ISBN-less epub
  or from a filename, because `import_epub` looks a found ISBN up through the
  providers; the ISBN-matching test seeds the book first, so the match happens
  before anything would be looked up.
- `write_isbnless_epub` moved from `engine_facade.rs` into `tests/common/mod.rs`
  — `book_files.rs` and `workflows.rs` both need it, and two epub builders would
  be two definitions of what a valid epub is.
- The three levels are asserted **separately and by name**. A suite that only
  ever imports the same epub twice tests level 1 and claims all three.
- The property in `files.rs` (`any_bytes_round_trip_to_their_own_address`) exists
  because the failure it rules out — a buffer-boundary bug in the copy loop — is
  about lengths, and lengths are what a generator explores.
- Two test expectations were wrong on the first run and the code was right both
  times: `Path::new("book.../etc/passwd").extension()` is `None` (it is a path
  whose last component has no extension), and `Pachinko (2).azw3` auto-matches
  `Pachinko` on title, so it never became the second book the merge test needed.

## Known gaps, deliberate

- **No garbage collection of orphaned bytes.** There is one way to orphan them
  (kill the process between `store` and `add_book_file`) and no sweeper. A file
  in the store with no row is invisible and costs disk; the fix is a `files gc`
  that lists the store and diffs it against the table, and it wants a CLI to
  live in.
- **No format conversion**, by design — that is item 13's `ebook-convert`.
- **Level 3 on a non-epub is filename-only.** A `.azw3` whose name is
  `B00KF1L3.azw3` matches nothing and, with an empty candidate band, becomes a
  new book named after the file. Calibre's metadata (item 13) is what improves
  this; inventing an azw3 parser here would not.
