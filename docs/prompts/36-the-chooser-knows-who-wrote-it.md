# Prompt — Item 36: the chooser knows who wrote it

Paste into a fresh session at the repo root, on branch `feat/engine-candidate-author`,
branched from `main` **after items 34, 35 and 37 have merged**. That base is
deliberate: this item collides with 33 (api + bindings) and 36 (`koreader.rs`),
and a base that already contains them removes both conflicts. Item 18 had the one
real file collision last wave and merged with **zero** conflicts for exactly this
reason.

---

Read `docs/decisions.md` (item 22 — it decided the band's *membership*, which is
not being revisited) and `crates/api/CLAUDE.md`. `CLAUDE.md`'s **Engine
standards** section is binding.

**No migration.** Engine + API. No CLI, no TUI beyond keeping it compiling, no
GUI beyond the regenerated bindings.

## What

`MatchCandidate` / `MatchCandidateDto` carry the **author**. Consider
`publish_year`. Consider `cover_path` **only** if a chooser would actually show a
jacket — a field nobody draws is weight on a wire.

## Why

`koreader::band` (`crates/engine/src/koreader.rs:636`) already holds the whole
`Book` and throws everything but `book_id`, `title` and `score` away:

```rust
Some(MatchCandidate {
    book_id: s.book.id?,
    title: s.book.display_title().to_string(),
    score: s.score,
})
```

So "which Dune is this" — the **first** screen a refusal sends you to — costs an
N+1 `get_book` per candidate, and a chooser that shows only titles cannot answer
the question it is asking. Two Dunes with the same title and different authors
are indistinguishable in the list that exists to distinguish them.

Reported independently by items 22 and 18, which is what makes it worth an item
rather than a note.

## Every candidate-producing path returns the same shape

`band` is the one constructor today, but check that the shape actually flows
through it everywhere before you assume it: `sidecar_candidates`,
`identify_file`, `match_candidates`, `import_calibre_library` and
`import_goodreads` all surface candidates. If one builds a `MatchCandidate` by
hand, it is a second place your field can be forgotten, and the fix is to make
`band` the only constructor rather than to fill it in twice.

The TUI has ~15 `MatchCandidate` construction sites in tests
(`crates/tui/src/app.rs`, `ui/device.rs`, `ui/calibre.rs`, `ui/goodreads.rs`).
Those are test fixtures, not a design constraint — but `cargo clippy
--workspace --all-targets` gates on them, so budget for the churn and do not let
it push you into a `Default` impl that lets a real call site forget the field.

## Done when

- No caller needs a `get_book` to render a candidate row. Find the ones that do
  today and show they are gone.
- `make ts` regenerated and committed; `make ts-check` clean.
- Whatever you decided about `publish_year` and `cover_path` is written down with
  its reason.
- `docs/decisions.md` **appended**.

Adding fields to a DTO does **not** bump `API_VERSION`. Do not bump it.

## Must not

- **Grow into a matcher change.** The band's membership — what `CANDIDATE_MIN`
  admits, what `can_auto` excludes — is item 22's decision and is not being
  revisited. You are changing what a candidate *carries*, not which books are
  candidates.
- Change `score`, its meaning, or the sort. The `then(a.book.id.cmp(&b.book.id))`
  tie-break above `band` is load-bearing: it is what makes "the best candidate"
  show the same book twice running.
- Hand-edit `gui/src/lib/api/bindings.ts`.

## Files

`crates/engine/src/koreader.rs` (`MatchCandidate`, `band`),
`crates/api/src/dto.rs` (`MatchCandidateDto`), `gui/src/lib/api/bindings.ts`
(**generated — `make ts`**), and the TUI's construction sites.

**You run alone**, on a base containing items 34, 35 and 37. Nothing else is in
flight.

## How you are gated

**Not `make ci`** — a fresh worktree has no `gui/node_modules`, so `web-check`
and `routes` print `SKIPPED:` and you would "pass" them without running them.

Run **`make fmt lint build-check test ts-check`**, and read the exit code
properly: never `make test | tail -25`, which reports *tail's* status. Redirect
to a file and read `$?`.

## `docs/decisions.md`

**Append** your entry and restructure nothing. The file is in **build order, not
numeric order**, deliberately.

## Report the corrections this forced

In the shape `docs/decisions.md`'s existing entries use.

**Push back rather than comply.** One place this prompt may be wrong: the honest
answer may be that a candidate should carry the whole `BookDto` rather than a
hand-picked three fields, since `band` already holds the `Book` and the picking
is what created this bug in the first place. If that is right, say so — the
counter-argument is wire weight on a list that can be long, and it is worth
measuring rather than asserting.

> **Note on `cargo-tester`.** If you are a subagent you cannot launch it —
> subagents cannot spawn subagents. Run its procedure directly:
> `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
> `cargo test --workspace`. Say which you ran.
