---
title: The GUI layout, redesigned — the shelf you look back on and the desk you work at
date: 2026-08-08
status: **built and settled.** What shipped is `docs/decisions.md` entry 53, and
        where this file and that entry disagree the entry is right — it records
        the divergences, of which the load-bearing ones are: put-down readings
        could not be grouped by the year they were put down (the engine records
        no such date), `/notes`' column emphasis is swapped, `Recent` was
        dropped, and the chips are the engine's own sources rather than note
        kinds. `docs/gui/design-applied.md` overrode this file in twenty-two
        places and won every one of them.
source: `docs/gui/gui-vision.md` for the product argument and the axiom this is
        held against; `docs/gui/spec-gui-17-28.md` for the engine underneath it.
        The clickable prototype is `docs/gui/layout-prototype.html` — open it in
        a browser, it needs no server and no build.
---

# The layout

## What this is, and how to read it beside the prototype

The prototype is the argument; this file is the handoff. Every region named
below exists in that HTML, at a hash route, at three widths, in both themes, and
now in **six whole-app arrangements** — see "The six arrangements" below.
Where the two disagree, **the prototype is right about proportion and this file
is right about behaviour** — the HTML has no data layer and cannot express a
failure state, and this file cannot express how much air a shelf needs.

The prototype's top strip is monospace, sunk and hairlined off from everything
below it on purpose: **nothing in that strip is a design proposal.** It jumps
between routes, flips the theme, turns tile captions on, and swaps in the empty
library. It is scaffolding and it does not ship.

## The six arrangements

The strip's **`layout ⟳`** button (shortcut: **L**) cycles six arrangements of
the whole app. They are not knobs and not a settings surface: **the shell, the
library and the desk move together**, because a nav that has become a column
changes what the desk's own rails mean. Same markup, same data, same tokens —
each variant is `html[data-layout=…]` and nothing else, which is why the state
survives a route change and why no variant can add a component the others lack.

| # | name | what it argues |
|---|---|---|
| 1 | **open** | the proposal in this document: top bar, wall of years, three-column desk |
| 2 | **rail** | a fixed nav column, and the desk's index folded into a row of chips above the work |
| 3 | **focus** | one reading proud on the home surface, the wall very small, both desk rails stacked to the right |
| 4 | **column** | one measure, everything stacked, nothing sticky |
| 5 | **panels** | structure carried by raised cards with edges rather than by air |
| 6 | **split** | the reading column stays put while the shelf scrolls; on the desk all context goes left and the work is one wide surface |

The order is deliberate: 1 is this document and each of the others is a named
argument against a specific part of it, so pressing **L** repeatedly walks the
disagreement rather than a preference list. What each is actually testing:

- **rail** buys the whole window for the work and makes "where you are"
  permanent, at the cost of the thing the no-sidebar argument was protecting — a
  permanent edge. Look at whether the library still reads as calm beside it.
- **focus** is the strongest reading of "calming": the home surface *is* the book
  you are in, and the shelf is texture behind it. It is also the variant with the
  widest work surface, so it is the fairest test of whether the left rail needs
  to be a column at all.
- **column** is the only variant where the note editor and its links are never on
  screen together — which is precisely what the third column was built to buy. It
  is here so that claim can be checked rather than asserted.
- **panels** is the opposite bet from the baseline and the most likely to read as
  an admin console rather than as a place. That is why it is on the list instead
  of being assumed away.
- **split** is the only variant that puts what you are reading and what you have
  read on screen at once, permanently.

**The variants are scoped to `min-width: 1181px`.** Below that the responsive
rules documented under each page are the design for all six. A layout proposal
is an argument about a wide window, and letting one of them win on a phone would
be reviewing a bug rather than a layout.

Everything the axiom forbids is still forbidden in every variant — no aggregate
on a home surface, no badge, no count in the shell. A variant that needed one
would be disqualified rather than accommodated.

## The brief this answers

Stated so the reasoning can be checked rather than taken on trust:

- The grid library with a currently-reading section is right. The books are too
  large and take too much of the screen; there should be **less happening and
  more whitespace**. Opening the app should be calming — and still carry enough
  that a reader can look back over the books and feel they did something.
- Currently-reading books should carry **more information and more of a
  preview** than the rest.
- The **single book page is where the time goes**: writing notes, reviews and
  reflections, and connecting them to each other. **Exploring notes is a central
  activity**, not a subsidiary one.
- **No moving parts, and no 3D**, in this phase.
- The colour scheme stays.

Two of those pull against each other and the resolution is worth stating,
because it decides most of what follows. "Calming, less happening" and "the
place where the work is done" are not the same room. So they are not the same
page: **the library gets quieter than it is today and the book page gets much
bigger.** The shelf is where you look; the book is where you work. Nothing is
added to the library to make it useful, because being useful is not its job.

---

# The shell

A single row at the top of the window, `max-width: 1400px`, centred.

```
readingbuddy                        Library   Notes   Cards   Reading life
```

- The wordmark is `--accent-text` and links to the library. It is the app saying
  where you are, which is why the library needs no page heading of its own.
- Four nav items, `0.85rem`, `--ink-dim`. The current one is `--ink` with a
  1px `--accent` underline. **`aria-current="page"`, not a class** — the state
  is real and assistive tech should get it.
- **No sidebar.** A permanent left column is a permanent edge, and the calmest
  surface is the one with the fewest of them. Four text links do not need a
  column, and the rails that *do* exist (on the book page) are then unambiguous:
  a rail means "you are working", not "you are navigating".
- **No count, no badge, anywhere in the shell.** Unchanged from today and
  non-negotiable — `docs/decisions.md` forbids task-completion framing by name.

**Divergence:** the nav gains **Library** and **Notes**. Library because the
wordmark alone as a home link is discoverable only by guessing; Notes because
the vault is being promoted to a place (see below).

---

# Page inventory

| route | name | what it is | today |
|---|---|---|---|
| `/` | Library | the home surface: three previews and a quiet wall | exists, redesigned |
| `/book/[id]` | The book — passages | the desk, work surface showing the passage list | exists, restructured |
| `/book/[id]?note=N` | The book — writing | the desk, work surface showing one note | exists as a band; **promoted to the work surface** |
| `/notes` | Notes | the vault as a place: search, results, preview, links | **new** |
| `/cards` | Cards | the wall of reading cards | exists, restyled |
| `/book/[id]/cards` | Cards for one book | unchanged in structure | exists |
| `/life` | Reading life | the one page counts live on | exists, restructured |

`?note=N` stays a query parameter rather than becoming `/book/[id]/note/[nid]`.
It already works, it already survives a reload, and it is already how a moment
lands *in* the reflection. A second route would be a second thing to keep in
sync for no gain.

---

# `/` — the library

Two bands, in this order, and nothing else on the page.

## Band 1 — "Reading now"

`repeat(auto-fit, minmax(310px, 1fr))`, `gap: 1.25rem 2rem`. One to four across
depending on width. **Not a horizontally-scrolling strip** — see the divergences.

Each preview is `grid-template-columns: 68px minmax(0, 1fr)` with `1.1rem` gap:

| region | content | source |
|---|---|---|
| jacket | 68px wide, `aspect-ratio: 2/3`, links to the book | `coverSrc(book)` → `Jacket` fallback on absence |
| title | `0.95rem`, links to the book | `titleLabel(book.title)` |
| author | `0.82rem` `--ink-dim` | `authorsLabel(book.authors_display)` |
| progress rail | 2px track, `--line`; fill `--accent` at the percentage | `book.progress` |
| progress line | **p. 214** of 502 · 43% · started 12 March | `progressDetail(book.progress)`, `readingSpan(reading)` |
| latest mark | label + one to two clamped lines, quoted italic for a passage, the note's title in italic for a note | see the gap below |
| actions | `Write` → `?note=`, `Passages` → the book | — |

**The latest mark is the whole point of this band** and is what "more of a
preview" means. It is the reader's own material, which is the only content that
earns the space — a blurb would not. Label it `LATEST PASSAGE` or `LATEST NOTE`
by which is newer.

**The number rule holds here and is worth restating** because this band carries
more digits than anything else on the home surface. `p. 214 of 502 · 43%`
describes **one book you chose to open**. It never describes the shelf. There is
no "3 books in progress", no total, no aggregate anywhere on this page.

**States.** No open readings → the band renders nothing at all, no heading and
no empty message. An open reading with no highlights and no notes → the mark
block is omitted, not replaced with "nothing yet". A reading with no page count
→ the rail is omitted and the line degrades to `p. 214 · started 12 March`;
never `p. 214 of 0`.

## Band 2 — "The shelf"

Heading left, a four-way arrangement switch right: **Year · Author · Title ·
Recent**. Default **Year**.

Below it, groups. Each group is a small uppercase heading with a hairline rule
running to the right margin, then a grid:

```
grid-template-columns: repeat(auto-fill, minmax(86px, 1fr));
gap: 1.9rem 1.4rem;   /* the row gap is deliberately larger than the column gap */
```

- **86px minimum, not 150.** This is the single biggest change on the page and
  it is what "too large, too much of the screen" asked for. At 1440 it gives
  eight or nine books a row instead of five, which is the density a shelf
  actually has.
- **The row gap exceeds the column gap.** Books on a shelf sit close together
  horizontally and shelves are far apart vertically. Equal gaps read as a
  spreadsheet.
- **Captions are off by default.** A tile is its jacket. The title and author
  are on the `title` attribute and in the accessible name, and the prototype's
  `captions` toggle shows what the alternative looks like — it should be a real
  setting eventually, not a hardcode. A book with no cover keeps the
  typographic plate, which already carries its title, so nothing is unnamable.

### Grouping by year, and why it is the default

Grouping the wall by **the year a reading closed** is the cheapest thing on this
page and does the most work. It is past tense, it contains no target, and
scrolling from 2026 down through 2024 is the "look back and feel accomplished"
the brief asked for — delivered with no digits, no streak and no badge. A count
would have said the same thing worse.

The group key is **the reading's finish year, not the book's publication year.**
The TUI's `Sort` has a `Year` that means publication; if item 17a's engine sort
keeps that name, this needs a different one. Name it explicitly rather than
inheriting the ambiguity.

Books with no closed reading land in a final group headed **"No reading
recorded"**. That is a statement of fact, not a queue: no count, no call to
action, no styling that marks it as unfinished business. Goodreads `to-read`
arrives as zero readings and this is what zero readings looks like.

**Put-down readings appear in the year they were put down, styled identically
to finished ones.** This is a real decision and the vision doc left it open
(`gui-vision.md`, "one open question, deliberately left open"). The prototype
takes the position because a shelf had to be looked at to decide, and looking at
it, hiding them was worse. **It is still the thing most worth arguing about
before this ships** — see Open questions.

**States.** Empty library → no bands, no headings; one line of fact and the two
importers that need no network, exactly as today. Loading → one dim line. The
library failing to open is this page failing; the reading band failing is not,
and must not replace the shelf (this is already the rule in `routes/+page.svelte`
and it survives the redesign intact).

---

# `/book/[id]` — the desk

Where the time goes, so it gets the room.

## The header

```
← Library
[52px jacket]  The Overstory
               Richard Powers · 2018 · 502 pages · Reading · p. 214 · 43% · started 12 March
```

One line of metadata, `--accent-text` on the state and progress fragment.
Hairline rule underneath.

**Divergence, and the most consequential one on this page:** the hero jacket
goes from **150px to 52px** and the two-line stacked identity becomes one line.
That is roughly 130px of vertical space, at the top of every book page,
recovered for the surface people actually work on. A book page is not a product
page; you already know which book you opened.

## The three columns

```
grid-template-columns: 15rem  minmax(0, 1fr)  19rem;
gap: 0 2.6rem;
align-items: start;
```

Both rails are `position: sticky; top: 1.5rem`. The centre is capped at
`--measure-wide` (82ch).

### Left rail — the book's index

Sticky. Three sections, each with a `.band-title` heading:

1. **Write** — `Note`, `Reflection`, `Review`. Always visible, at the top,
   above everything. Today these are only reachable when no note is open, which
   means the act the page exists for is hidden exactly while you are performing
   a neighbouring one.
2. **What you wrote** — every note for this book in **one list**, kind chip
   first (`REFLECTION` in accent, `REVIEW` in `--ink-dim`, plain notes
   unchipped), title truncated to one line. Selecting one sets `?note=`. The
   selected row gets `--bg-raised` and a 2px accent inset on its left edge.
   **One list, not four tabs** — the TUI's ruling and it stays right: a tab
   holding the single reflection is the wrong shape for a thing there is exactly
   one of.
3. **The book** — `Passages`, `Reads`, `About & sources`, and `Cards →`. These
   switch the centre column. `Cards →` leaves for `/book/[id]/cards`, and the
   arrow is what says so.

The rail is what makes the centre swappable without anything being modal: every
other destination is on screen while you are in any one of them.

### Centre — the work surface

Three states, driven by the rail.

**`passages`** (default). The passage list, `--measure-wide`. Each passage:

- the text as a `blockquote`, `0.95rem/1.65`, 2px left rule
- the rule is `--accent` when some note quotes it, `--line` otherwise
- the annotation, if any, in a `--bg-raised` block beneath
- a meta row: `p. 214`, `quoted`, `2 cards captured`, then the actions
- **the actions — `Annotate`, `Capture a word`, `Cite` — are `opacity: 0` until
  the passage is hovered or contains focus**, and always visible under
  `@media (hover: none)`. Three buttons repeated down forty passages is exactly
  the "too much happening" the brief objects to, and the passage is the content;
  the controls are not.

`Cite` is only meaningful with a note open. When none is, it should read as a
prompt to open one rather than silently doing nothing — do not disable it
without saying why.

**`note`**. The editor, and it is the surface this whole redesign is for:

- title, inline and editable, `1.05rem` semibold on a bare bottom rule — see the
  API gap, this may have to be read-only in v1
- the body: monospace, `0.9rem/1.75`, `min-height: 26rem`, `--bg-raised`,
  resizable vertically. **Markdown as markdown, never a rich-text editor** — the
  file in the vault is the origin and Obsidian is the other thing editing it.
- a bar beneath: the save status on the left (`Saved · in your vault as
  <code>trees-as-a-time-scale-argument.md</code>` — naming the file is the app
  telling you it did not capture your writing), then `Delete`, then `Save` as
  the one primary button on the page.
- for a `review` only, the rating row beneath. Unchanged in behaviour.

Compare what this replaces: a `rows="10"` textarea inside a 68ch column inside a
band inside the left half of a two-column page. Same component, three times the
room, and the connections now sit beside it instead of behind a button.

**`reads` / `about`**. The reading rows and the `About` component as they are
today, moved out of a permanent sidebar into the centre. They are reference; they
should be reachable and they should not be occupying a column full-time.

### Right rail — connections

Sticky. **This rail is the layout answer to "connecting notes with other
notes."** Its contents depend on the centre.

When **a note is open**:

1. **Links** — `3 out · 2 in`, then the merged list. `→` for outgoing, `←` for
   incoming, direction carried **in the text and not in a colour** (survives a
   theme, a high-contrast mode, and a colour-blind reader; and a test can assert
   on it). A dangling target is `--ink-dim` with a quiet `no note` beside it —
   not an error, not a warning, not a fix-me. It resolves itself the day that
   note is written.
2. **Link to…** — a search box over **every note in the vault**, and each result
   inserts `[[Title]]` at the cursor. This is new, it is the most valuable single
   region in the redesign, and it is buildable today: `searchMarks(q, null, n)`
   already accepts a null book id.
3. **Cited passages** — the passages this note quotes, with a line telling you
   citation is done from the passage list.

When **passages are shown**: a `Search this book` box (`searchMarks(q, id, n)`,
which is today's `MarkSearch` moved into the rail where it belongs) and the
`Reads` summary as a two-line readout.

**Divergence:** the links pane stops being a *depth* you navigate into by
pressing a button, and becomes a *region* that is simply present while you
write. The old arrangement was right when the links pane had to replace the note
list in a single narrow column — it was the honest way to avoid a modal. With a
third column there is no longer anything to trade off, and a graph you can see
while writing into it is a different tool from a graph you have to go and look
at.

## Responsive

Two breakpoints, and they drop in this order:

- **≤1180px** — the right rail unsticks and folds under the centre, above a
  hairline. The left rail stays: navigation survives longer than context.
- **≤860px** — the left rail unsticks and stacks above the centre. Page padding
  drops from 2rem to 1.25rem.

*(Implementation note: `.rrail` carries `grid-column: 2` in the 1180 rule. The
860 rule must reset it to `auto` or it conjures an implicit second column and
the "one column" layout silently renders as two. This was a real bug in the
prototype, found by screenshotting at 800px.)*

---

# `/notes` — the vault as a place

New page. The engine has had `notes_fts` since migration `0001` and it has had
no interface outside the CLI; the brief makes note exploration central, and a
full-text index over everything you have ever thought does not become central by
living inside one book's page.

```
[ big search box, 46rem, 1rem type ]
[ All · Notes · Reflections · Reviews ]   full text, every note in the vault, and the highlights too.

grid-template-columns: minmax(0, 68ch) 22rem;   max-width: calc(68ch + 25rem);
```

- **Left: results.** Title, kind chip, book name pushed right, then a two-line
  snippet with the query terms in `<mark>` at 30% accent. Selecting a row marks
  it with `--bg-raised` and an accent inset.
- **Right: the focused note.** Title, `book · 3 out · 2 in`, the body faded out
  under a mask at 15rem, `Open` and `Go to the book`, then the same Links list
  as the book page's rail.

The `max-width` on the grid matters: with a bare `1fr` first column the two
halves sit at opposite ends of a 1440px window with dead air between them, which
is what the first render did.

**Counts are allowed here.** `/notes` is a page you chose to open, which is the
condition `gui-vision.md` sets. `3 out · 2 in` are edges that exist, stated in
the past tense. Nothing on this page counts something you have not written.

**Deliberately absent: a dangling-links index.** "Notes waiting to be written"
is an orphan queue with better manners, and `decisions.md` rules the orphan queue
out by name. A dangling target is visible wherever it is *linked from*, which is
where it is actually meaningful. Do not add a global list of them, and if someone
asks for one, this paragraph is the answer.

**States.** No query → "Recently written", same layout. No results → one line
saying nothing matched, and the box stays focused. Empty vault → the two moves
that fill it (`rb note`, or open a reflection from any book).

---

# `/life` — reading life

```
grid-template-columns: 9rem  minmax(0, 1fr);
```

- **Left:** a sticky year list from `readingYears(null)`. Selected year gets the
  accent inset.
- **Right:** a `7rem` month label beside a prose block, one row per month,
  hairline between.

Each month is written as sentences, not as a stat block: *"Finished **Checkout
19** and **The Employees**. Wrote **9 notes** and made **14 links**."* Bold on
the figures, `--accent-text` never — accent is reserved for state on this page.
Book covers finished that month appear as 40px jackets beneath (with jacket
typography suppressed at that size; it is illegible and looks like a rendering
fault).

**An absent fact gets its own italic dim line, and says what is absent:** *"No
reading time recorded — the device has not been connected this month."* Never a
zero. This is `reading_events`' own discipline surfaced verbatim — a month with
no device data returns absent minutes, not zero, and zero is a claim.

**A month in which nothing was finished says nothing about finishing.** The
prototype originally opened those months with "Finished nothing." — which is a
deficit sentence wearing a fact, and it is the exact failure mode this app is
built to avoid. The page states what happened and is silent about what did not.

---

# `/cards` — the wall

`repeat(auto-fill, minmax(258px, 1fr))`, `gap: 1.6rem`. Each card: a 48px
jacket, title and author, a rule, the span and the rating, then one passage in
italic behind a 2px left rule. One card per **reading**, so a reread has two side
by side; a put-down reading gets a card that says so plainly
(`put down 3 May 2026 · p. 201`) and is not styled as a lesser object.

Structurally unchanged from what exists. Included in the prototype so the wall
can be judged against the new tile scale, not because it is being redesigned.

---

# Tokens

The palette is settled and nothing here invents a hue. `app.css` is copied
verbatim into the prototype. **Three tokens are added**, and each earns it by
being used on more than one screen:

| token | dark | light | why |
|---|---|---|---|
| `--bg-sunk` | `#100f15` | `#f2efe8` | one step below `--bg`. Used by the prototype chrome only, today; reserved for a future settings surface. **If nothing in the shipped app uses it, do not add it** — a token is a promise every screen will use it. |
| `--measure-wide` | `82ch` | — | the work surface. `--measure` (68ch) is a prose measure and is right for reading; a monospace editor and a passage list want more. |
| `--rail` / `--rail-r` | `15rem` / `19rem` | — | the two rails, named so the breakpoints read as a decision rather than as magic numbers. |

`--accent-text` vs `--accent` discipline is unchanged and still applies: brass
measures 2.78:1 on the light background, so anything that is body text uses
`--accent-text`. The selected point of a segmented control, a primary button and
a progress fill are surfaces and use `--accent` with `--accent-on` labels.

---

# Divergences from what is built today

Per the clean-slate decision. Each is a thing to tear out, with the reason.

1. **The reading strip becomes a grid.** Today it is a horizontally-scrolling
   `grid-auto-flow: column` band with a mask and scroll-snap, bleeding to the
   window edge on `--bg-raised`. It goes. Horizontal scroll hides books behind a
   gesture, and the raised ground plus the mask plus the snap is three mechanisms
   holding up a band that will usually contain two items. A plain wrapping grid on
   the page's own ground is quieter and shows everything.
2. **Tiles shrink from ~150–200px to 86px minimum, and lose their captions by
   default.** The direct answer to "too large, too much space".
3. **The shelf groups by year.** Today it is one ungrouped grid with a
   cover/rows layout switch. The `ShelfSwitch` layout choice is replaced by an
   arrangement choice (Year · Author · Title · Recent). The `Rows` layout is
   dropped — a shelf of rows is a list, and the list is what the tile grid
   already is at a smaller size.
4. **The book hero drops from 150px to 52px** and the identity block becomes one
   metadata line.
5. **The book page goes from two columns to three**, and the split changes
   meaning: today it is *what you wrote* beside *what is known*; now it is
   *where you navigate*, *what you are doing*, *what it connects to*. `About` and
   `Reads` move out of a permanent sidebar into the centre, reachable from the
   rail.
6. **The notes band becomes the left rail plus the whole centre column.**
   `NotePane`'s three-depths-in-one-pane structure dissolves: the list is the
   rail, the editor is the centre, the links are the right rail. All three are
   visible at once, which is what the third column bought.
7. **`MarkSearch` moves into the right rail** from above the two bands.
8. **The write actions become permanently visible** instead of appearing only
   when no note is open.
9. **Passage actions become hover-revealed.**
10. **`/notes` is new.**
11. **The nav gains Library and Notes.**

Nothing above changes a single line of engine behaviour, and none of it touches
the axiom — which is the test each one had to pass.

---

# What the API can and cannot serve

Checked against `gui/src/lib/api/client.ts` rather than assumed.

**Already there, no engine work:**

- vault-wide search — `searchMarks(query, null, limit)` already takes a null book
  id, so `/notes` is buildable today
- all notes regardless of book — `listNotes(null)`
- the year list and per-year readings — `readingYears`, `listReadingRows`
- month facts and their absences — `activityByMonth`, `activitySummary`
- two cover tiers — `coverSrc` (shelf) and `heroSrc` (the book page)
- links both directions — `outgoingLinks`, `backlinks`
- citations, per note and batched — `citationsFor`, `citationsForNotes`

**Gaps. Each is an engine item, never a frontend workaround:**

1. **No "latest mark for a book".** The reading preview needs the newest of
   (highlight, note) per open reading. Today that is `listHighlights(id)` +
   `listNotes(id)` per book — every highlight in the book fetched to display one
   line, times up to four books. It works and it is wrong. Wants a request, or a
   field on `OpenReadingDto`. **Run `api-surface-auditor` on this before
   building the band.**
2. **No note rename.** `updateNoteBody` exists; nothing sets a title. The
   editable title in the prototype is therefore aspirational — either add the
   request or ship the title read-only in v1 and say which.
3. **`listBooks` defaults to `limit = 200`.** The shelf wants the whole library.
   This is item 18 and it is a real blocker for a large library, not a polish
   item.
4. **The year grouping key.** Confirm whether item 17a's `BookSort::Year` means
   publication year. The shelf needs *the reading's finish year* and they must
   not share a name.

---

# Open questions — decide these before building

1. **Do put-down readings belong on the wall?** The prototype says yes,
   unremarked, in the year they were put down. `gui-vision.md` deliberately left
   this open to be decided against a real shelf. This is that shelf; look at it
   and rule. Hiding them implies abandonment is failure, which the axiom rejects;
   showing them identically to a finished book is arguably a lie about what
   happened.
2. **Captions on or off by default, and is it a setting?** Off is calmer and is
   what the brief asks for. On is more scannable and much better for a library
   full of similar-looking jackets. The prototype toggles it so both can be seen
   at real density.
3. **Is `Recent` still needed** in the arrangement switch once Year exists? Year
   already puts the most recent finishes at the top.
4. **Does the centre column need `About`/`Reads` at all**, or does the metadata
   line in the header plus the right rail's Reads readout cover it? Dropping them
   would remove a state from the work surface.
5. **`--bg-sunk` earns its place only if the shipped app uses it.** It exists in
   the prototype for the scaffolding strip, which does not ship.

---

# Deliberately not here

So it is not quietly added later on the grounds that it was merely forgotten:

- **Any motion.** No transitions, no animation, no camera. Requested explicitly.
- **Any 3D.** The shelf is jackets, not spines, and item 19 and item 26's WebGL
  shelf are untouched by this document — deferred, not cancelled. The layout
  above is what the app looks like until they land.
- **A graph view.** Out of scope in `decisions.md` and it stays out. The Links
  rail plus a reflection's reach is what the graph view was actually wanted for.
- **A dangling-links index**, per the reasoning under `/notes`.
- **Note tags.** Wanted, not this phase.
- **Any aggregate on the home surface.** Not a stylistic preference — the rule.
