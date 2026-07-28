# Prompt — Item 9b: the backlinks pane

Paste into a fresh session at the repo root, on branch `feat/tui-backlinks`.

---

Read `docs/spec-08-10.md` (item 9b) and `docs/decisions.md` before starting.

**Depends on 9a** (`Engine::backlinks` / `outgoing_links`) and **rebases onto
8b** — that thread adds a screen and touches the same exhaustive matches, so
rebase after it merges rather than resolving by hand.

**TUI only.**

## Where the pane goes

**On the note list, not a new tab.** Select a note in the book view's Notes
section, press a key, see what links in and what links out.

That reuses the existing list machinery, adds no `BookTab` variant — which would
mean another pass over four exhaustive matches — and puts the pane where the
graph is actually consulted. `docs/decisions.md` is explicit that the reflection
is the hub and that book-to-book connection runs reflection-to-reflection, so the
common case is standing on a reflection and asking what points at it.

## What it shows

Both directions, distinguishable:

- **inbound** — notes whose body links here
- **outbound** — what this note links to, including **dangling targets**, which
  are kept as text on purpose. A wikilink to a note that does not exist yet is a
  zettelkasten forward reference, not an error, and it back-resolves when the
  target is written. Showing it as absent would misrepresent it; hiding it would
  lose it.

Selecting an entry that resolves opens that note. A dangling one is not a dead
end either — it should say what it is rather than doing nothing.

## Constraints

- The key bar in the book view is always present and `o` expands it; never hide
  it entirely, or the view becomes a dead end.
- The pane inherits whatever `book_layout` gives it. Do not add a new orientation
  or a second divider — `t`, `[`, `]` and Tab keep their meanings.
- Nothing modal. Esc backs out one level, as everywhere else.

## Tests

CI runs the whole workspace on ubuntu now, so this suite gates the PR.

- `every_screen_draws_at_every_size` still passes, including the 1x1 case
- inbound and outbound render as different sets
- a dangling target renders and is not mistaken for a resolved one
- selecting a resolved backlink navigates to that note
- a note with no links at all renders something rather than an empty box

## Done when

`make ci` is green, the `cargo-tester` agent reports clean, and the PR body
carries the ASCII dumps from
`cargo test -p readingbuddy-tui -- --ignored --nocapture print_layout print_lists`.
Say what changed and what was deliberately left out.
