---
title: The layout rework — the deep dive, built
date: 2026-08-18
scope: every GUI screen, `docs/decisions.md` entry 53, one engine bug
---

# The layout rework

One session, no wave, no worker threads. `docs/gui/layout-redesign.md` (the
proposal) and `docs/gui/design-applied.md` (the argument that overrides it in 22
places) were drafts from the previous session; this built them and settled both.

## Decisions locked

- **Scope: everything in one pass.** Shell, library, book page, `/notes`, `/life`,
  `/cards`, tokens. The user chose this over phasing.
- **Where the two docs disagree, `design-applied` wins.** The user ruled this up
  front, so `Recent` is dropped, the band is capped at four, `Author`/`Title`
  exclude books with no reading, `/notes`' columns are swapped, `--bg-sunk` is
  rejected, and the accent comes off descriptive text.
- **The latest-mark N+1 is accepted for now** rather than built as a request —
  the user's call. It is `listHighlights` + `listNotes` per open reading, left
  visible in `library/latest.ts` rather than hidden behind a client-side
  aggregate.
- **Four terminal groups on the wall, not one.** The proposal wanted put-down
  readings in the year they were put down; the engine records no such date (see
  gotchas), so the wall ends *Still reading* / *Put down* / *Read, undated* / *No
  reading recorded* — four true statements instead of one invented date.
- **`/notes` chips are the engine's own sources** (All · Notes · Passages), not
  note kinds. Kind is not on `SearchMarks` and a client-side filter over a
  ranked, limited list under-reports silently.
- **The note title ships read-only.** Nothing on the wire renames a note; a
  control that silently did nothing would be worse.
- **The passage list does not claim `role="listbox"`.** A role is a promise, and
  listbox commits to type-ahead, a selection model and non-interactive children —
  all three false here. Plain list, focusable items, one tab stop.
- **Per-page `← Library` dropped from the pages the nav names** (`/cards`,
  `/life`, `/notes`); kept on the leaves (`/book/[id]`, `/book/[id]/cards`).

## Bugs found

- **`slugify` was not idempotent — pre-existing engine bug, fixed.**
  `'İ'` (U+0130, Turkish dotted capital I) lowercases to `i` + U+0307 COMBINING
  DOT ABOVE. `char::to_lowercase` returns *both*, and a combining mark is `Mn`
  rather than alphanumeric — so the mark survived pass one and became a dash on
  pass two, and the same title produced two filenames depending on how many times
  it had been through. Proptest found it mid-session and **committed the
  regression seed**, which turned a latent flake into a deterministically red
  gate. Fix: filter the lowercase expansion to alphanumerics
  (`crates/engine/src/notes.rs:90`). Nothing to migrate — existing notes keep
  their `file_path`.
- **The focus ring failed WCAG AA on the light theme.**
  `:focus-visible` used raw `--accent` (`#c48b3f`), which is the same value in
  *both* themes because only `--accent-text` was ever overridden — **2.78:1** on
  `--bg`, under the 3:1 SC 1.4.11 requires. `app.css` already carried that exact
  number in a comment and had repaired it for *text* only.
- **The dark theme was the one that needed the contrast repair**, which is
  counterintuitive and was the reverse of what the file claimed. `--ink-dim` on
  dark measured **Lc 46.1** against **Lc 74.0** for the same token on light;
  `--accent-text` measured Lc 45.2 against 72.6. WCAG 2's formula is symmetric
  and cannot tell light-on-dark from dark-on-light, so it reads both as fine.
- **The three test viewports were one per device rather than one per layout.**
  1180 and 720 screenshotted the *folded* book page twice and the three-column
  desk **never** — on the wave whose whole subject is the three-column desk. Now
  1440 / 1100 / phone.

## Technical gotchas

- **`abandon_reading` leaves `finished_at` NULL.** A put-down reading is still
  *open* — which is also why `ReadingFilter::open` is not redundant with
  `status`. There is no "put down at" date anywhere on the wire, so a wall
  grouped by finish year cannot place one. Engine item.
- **Opening a note must not close the passage list.** `Cite` needs a note to cite
  *into* and a passage to cite *from*, and only one can be the work surface — so
  the centre's state is **independent** of `openNoteId`. Got this wrong first
  time; the route suite caught it as `Cited in` resolving to zero elements.
- **`opacity: 0` removes nothing from the tab order** — a hover-revealed control
  on forty passages is ~120 stops on invisible buttons. But **`visibility:
  hidden` breaks Playwright**: `click()` checks visibility before hovering, so
  hidden-until-hover controls deadlock the driver. Opacity alone plus a roving
  tabindex is the combination that is both correct and drivable.
- **A `ch` measure is not a character count.** `ch` is the advance of the zero
  glyph, ~20–30% wider than the average glyph in a proportional face, so `68ch`
  renders as ~85–90 characters. `--measure` was renamed `--column` for that
  reason; `--editor` stays in `ch` because a monospace face makes `ch` honest.
- **`--bg-raised` measures Lc 0.0 against `--bg` in both themes.** A selected
  row's accent inset does all the work; the fill contributes nothing
  perceptually. Do not add a third state that leans on it alone.
- **`snippet(…, -1, …)` picks whichever column matched**, and `notes_fts` indexes
  the title beside the body — so a title match returns the title as its snippet
  and printing the title above it draws the same words twice. `/notes` compares
  the flattened snippet to the title, same as `MarkSearch` already did.
- **Svelte trims whitespace around markup**, so a ` · ` separator written between
  two `{#if}`/`{#each}` blocks arrives as `10 h 20 min·410 pages`. A `::before`
  cannot be trimmed. This bit `Months.svelte` exactly as it had already bitten
  `Passages.svelte`.
- **A visually-hidden `h1` needs `textContent`, not `innerText`,** in Playwright:
  `innerText` reads what is rendered and returns empty for the clip-rect idiom.
- **`getByRole(name)` is substring by default**, so `'On The Doorstop'` also
  matched `Cite into “On The Doorstop”` once the passages were back on screen.
  `exact: true` for a row whose title a neighbouring control quotes.
- **The fake client's `listNotes` had no ordering.** The engine sorts
  `created_at DESC, id DESC` and applies `limit` after; the fake returned
  insertion order and ignored `limit`, so `/notes`' *Recently written* would have
  been honest against the engine and wrong against the fixture.

## Verification

- `make ci` exit 0 — fmt, clippy `-D warnings`, `cargo check --workspace`,
  ts-check, whole-workspace test, web-check, routes.
- **545** engine unit tests, **257** vitest (22 of them new:
  `shelf/arrangements.test.ts`, `library/latest.test.ts`, `joinList`), **117**
  Playwright across three viewports.
- **Every desktop screenshot was looked at**, not just regenerated — which is how
  the first passage's controls showing at rest (the `li.active` reveal), the
  `·`-without-spaces separator and the empty `/notes` preview column were caught.
- No Rust API change, so `API_VERSION` and `bindings.ts` are untouched. Two
  client methods gained optional trailing params (`searchMarks(…, source)`,
  `listNotes(…, limit)`), both already on the wire.

## Deferred — all engine items, none of them frontend workarounds

- **A put-down date**, without which the wall cannot group a put-down read by
  year.
- **Latest mark per open reading** — a request, or a field on `OpenReadingDto`.
- **Note rename**, which is what makes the editor's title read-only.
- **Note-kind filtering on `SearchMarks`**, which is what `/notes`' chips would
  otherwise need to fake above the seam.
- **`listBooks` still takes a limit** (the wall asks for 2000). A library past it
  is silently short.
- **A notification-level spec** and **undo before accelerators** — the two
  non-layout items from `design-applied` Part 6, both architectural.
- **Still no dark-theme screenshots anywhere in the suite**, so every contrast
  value above is computed rather than seen. Carried over from item 47's review.
