# Drop-in dir for real PDFs

Copy one or more `.pdf` files here and
`readingbuddy::pdf::tests::a_real_pdf_reports_a_plausible_length` reads every
one of them. Anything here except this README and `.gitkeep` is **gitignored**
— your books never get committed.

## Why this exists at all

`pdf.rs` generates its own synthetic documents, and they cover *shape*: a page
count, a title, an absent `/Info`, a document with no catalog, a truncated
file. What they do not cover is a **cross-reference stream** with the page tree
inside a compressed object stream, which is what essentially every PDF written
since 1.5 actually is — and building one by hand would mean writing a Flate
encoder into a test fixture to prove that a dependency can read its own format.

So the shape coverage is committed and the realism coverage is a drop-in, the
same split `crates/engine/tests/fixtures/koreader/real/` uses and the same one
`corpus/` uses at a larger scale.

## Absence is loud

With no `.pdf` here the test prints

```
SKIPPED: a_real_pdf_reports_a_plausible_length — no .pdf in …
```

and passes. With `READINGBUDDY_REQUIRE_FIXTURES=1` set it **fails** instead.
That is the repo rule from `CLAUDE.md` → Engine standards: a test that returns
silently when its fixture is missing is green without asserting anything, which
`epub.rs` did for months.

Do not set `READINGBUDDY_REQUIRE_FIXTURES=1` globally in CI — this directory is
absent by design there, exactly like the KOReader `real/` one.
