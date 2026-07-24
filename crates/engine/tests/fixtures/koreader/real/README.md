# Drop-in dir for real KOReader exports

Copy your own KOReader library (or individual `<Book>.sdr/` directories) into
this folder to run the import test harness against real data. Anything here
except this README and `.gitkeep` is **gitignored** — your highlights never get
committed.

## Layout

Each book is a `.sdr` directory holding a `metadata.epub.lua` (or
`metadata.pdf.lua`, etc.):

```
real/
  Some Book Title.sdr/
    metadata.epub.lua
  Another Book.sdr/
    metadata.epub.lua
```

You can copy a whole KOReader library subtree — the harness walks recursively.

## What the harness does with it

`crates/engine/tests/koreader_import.rs::real_exports_are_idempotent` runs only
when this dir is non-empty. It:

1. Imports every discovered sidecar into an in-memory DB (books auto-seeded from
   each sidecar's `doc_props.title` so fuzzy matching succeeds).
2. Imports the **same** data a second time and asserts nothing new is inserted
   and no existing highlight/flashcard row changed — the idempotency guarantee.

There is no golden snapshot for real data (content is unknown); only the
counts-and-stability invariants are checked. If the dir is empty the test logs
`skipped` and passes.

Run it directly:

```
cargo test -p readingbuddy --test koreader_import real_exports_are_idempotent -- --nocapture
```
