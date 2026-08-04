---
title: The GUI, and what a reading life looks like when it is a place
date: 2026-08-04
source: this document is the argument; the settled lines are at the bottom and
        belong in `docs/decisions.md` once they stop moving
---

# The GUI

## What the brief said, and what survives it

The brief:

> a place where readers and their notes can live — they can track their reading,
> set goals, earn rewards, and take plentiful notes which talk to each other,
> and there are lots of "congratulatory" things that happen when books are read.
> It is a collection space where readers can display the books they've read and
> their thoughts on them. It is a place for the readers' *data* to live — it
> becomes the source of truth for the reader on their reading life.

Six claims. Five survive intact. One does not, and it is worth being precise
about which, because `docs/decisions.md` already forbids it in writing:

> **No task-completion framing.** No inbox, no badge counting unwritten
> highlights. Places you can go, never numbers that greet you.

**"Set goals" is the casualty.** A goal is a number you can fail, and a goal
that is visible is a number that greets you. It goes.

Everything else survives, and one of them — "congratulatory things" — turns out
to be *older* than the axiom that appears to threaten it. `plan.md`, 15/10/2025,
before any of this was designed:

> 3. Finish book
>    1. congratulations page
>    2. review larger (and smaller) thoughts
>    3. write final thoughts

The instinct was there from the first week, and it was already the right shape:
the congratulation is not the end of the flow, it is the doorway into the
reflection. That is the difference between a celebration and a trophy.

## The rule that reconciles them

One sentence, and every feature in this document can be held against it:

> **The app tells you what you did. It never tells you what you have left.**

That is the whole reconciliation. Tracking can be total — every page, every
highlight, every note, every day — because tracking is not the thing the axiom
forbids. What it forbids is *arithmetic performed on your behalf against a
target you did not reach*. "You have read 43 books" is a fact. "43 of 50" is an
accusation with a fact attached.

Consequences, and they bind:

- No number appears on the home surface. Ever. The home surface is the shelf,
  and a shelf's only number is how much of the wall it covers.
- Counts exist and are queryable, and they live on a page you deliberately
  visit. A place you can go.
- Nothing is ever framed as remaining, pending, unwritten, incomplete or due.
  There is no orphan queue, no "3 books unrated", no highlight inbox.
- Streaks may be *shown in the past tense* — "you read on eleven days in March"
  is a fact about March. A live counter of consecutive days is a target with a
  countdown, and it is out.
- Abandoning a book is not a failure state and is never rendered as one.

## Rewards: three registers, no targets

The brief wants rewards, and note-taking and device reading should both earn
them. They are not the same kind of act, so they should not be flattened into
one score. Three registers, each recognising a genuinely different thing:

**Finishing.** A reading closes. This is the loudest register and the rarest —
a handful of times a year for most readers, which is exactly why it can afford
ceremony.

**Thinking.** A note written, a highlight annotated, a reflection opened, two
books joined by a link. This is the register the positioning already privileges
— *"the unit of value is the connection between highlights, not the highlight"*
— and it is the one derivable from data readingbuddy fully owns. A reader whose
entire library came from a Goodreads CSV can still earn every reward in this
register, because they are earned here, at the desk.

**Returning.** Days on which anything happened, from any source. The weakest
signal and the most universal. Retrospective only, per the rule above.

Nothing in any register has a threshold announced in advance. You find out what
you earned by having earned it. That is the difference between a reward and a
quota, and it is the entire reason this can coexist with the axiom.

## The chain: a moment, a card, a shelf

The three celebration forms in the brief are not three features. They are one
chain, and each is the next one's cause:

```
a reading closes  →  the moment  →  the card is minted  →  the card takes
                     (transient)     (permanent)           its place on the shelf
```

### The moment

Transient by design. The book turns, closes, and slots into the wall. Then —
and this is the load-bearing part — **the reflection opens.** The ceremony's
payload is an invitation to think, not a trophy and not a dead end. `plan.md`
had this right in October: congratulations, then *write final thoughts*.

A moment that ended in a dismissable dialog would be a task-completion popup
wearing a costume. A moment that ends with a cursor in an empty reflection is
the app doing what it exists for.

Moments fire once. That requires recording that they fired, which is state —
but it is state the app keeps *about itself*, not a number it shows you.

### The card

**A card per reading, not per book.** This falls straight out of the schema and
is better than the thing it falls out of: `readings` makes rereads first-class,
so reading *Piranesi* twice mints two cards, and the two sit side by side
showing what changed. What you rated it at 22 and at 31. Which passages you
marked both times. That comparison is not available anywhere else in the world,
because nowhere else kept both readings.

A card carries: the cover, the dates, the rating if a review was written, one
passage pulled from the highlights, and what you left behind — notes, links,
the reflection. It is exportable as an image, because the one social gesture
this app should support is showing someone a book you loved.

Worth noting: the repository was called **`reading_card`** before it was called
readingbuddy. The name was pointing at something.

### The shelf

The permanent home, and the answer to "a collection space where readers can
display the books they've read."

`docs/ux-positioning.md:113` already observed that this is nearly free: the 3D
book's spine thickness comes from `page_count` and its front aspect from the
real cover, so *a row of books is the same model repeated along an axis*, and
the thickness of the wall is honest. A hundred-page novella is thin. *Infinite
Jest* is not. Nobody has to be told this and no number has to say it.

What the shelf gives, in ascending order of how much it justifies the whole
project:

- The library, spine-out, at true proportion.
- Currently-reading pulled proud of the row — the one piece of present tense.
- A finished wall that grows, and a year filtered out of it.
- Selection: the book slides out and turns cover-forward, which is the existing
  single-book scene, so the transition is one camera path rather than a new
  screen.

This is the part no competitor copies quickly, and it is the part that makes
this a place rather than a tool. It is also, not incidentally, a display of
achievement that contains no digits.

**This reverses a settled decision, and the reversal has to be explicit.**
`docs/decisions.md:230` lists, under *Out of scope for now*: "Orphan queue.
Graph view. Author/corpus view. **Shelf view.**" Making the shelf the home
surface is a direct overturn of that line, and two of its neighbours move with
it — an author sort needs some of what "author view" meant, and the orphan queue
is re-examined below rather than merely inherited. Graph view stays out.

The reason the original ruling should not survive: it was made when the shelf
was a *list of books grouped by collection*, and collections were deferred
because three systems minting them is a merge problem with no good default. That
reasoning is still correct and this is not that feature. This shelf groups by
nothing. It is the library at true proportion with no taxonomy imposed on it,
which is precisely why it does not wait on collections being solved. Whoever
edits `decisions.md` should say so on the line, rather than quietly deleting
four words.

**One open question, deliberately left open:** does an abandoned reading appear
on the wall? Hiding it implies abandonment is failure, which the axiom rejects.
Showing it identically to a finished book is a lie. The likely answer is that it
appears, unremarked, and the book knows what it is when you pick it up — but
this is a decision to make against a real shelf with real books on it, not in
advance.

## Notes that talk to each other

The brief asks for "plentiful notes which talk to each other." This is the one
area where the engine is not merely adequate but genuinely strong, and the GUI's
job is mostly to stop hiding it.

Built, working, and currently under-surfaced:

- Markdown in a vault, one file per note, Obsidian-compatible as a courtesy.
- `[[wikilinks]]` with alias and heading syntax, extracted on save.
- **Dangling links are kept as text and resolve themselves later** — link to a
  note that does not exist yet and the edge completes the day you write it.
- Backlinks, indexed in both directions.
- FTS5 full-text search over every note body, with snippets.
- Four kinds — note, session, reflection, review — where the **reflection is the
  graph hub**: private, accretive, openable mid-book, and the thing that ties
  books to each other. Book-to-book connection runs reflection-to-reflection.
- Citations linking a note to a specific highlight.

What the TUI never built and the GUI should: **note search has no interface at
all** outside the CLI, and neither does citation. A full-text index over
everything you have ever thought, with no search box, is the clearest example
of the engine being ahead of its frontends.

What nothing has yet: **note tags** (book tags exist; note tags do not), and a
way to see the graph as a shape. A graph view is listed out of scope in
`decisions.md` and should stay there for now — but the backlinks pane earns its
place, and a reflection that shows every book it has reached is most of what a
graph view is actually wanted for.

One real bug the GUI will expose: `refresh_note_from_disk` is fully built,
exposed on the API and tested — and **nothing calls it automatically.** Edit a
note in Obsidian and `notes_fts` silently goes stale until someone issues the
call by hand, which no frontend ever does. `watch.rs` is wired, but only for
mounts; the vault has no watcher. The GUI is the frontend that makes this
visible, and it should be the one that fixes it.

## The fourth source: reading here

The brief adds something new, and it is a larger change than it looks:

> a surface to attach PDF books as well and have the user be able to submit
> their progress and notes for them — the reading would happen on the same
> device as the app.

Three sources feed the library today — KOReader, calibre, Goodreads — and
`decisions.md` is careful about what readingbuddy is the origin of:

> readingbuddy keeps a durable, queryable local copy of everything but does not
> claim to be the origin of what it copies.

with conflicts resolving toward the origin **for origin-owned fields**. The
ownership table there already has four rows, and the fourth is readingbuddy
itself: vault, links, reflections, reviews, cross-book structure, flashcards,
annotations. So this is not the first data the app originates — it originates
plenty.

What is new is narrower and more interesting: it is the first **reading state**
readingbuddy originates. Where you are in the book, when you started, when you
finished — that row belongs to KOReader for every source that exists today. A
PDF read on this machine has no origin but this machine, so for it, the app is
the device. That is not a violation of the axiom; the rule was never "we never
own anything", it was "we never claim to own what we copied."

What this needs, concretely, is smaller than it sounds, because most of it is
built:

- Attaching the file: `book_files` already stores content-addressed bytes keyed
  by sha256 with the format as a lowercased extension, so `pdf` is already a
  legal value and `import_file` already copies it in.
- Progress: `update_progress` already exists and already writes to the active
  reading. What is missing is a reading with `source = 'local'` and a surface
  to type a page number into.
- Notes: `notes.page` already exists and was written with this in mind — its doc
  comment reads "Device/pdf page the note anchors to". (`notes.location` is a
  free-form sibling and does not mention PDFs; it is useful here anyway.)

What is genuinely missing: **page count and title from a PDF.** `epub.rs`
extracts metadata; there is no PDF equivalent, so an attached PDF has no
denominator and every progress display degrades. That is the one real piece of
new engine work this source requires.

**Explicitly not in scope: an embedded PDF reader.** The brief says the reading
happens on the same device, not inside this app — and readingbuddy's entire
positioning is *the desk you sit at after you put the book down*. It is not a
reader and never competes with one. Attaching a PDF, recording where you got to,
and writing against it is the desk doing its job. Rendering the pages is a
different product, and if it is ever wanted it should be argued for on its own
merits rather than smuggled in as an implementation detail of progress tracking.

A note on what this source can and cannot supply: it has no highlights, because
nothing captured them. That is fine and should be stated rather than papered
over — the same discipline that makes `goodreads.rs` refuse to invent a start
date. A locally-read PDF earns rewards in the *finishing* and *thinking*
registers, honestly, and contributes nothing to a highlight count that would
have been fabricated.

Worth knowing before it surprises someone: KOReader probably cannot supply PDF
highlights either. `entry_to_highlight` requires a string `pos0`, and on PDF
KOReader stores a table there — so the entry would be skipped in silence. This
is reasoned from the source, not observed: `docs/koreader-format.md:482` files
PDF annotations under *unobserved*, because both PDF sidecars in the corpus have
an empty `annotations` block. Either way, PDFs are the format where highlights
do not arrive, and the local source does not change that.

## "Source of truth", honestly

The brief's strongest claim is the last one, and it needs a sharper version to
be true.

readingbuddy is not the source of truth in the sense of being where the facts
originate — KOReader knows where you are in the book, calibre owns the file,
providers own the ISBN. `decisions.md` is right about this and should not be
softened.

It is the source of truth in a different and better sense: **it is the only
place that has all of it, and the only place that outlives any single source.**

That is not a slogan. It is a description of things that have already happened:
Goodreads shut down its API in December 2020 and the CSV export is the only way
data leaves. KOReader sidecars live on a device that gets wiped, upgraded, lost
or replaced. Calibre libraries live on a drive. Every one of these is a place
your reading life is held hostage, and readingbuddy is the copy that survives
all of them failing.

So the honest claim, and a stronger one than the brief made:

> Every source is a tenancy. This is the freehold.

The consequences are practical and they are mostly gaps. If this is the copy
that survives, then **export must be as good as import**, and today it is not:
there is a Goodreads CSV out (which drops rereads by format necessity) and a
flashcard TSV, and nothing else. Highlights, notes, per-reading history and
device state have no way out. A vault of markdown files is a real hedge and
should be said out loud as one — but the database around it is not exportable,
and a source of truth you cannot get your data out of is just another tenancy
with better manners.

## What the GUI is not

Stated so it does not have to be re-argued per feature:

- **Not a reader.** No epub rendering, no PDF viewer, no reading position
  tracking by observation. The desk, with the shelf behind it.
- **Not a social network.** The card exports as an image. That is the whole of
  the social surface. Reviews are written for an audience and go out as text;
  publishing them from here is out of scope and stays out.
- **Not a task manager.** No due dates, no reading queue with a shape that
  implies obligation, no "to-read" as an inbox. `to-read` arrives from Goodreads
  as zero readings, which is the correct representation: a book you have not
  started is a book with no history, not a task with no completion.
- **Not a replacement for the TUI.** The two are peers and neither gates the
  other. Work on one must never require work on the other, which is a
  constraint on the *engine*, not on either frontend: anything both need lives
  below both.

---

# Settled — for `docs/decisions.md`

## The GUI

- **Tauri + Svelte**, per the existing decision, and the GUI links
  **`readingbuddy-api` in-process** behind a swappable client trait. Not the
  daemon. The daemon does not solve the two-writer problem while the TUI keeps
  its direct engine link, buys no remoteness while covers and paths cross as
  filesystem strings, and has no push channel to announce background work with.
  It arrives with item 15's plugin listener, which is where it was justified
  from, and drops in without touching GUI code.
- **Every call goes through the API vocabulary even in-process.** A gap in the
  API surface must be a compile error, not a temptation to reach past it.
- **TUI and GUI are peers and are developed independently.** Shared logic moves
  into the engine; neither frontend depends on the other's schedule. The TUI
  keeps its current implementations until it chooses to migrate.

## Rewards and celebration

- **Tracking is total; presentation is retrospective.** The rule: *the app tells
  you what you did, it never tells you what you have left.*
- **No goals.** No targets, no live streak counters, no pending/remaining/due
  framing anywhere. Past-tense statements of fact are permitted.
- **No number on the home surface.** Counts live on a page you visit.
- **Three reward registers**: finishing, thinking, returning. Note-taking and
  device reading both earn; neither is a proxy for the other.
- **The chain is moment → card → shelf.** The moment is transient and ends by
  opening the reflection. The card is permanent and is **per reading, not per
  book**. The shelf is the home surface.
- **Abandoning is not failure** and is never rendered as one.

## Reversal of a settled line

- **"Shelf view" leaves *Out of scope for now*** (`decisions.md:230`). The
  ruling was made against a shelf that grouped by collection, and collections
  are still deferred. This shelf groups by nothing, so it does not wait on
  them. **"Author/corpus view" partially moves with it** (an author sort is in
  scope; a corpus view is not). **"Graph view" and "Orphan queue" stay out** —
  the latter now for the additional reason that the axiom forbids it outright.

## The local-reading source

- **readingbuddy originates reading *state* for books read on this machine** —
  the row KOReader owns for every other source. It already originates the vault;
  this is new only for position, start and finish. Conflicts have no origin to
  resolve toward, because this is the origin.
- A locally-read book gets a reading with `source = 'local'`, manual progress,
  and notes. **It has no highlights and none are invented.**
- **PDF metadata extraction** (page count, title) is required engine work.
- **No embedded reader.** Not an epub renderer, not a PDF viewer. If one is ever
  wanted it is argued separately.

## Source of truth

- The claim is **"the only place that has all of it, and the only place that
  outlives any single source"** — not origination.
- Therefore **export is a first-class obligation, not a convenience**, and it is
  currently the weakest part of the system.

## Deferred, with reasons

- **KOReader `statistics.sqlite3`** — per-session time and pages/day. Lands with
  the KOReader plugin work (item 15), not with the GUI. Until then it is one
  absent filler of an event log that already works without it.
- **Goals of any kind.** Not "later" — decided against.
- **Graph view.** Stays out of scope; the backlinks pane and the reflection's
  reach cover the real want.
- **Note tags.** Wanted, not this phase.
- **Whether abandoned readings appear on the shelf.** Decided against a real
  shelf, not in advance.
