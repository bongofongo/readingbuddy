---
title: Surfacing items 21/29/30/31/32, and closing the MERGE_RULES debt
date: 2026-08-05
follows: sessions/2026-08-05-items-29-32-engine-wave.md
---

# Session log

Ran `docs/next-thread-handoff.md`. It offered three paths; the user picked **B +
C**, sequential in one thread. No migration, no new engine capability — this is
the CLI and API the last wave deliberately did not build, plus the one debt it
recorded.

## Decisions locked

- **B + C over the GUI wave (A).** The handoff argues it itself: doing the
  surfacing first means the GUI consumes an API rather than inventing one.
  A is now the only remaining path and the handoff says so.
- **Item 30 was in scope, though the handoff's list omitted it.** It listed
  21/31/32 as engine-only. Item 30 had a CLI (`rb enrich`, `rb set`) and **no
  API at all** — same class of gap, found by reading `protocol.rs` rather than
  by trusting the list. `enrich_book`/`set_book_fields`/`field_provenance` are
  now on the API.
- **`Rule::federated` closes C by making the arrangement total.** Not a comment
  at the top of `merge_into` (the handoff's minimum) — a sixth generated thing.
  A new book column no longer compiles without saying how it merges in a
  federated search.
- **The merge body moved to `books.rs`** as `merge_provider_record`, beside the
  table. `search::merge_into` is now the `FieldClaims` bookkeeping and nothing
  else. The alternative — exposing `MERGE_RULES` internals to `search.rs` —
  would have put the table's private shape in a second module.
- **`Federated`'s variants carry their own setter** (`Fill(Take)`,
  `Prefer(Source, Take)`, `WithPair(Take)`, `Local`). So a column that is never
  merged has no dead `fn` pointer beside it, and the type says which columns
  have nothing to copy.
- **`Federated::Local` is pinned by name**, not left as an escape hatch.
  `only_our_own_columns_sit_out_the_federated_merge` asserts the set is exactly
  `["sort_title", "cover_path"]`, so "make it Local and move on" is a decision
  somebody has to make in that test rather than a quiet default.
- **`series_index` with no series is refused in the CLI, not the API.** A rule
  at the seam is a rule the in-process caller never meets — this crate's own
  argument about `dispatch`, applied to a DTO. Documented in both CLAUDE.mds.
- **The refusal checks the *stored* series too.** First cut refused whenever
  `--series` was absent; that blocks renumbering a book already in a series,
  which is the ordinary use of the flag.
- **`Book::series_label` is the engine's**, not each frontend's. Phrasing
  belongs to a frontend; deciding what the pair means together does not, and
  `series_index` is a REAL so two frontends would eventually disagree.
- **`activity --refill` rather than a top-level `refill-events` verb.** No
  importer fills `reading_events`, so an empty log is ambiguous between "nothing
  happened" and "nobody built it". The command that reads the log is where the
  move belongs, and the empty case names it.
- **`ko stats` is its own verb**, matching the engine: arrival is read-only.

## Bugs found

- **I duplicated `trim_index` into the CLI** while adding the `set` refusal —
  the exact drift I had just moved it out of `books.rs` to avoid. Caught before
  commit; it became `readingbuddy::series_index_text`, `pub`, used by
  `Rule::show`, `Book::series_label` and the CLI's refusal line.
- **A test I wrote asserted the wrong thing about the fixture, not about the
  code.** `an_empty_activity_log_…` expected `inferred` rows from
  `Gen-Summary.sdr`. The readings filler stamps `measured` with source
  `koreader`, and `confidence` ratchets to `measured` and never back, so those
  rows are `measured` — on the same `(book_id, day, source)` key as the
  highlight filler's. Rewritten to assert the sharper case: **a `measured` day
  with no minutes**, which is the one a renderer is most likely to print `0`
  for.
- **A `capture()` helper I wrote did not capture anything** — it re-ran the
  formatting itself. Deleted; `print_summary` became `summary_text -> String`,
  following `render.rs`, which is what makes the block assertable at all.

## Technical gotchas

- **`MERGE_RULES` is a `const`, so `MERGE_RULES.iter()` materialises a
  temporary.** You cannot return a `&'static Rule` from a lookup helper — the
  existing `field_pair`/`field_value` read *through* the expression for exactly
  this reason. The nested pair lookup inside `merge_provider_record` follows the
  same shape.
- **`merge_into`'s ISBN pair is independent; its series pair is not.**
  `isbn_10`/`isbn_13` are each `Prefer(OpenLibrary)` and move separately in the
  federated merge, while `series_index` never moves without `series`.
  `Rule::pair` means "user-ownership of either half protects both" in the
  *storage* merge — a different question from `Federated::WithPair`. Do not
  conflate them; the generated code checks the pair's own `Federated` before
  moving it.
- **`merge_provider_record` assigns the paired index unconditionally but claims
  it only when present.** A provider naming a series with no index must *clear*
  a stale index belonging to a different series; a source is not the origin of
  a NULL, so no claim is recorded. This reproduces the hand-written behaviour
  exactly and `a_series_index_travels_with_the_name_or_not_at_all` pins all four
  cases.
- **Claim ordering changed and nothing depended on it.** The generated loop
  walks `MERGE_RULES` order rather than the old statement order; `FieldClaims`
  is a keyed `Vec` with replace-on-repeat and its only iterator is the
  `#[cfg(test)]` drift guard, which checks membership.
- **`format!("{}", 2.0_f64)` already prints `2` in Rust.** `series_index_text`
  is defensive rather than load-bearing for the common case — kept and reused
  rather than relitigated.
- **`clippy::collapsible_if` fires on nested `if let`** in this toolchain; the
  fix is edition-2024 `let … && let …` chaining, which the repo already uses.
- **`DayRange` validation belongs in the `Api` method**, not passed as two
  strings to the engine. `activity_summary`/`activity_by_day` take `&DayRange`,
  so the seam has to construct one — which is what makes an inverted span an
  `InvalidInput` instead of a confident zero on the wire.
- **`FillStats` was not exported** from `crates/engine/src/lib.rs` (only from
  `storage/mod.rs`), so the DTO could not name it. Added to the `pub use`.
- **The CLI's `--help` subcommand list is a golden.** `the_subcommand_set_is_what_we_decided`
  in `crates/cli/tests/cli.rs` must be updated in the same commit — `activity`
  and `toc` went in; `ko stats` is a sub-subcommand and does not appear.

## Verification

- `make ci` → **exit 0**. Engine lib **323** (was 320), TUI 291, API 20
  integration + 18 unit, CLI 14 unit + 12 integration.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` clean.
- `cargo check --workspace --locked` clean — the build where the `internals`
  feature is off.
- New tests, all of which fail if the rule they name is broken:
  - `every_provider_column_survives_a_federated_merge` — every `MERGE_RULES`
    column either merges and is claimed, or is `Local`.
  - `only_our_own_columns_sit_out_the_federated_merge`
  - `a_series_index_travels_with_the_name_or_not_at_all`
  - `an_index_is_refused_only_when_no_series_names_it`,
    `the_echo_names_the_pair_and_the_subjects`
  - `nothing_measured_is_never_printed_as_zero`,
    `a_period_nobody_measured_says_so_rather_than_reporting_zero` (also asserts
    no streak/goal/"of 31" framing appears)
  - `a_period_with_no_device_data_has_no_minutes_rather_than_zero`,
    `a_backwards_range_is_refused_rather_than_answered`,
    `refilling_the_log_is_idempotent_across_the_seam`,
    `a_book_with_no_file_has_no_chapter_list_to_read`,
    `a_correction_crosses_as_the_users_and_is_recorded_that_way`
  - `an_empty_activity_log_names_the_move_and_a_refill_measures_nothing_it_did_not`,
    `a_book_with_no_epub_says_there_is_no_file_rather_than_no_chapters`,
    `set_writes_the_series_pair_and_refuses_half_of_it`

## Deferred

- **The TUI sees none of this.** No screen for the activity log, the chapter
  list or field provenance. Deliberate — the GUI wave's item 17 is the
  derived-facts layer and a TUI screen built first would be the second dialect.
- **No bulk `enrich`**, still — the per-book cost is a provider fan-out and a
  loop over the shelf is a rate-limit policy nobody has decided.
- **`rb activity --book` is not range-filtered.** A book's whole history is the
  interesting length and it is bounded by how long the book took; a `--from`
  that silently applied to one view and not the other would be worse.
- **`set` cannot clear**, so renaming a series leaves a stale index. The command
  now *says so* rather than fixing it — clearing needs a statement that can
  write NULL and a way to say "I mean it", which no caller has asked for.
- **The day skew between the two KOReader sources** (item 31) is untouched and
  still correct to leave alone.
