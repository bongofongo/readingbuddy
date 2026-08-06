# Prompt — Item 37: a real PDF sidecar, and a `Diagnostic` instead of silence

Paste into a fresh session at the repo root, on branch `feat/engine-pdf-sidecar`,
branched from `main` at the head of the 2026-08-06 non-GUI wave.

---

Read `docs/koreader-format.md` (the **unobserved** section is the whole subject
of this item), `docs/decisions.md`, `crates/engine/CLAUDE.md` and
`crates/corpus/CLAUDE.md`. `CLAUDE.md`'s **Engine standards** section is binding.

**No migration.** Engine + tier-1 corpus + docs. No API, no CLI, no TUI, no GUI.

## What

A PDF-shaped sidecar in the **tier-1** corpus, and a typed degradation where
KOReader's PDF annotations are currently dropped without a word.

## Why

`entry_to_highlight` (`crates/engine/src/koreader.rs:259`) requires a **string**
`pos0`:

```rust
// Modern `annotations` mixes highlights and plain bookmarks; a real
// highlight always carries a pos0 xpointer.
let pos0 = get_str(item, "pos0")?;
```

That comment is true for EPUB and false for PDF: KOReader stores a *table* there
on PDF — page plus coordinates, not an xpointer — so `get_str` returns `None`,
`?` returns `None`, and those entries are skipped **in silence**. No count, no
diagnostic, nothing in the report. A user with a PDF library imports and gets
zero highlights and no reason.

That behaviour is **reasoned, not observed**: `docs/koreader-format.md` files PDF
annotations under *unobserved* for exactly this reason, which is the honest state
and also the reason nobody has fixed it.

Silence is the wrong answer regardless of what the fixture turns out to show. The
engine's rule is that a partial failure returns a `Diagnostic` carrying the path
and an `ErrorClass`, never nothing and never a pre-formatted `String`.

## The two halves, and they are separable

**The fixture.** Tier 1 (`crates/corpus/src/synthetic.rs`, `make synthetic`) is
committed and covers *shape* — and a PDF sidecar is a shape. Tier 2 needs
gutenberg.org, which the sandbox proxy blocks, so shape coverage must never sit
behind a download; this belongs in tier 1 for that reason and not out of
convenience.

**The diagnostic.** `crates/engine/src/diagnostic.rs` — `ErrorClass` gets a new
variant if one is warranted. `EngineError::Other` is last-resort and a caller
that might branch deserves a variant. `Diagnostic`'s `Display` reproduces the old
CLI text byte-for-byte, so what you add is user-visible output; write it as such.
`Diagnostic` deliberately does **not** hold the source `EngineError` (that would
cost `Clone`/`Eq`, which the TUI's status buffer and the golden harness both
need) — so add to `ErrorClass`, do not reach for a wrapped error.

**Decide, and say which:** does a PDF entry become a skipped-with-a-diagnostic, or
does it become an imported highlight with a different anchor? The second is a
larger claim — it means deciding what a table-shaped `pos0` serialises to in the
`pos0` column and what `identity_hash` then does with it, and `identity_hash` is
what makes import idempotent. The first is this item's stated scope. If you
conclude the second is correct, say so and **do not do it here**; it is an item.

## Done when

- `make golden` regenerated, and the goldens show the skipped entries **as
  diagnostics rather than as an absence**. That is the assertion: a golden that
  merely lacks the highlights is the bug, written down.
- The count of imported highlights is **unchanged for every existing fixture**.
  This must be a pure addition. Diff the goldens and say the numbers.
- `docs/koreader-format.md` updated: move PDF out of *unobserved*, or state
  precisely what is still unobserved after the fixture — because a synthetic
  fixture is not an observation of a device, and pretending otherwise is exactly
  what that section exists to prevent. Be careful here; this is the sentence a
  later thread will trust.
- `docs/decisions.md` **appended**.

## Must not

- **Make the fixture by reading a real personal PDF sidecar into the repo.**
  `personal_data/`, the `real/` fixtures and anything on a mounted KOReader
  device stay untouched.
- **Let `crates/corpus` depend on `readingbuddy`.** Reusing the engine's own
  parsing to build its fixtures bakes any bug straight into the goldens, which
  is the whole of `crates/corpus`' value. Check `crates/corpus/Cargo.toml`
  before and after.
- Add a silently-skipping test. Every skip prints `SKIPPED:` and honours
  `READINGBUDDY_REQUIRE_FIXTURES=1`.
- Change what a non-PDF sidecar imports, in any way.
- Run `make corpus` (tier 2). It needs gutenberg.org, which is blocked here.

## Files

`crates/corpus/src/synthetic.rs` (668 lines — the tier-1 generator),
`crates/engine/src/koreader.rs` (`entry_to_highlight`, `parse_annotations`,
`parse_legacy`, and whatever collects the report), `crates/engine/src/diagnostic.rs`,
`docs/koreader-format.md`, the goldens under the import harness.

**Collides with:** item 36 (`koreader.rs`) and item 38 (corpus) — both of which
run **after** you merge, on bases that contain you. You do not coordinate with
either. Item 34 and item 35 touch none of your files.

## How you are gated

**Not `make ci`** — a fresh worktree has no `gui/node_modules`, so `web-check`
and `routes` print `SKIPPED:` and you would "pass" them without running them.

Run **`make fmt lint build-check test ts-check`** plus **`make synthetic`** and
**`make golden`**, and read the exit code properly: never `make test | tail -25`,
which reports *tail's* status. Redirect to a file and read `$?`.

## The one guaranteed conflict

`docs/decisions.md` — **append** your entry and restructure nothing. The file is
in **build order, not numeric order**, deliberately.

## Report the corrections this forced

In the shape `docs/decisions.md`'s existing entries use.

**Push back rather than comply.** Two places this prompt may be wrong: a
synthetic PDF sidecar may encode a `pos0` table shape that a real device does not
produce — in which case the fixture is a fiction that the goldens then enshrine,
and saying so is worth more than the fixture. And the right degradation may be
one diagnostic per *file* rather than one per entry; a 300-highlight PDF that
emits 300 diagnostics has replaced silence with noise, which is not better.

> **Note on `cargo-tester`.** If you are a subagent you cannot launch it —
> subagents cannot spawn subagents. Run its procedure directly:
> `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
> `cargo test --workspace`. Say which you ran.
