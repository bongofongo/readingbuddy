# Prompt — Item 8b: the currently-reading home screen

Paste into a fresh session at the repo root, on branch `feat/tui-home`.

---

Read `docs/spec-08-10.md` (item 8b), `docs/decisions.md` (the **Design axiom**
and **Reflection and Review** sections) and the TUI half of `CLAUDE.md`'s
Architecture section before starting.

**Depends on 8a** — `Engine::currently_reading` and the `NoteRecord`-returning
reflection accessor are foreign keys for this thread, not preferences.

**TUI only.** Run *before* 9b, and not beside it: both edit the same exhaustive
matches in a 2900-line `app.rs`.

## This thread absorbs item 7's deferred TUI half, deliberately

`docs/decisions.md` item 8 is "currently-reading home screen; **action = open the
reflection**" — and the TUI has no reflection or review surface at all.
`grep -r reflection crates/tui/src` finds one comment. Item 7 shipped
engine-plus-CLI with its TUI half deferred and never numbered.

So item 8 is not implementable without it, and splitting them means two passes
over the same four exhaustive matches for one feature.

## The screen

New `Screen::Home`, and **it becomes what the app opens to**. `m` already returns
to the menu from anywhere, so the menu stays reachable and nothing becomes a dead
end.

- Rows are open readings from `Engine::currently_reading`, shrink-wrapped through
  the existing `list_box` and reusing `library.rs::progress_tag`.
- `Enter` opens the book. A key opens the reflection straight into the existing
  `TextEditor` (`ui/textedit.rs`) — there is no `$EDITOR` path in this crate,
  deliberately, after a broken vim once killed the TUI.
- **No counts, anywhere.** Rows are places you can go. "3 books need
  reflections" is forbidden by the axiom — *no task-completion framing, no badge
  counting what you haven't done* — and it is the easy thing to write on a screen
  like this.
- **The empty state is a place too.** Nothing open must show the way to library
  and search, not a blank pane. `Screen::Home` inherits the ambient layer for
  free (`ambient_visible()` is `motif != Off && screen != Book`), so its list box
  must `Clear` first, like every other non-book screen — a `Block` styles the
  cells it does not draw but never blanks them.

## Reflection and review in the book view

**Keys plus a kind marker in the Notes list. Not two new section tabs.**

`note_line` (`ui/book.rs:295`) already builds an `anchor_tag`, and
`NoteRecord.kind` is loaded and never displayed. The section menu is about
*collections of things*; a reflection is a singleton, and two tabs holding one
item each is the wrong shape for it.

A review carries a rating; collect it through the existing single-line
`ui/input.rs` box and validate with `RatingScale::canonical`, which is the one
quantizer both sides go through. An unmapped value is `EngineError::UnmappedRating`
and must be reported, not rounded.

## The file-by-file cost of a new screen

Listed so it is not rediscovered:

- `Screen` enum and its `Action::Back` arm (`app.rs:37-46`) — navigation is plain
  assignment, there is no screen stack
- `MenuItem` and the `MENU` array, **whose length is part of its type**
  (`app.rs:48-100`)
- the `match app.screen` in `ui::draw` (`ui/mod.rs:374`)
- the `match (self.screen, action)` in `App::handle` (`app.rs:626`)
- both test sweeps, including `every_screen_draws_at_every_size`

If the screen needs a key that is already spent globally, `event::map_key_on`
gives a screen a small override map tried before `map_key` — that is what the
device screen did for `x`/`l`/`r`, and it is preferable to renaming a global
action that reads correctly elsewhere.

## Tests

CI now runs the whole workspace on ubuntu, so the TUI suite gates this PR.

- extend `every_screen_draws_at_every_size` to the new variant. It renders from
  120x40 down to 1x1 and a layout panic there wrecks the user's tmux pane.
- the round trip: home → book → reflection editor → save → home, with the
  reflection appearing in the Notes list marked as one
- the empty state renders and offers a way onward
- opening a reflection twice is the same note — the accretion rule, from the
  frontend this time

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, and the PR body
carries the ASCII dumps from
`cargo test -p readingbuddy-tui -- --ignored --nocapture print_layout print_lists`
so the layout can be judged without a terminal. Say what changed and what was
deliberately left out.

> **Note on `cargo-tester`.** If you are a subagent, you cannot launch it — subagents cannot spawn subagents. Run its procedure directly instead: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`. That is `make check`. Say which you ran.
