# 2026-07-30 — the TUI's `?` pages, half-page paging, and a library sort

Picks up from `267c834` on `main`. Three small frontend asks in one session, all
in `crates/tui`, no engine/api/daemon changes at all. Direct to `main`, no
worktree.

Asked for, in order: a `?` help page per screen; `ctrl-d` / `ctrl-u` paging; a
key that cycles the library's sort order.

## Decisions locked

- **Help is a layer, not a screen.** `App::help` is a plain `bool`, the screen
  underneath keeps being drawn, and there is no `Screen::Help`. A page you
  navigate *to* is a page you have to get back from; a page over the thing it
  describes is not.
- **Any key closes it**, ctrl-c excepted (which also quits). It is *first* in
  `dispatch_key`'s chain, so no key it lists ever fires against a screen the
  user cannot currently see — pressing `d` on the library's page must not queue
  a removal.
- **The split is the design**: a screen's page lists only the keys *that screen*
  gives a meaning of its own. `q` / `m` / `esc` / `j`,`k` / `?` live on the
  **menu's** page and nowhere else — repeated nine times they bury the two or
  three lines that are actually news. `enter` is deliberately *not* on that
  global list: it opens, pulls, imports and adds depending on where you stand.
  The menu's page therefore doubles as the app's introduction, which is right —
  it is the one screen about the app rather than about a book.
- **`?` stopped being an alias for `o`.** It expanded a key bar on the single
  screen that has one and did nothing at all on the other eight, which is the
  worst possible behaviour for the key everyone tries when lost.
- **A row wraps, a page clamps.** `j` off the bottom returning to the top is how
  every list here has behaved and costs one press to undo; `ctrl-d` doing it
  flings you to the far end of the list you were paging *through*. Clamping also
  gives the jump a meaning on a short list: go to the last row.
- **`PAGE` is 12 rows, not the drawn height.** Every list here is
  shrink-wrapped (`list_box` sizes to contents, then to the pane), so "a
  screenful" is a different number per screen, per terminal, and changes under
  the user when a filter narrows the list. A jump that moves a different
  distance each time cannot be aimed.
- **The library sort is a *frontend* sort, not a fifth `BookSort`.**
  `refresh_library` still fetches `BookSort::LastModified` and orders the result
  beside the filter. A SQL `ORDER BY … LIMIT 200` would make the sort key decide
  *which* 200 books are on screen, so `s` would swap the list's contents rather
  than reorder them. It is also the only place `Author` can live at all: "by
  last name" is a parse of a human name and SQLite has nothing to parse one
  with.
- **Articles are not stripped from titles.** A catalogue files "The Overstory"
  under O; this list *shows* the string it is sorting by, and that row landing
  between "Sea of Tranquility" and "Trust" reads as a bug however right the
  cataloguing is.
- **`Sort::Recent` is a no-op**, not a re-sort. The engine already returned the
  list that way; re-deriving it from `Book::last_modified` would be a second
  opinion about an order we were given.
- **Year is descending** (newest first, undated last) — ascending would be the
  one list in this app where the newest thing is at the bottom.

## What was built

- **`crates/tui/src/ui/help.rs`** — `Help { title, about, sections }`, `page()`
  exhaustive on `Screen`, `lines()` (measurable/assertable), `render()`. The key
  column pads to the widest key across the *whole* page, so descriptions line up
  across sections rather than within them.
- **`Action::Help`** on `?`; `Action::PageUp`/`PageDown` on ctrl-u/ctrl-d and on
  the `PageUp`/`PageDown` keys; **`Action::CycleSort`** on `s`.
- **`app::Move { Row(isize), Page(isize) }`** + `Move::land(cur, len)` — the one
  place either the wrap or the clamp rule is written. All eight `step_*` methods
  and the link picker go through it. Paging reaches home, library, search,
  device, calibre, goodreads, the book view's **open section**, the links pane
  and the link picker; deliberately *not* the book's section menu (four rows,
  where a jump lands where one press already does).
- **`ui::library::Sort { Recent, Title, Author, Year }`** with
  `label`/`from_label`/`next`/`apply`, mirroring `ambient::Motif` exactly.
  Persisted as `library_sort` in `tui.toml`; applied at startup via
  `App::set_library_sort` (re-orders in place — `App::new` already fetched, and
  the fetch is by recency whatever the order is).
- **`ui::library::last_name`** — the author parse (see gotchas).
- Border/key-bar/help updates: the library border always names the order, the
  menu advertises `?`, the book view's key bar gained `? help` in both states.

## Technical gotchas

- **`Martin Luther King, Jr.` is not the inverted name form.** The obvious rule
  ("a comma means `Surname, Given`") files it under *Martin*. The fix: a comma
  whose tail is nothing but suffixes is not an inversion — fall through to the
  word path. `Mandel, Emily St. John` and `King, Martin Luther Jr.` both still
  work.
- **Particles must be matched case-insensitively.** `Ursula K. Le Guin` (capital
  `Le`) and `Vincent van Gogh` (lowercase `van`) are the same rule; a
  case-sensitive list gets exactly one of them right. `Emily St. John Mandel`
  is the control: `John` is not a particle, so the surname is the last word
  alone.
- **`sort_by_key` is stable, and that *is* the tie-break.** Every arm can tie
  (two books by one author, two from one year) and the order underneath is
  recency — so within an author the newest is first, for free, with no second
  key.
- **`Reverse` must not wrap the `Option`.** `Sort::Year` keys on
  `(0, Reverse(y))` / `(1, Reverse(0))`: reversing a key that contains the
  present/absent flag would float the undated books to the top.
- **Re-sorting must keep the *book*, not the index.** A cursor that stays on row
  3 while the rows move under it is the app silently choosing a different book,
  and the next keypress might be `d`. `advance_library_sort` reads the id
  before and looks it up after.
- **`persist_config` writes the user's real `~/.config/readingbuddy/tui.toml`.**
  The repo already had a note about this on
  `persisting_carries_every_setting_not_just_the_changed_one`; the first cut of
  the sort test drove `handle(Action::CycleSort)` and would have clobbered it.
  Split into `cycle_library_sort` (= advance + persist) and
  `advance_library_sort` (the half worth asserting), the same shape as
  `config_snapshot` vs `persist_config`. **Tests must not go through the key
  path for any persisting action.**
- **`claimable` refuses any key with Control held**, which is what makes
  ctrl-d/ctrl-u unshadowable by the three import shelves' override maps —
  unlike `s`, which they *do* claim for `Sync` and which is why taking `s`
  globally for the sort is safe.
- **`animating()` was narrowed, not widened.** Adding `&& !self.help` is the
  same kind of guard as the existing `input`/`note_editor`/`api_key` ones (a
  modal is up), not the ambient-layer widening CLAUDE.md forbids. It still
  means "the book is turning".
- **clippy's `empty_line_after_doc_comments`** caught a stray blank line left
  between `cycle_ambient`'s doc comment and its signature after an edit landed
  mid-comment. Green under plain `cargo build` and `cargo clippy` without
  `-D warnings`; only `make ci` failed it.

## Verification

- `make ci` green (fmt + `clippy --workspace --all-targets -D warnings` +
  `cargo check --workspace --locked` + the whole workspace suite).
- TUI suite 271 → **285** tests. New coverage:
  - `ui::help` — every screen has prose + keys; `no_screen_page_repeats_a_global_key`
    (the split, asserted rather than remembered); `no_page_counts_anything`;
    the menu page is the introduction *and* the global table; the key column is
    one width down the whole page.
  - `app` — `?` draws the right page from all nine screens; any key closes it
    and none reaches the screen behind (including `d` on the library); the page
    calms the book and the ambient field.
  - `event` — `?` is help everywhere and `o` is still the key bar; the page keys
    are control-only and no screen shadows them; `s` sorts the library and still
    syncs the three shelves.
  - `app` — `Move::land` wrap vs clamp, including the short-list and empty
    cases; the page keys move every list; the cursor follows the book through a
    re-sort; the config snapshot carries the order beside every other setting.
  - `ui::library` — the name parse on eleven real shapes; all four orders on one
    list; missing author/year sink; ties keep arrival order; the cycle closes
    and every label round-trips.
- `every_screen_draws_at_every_size` now draws each screen's help page too —
  the page is the widest box in the app, so 1×1 is where it would underflow.
- New dev aid: `cargo test -p readingbuddy-tui -- --ignored --nocapture print_help`
  draws all nine pages as text. The pages are mostly prose and prose is the one
  thing a unit test cannot judge.

## Deferred

- **No reverse cycle** for the sort (no shift-`S`). Four orders forward is
  enough; a second key for the same idea can wait for a complaint.
- **The sort is library-only.** The search results, the device shelf and the two
  import shelves keep the order their source gave them — those lists are *about*
  their source's ordering in a way the library's is not.
- **`?` is not advertised on the shelf screens' key bars** (only the menu and the
  book view). Their key bars are already at the width the shrink-wrapped box has
  to accommodate, and `?` is on the menu's page where a lost user is sent.
- **No `BookSort::Author`/`Year` on the engine**, so `rb list --sort` is
  unchanged. The frontend argument above is why; if the CLI ever wants the same
  four, `last_name` moves to the engine and `BookSort` grows two variants that
  sort in Rust after the fetch.
