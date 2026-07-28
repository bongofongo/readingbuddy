# Goodreads fixtures

Two kinds, and the difference is the whole point.

## `recorded/` — hand-authored, and deliberately so

`CLAUDE.md` says fixtures are generated, not hand-written, because a generator
that reused the engine's own parsing would bake any bug in that parsing straight
into the goldens. These files are the documented exception, for the same reason
the three KOReader checksums in `docs/koreader-format.md` §5 are: **a Goodreads
CSV is a recorded artifact of another system.** Generating one from our own
understanding of the format would prove only that we agree with ourselves.

(The reason lives here rather than in each file's header because CSV has no
comment syntax. Teaching the reader to skip `#` lines would misparse a real
export the first time someone shelves *#Girlboss*.)

They pin the shapes only a real export has, and every one of them was a mistake
waiting to happen:

| shape | file | why it is here |
|---|---|---|
| Excel-armoured ISBNs, `="1455563935"`, **unquoted** | `library-export.csv` | Goodreads writes them this way so a spreadsheet cannot eat the leading zero. It is not CSV quoting, and a bare `normalize_isbn` sees `="1455563935"` and returns `None` — silently losing every ISBN in the file. |
| a row with no ISBN at all (`=""`) | `library-export.csv` | The armour still there, the value gone. `Some("")` is not `None`, and a book keyed on `""` collides with every other book keyed on `""`. |
| an empty `My Rating` beside an explicit `0` | `library-export.csv` | Different cells. Goodreads' `0` means *unrated*; an empty cell means the column said nothing. Neither is a rating, and storing either as `0.0` puts a real "zero stars" on the user's shelf. |
| `Read Count 3` with one `Date Read` | `library-export.csv` | Three readings, one date. See `goodreads::reconcile_readings` for what that collides with. |
| a comma and a double quote inside a title | `library-export.csv` | `The Catcher in the Rye, or "Holden"`. |
| an embedded newline inside a review | `library-export.csv` | A quoted field spanning two lines, which is what breaks a splitter that reads line by line. |
| CRLF line endings | `library-export.csv` | Goodreads writes them; a Unix-only reader keeps the `\r` on the last column of every row. |
| the importer's smaller column set | `importer-columns.csv` | Goodreads' *exporter* writes 24 columns and its *importer* documents 8 — including `Review` where the exporter says `My Review`. Our own export writes the importer's names, so a file we wrote has to be a file we can read. LF endings, to prove the reader is not pinned to CRLF either. |

## `generated/` — the generator's output

`cargo run -p corpus -- gen-goodreads` (or `make goodreads`), committed. Covers
volume and variety rather than shape: many rows, every shelf, ratings across the
whole range, shelf lists, rereads. Deterministic — ChaCha8 from a fixed seed, a
fixed date epoch, never `now()` — so a diff in this file is a change in the
generator and nothing else.

`crates/corpus` does **not** depend on `readingbuddy`, here as everywhere: the
generator writes a CSV the way an exporter would, and the engine reads it the way
a reader would, and neither borrows the other's idea of the format.
