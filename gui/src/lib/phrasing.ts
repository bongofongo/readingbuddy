/**
 * Words for values the engine already decided.
 *
 * The line `gui/CLAUDE.md` draws: the engine does no *phrasing*, so wording,
 * pluralisation and layout are the frontend's. The **values** being phrased are
 * not. So a `reading_state` of `abandoned` is the engine's answer, and calling
 * it *Put down* is this file's.
 *
 * What must never appear here is a function that computes a state from other
 * columns. `!finished && current_page === null` is row-state **derivation** and
 * belongs in the engine — every time that temptation came up while building the
 * item 25 slice it became a line in `docs/prompts/17-derived-facts.md` instead
 * of a line here, and **item 17 has now landed all of them**:
 *
 * - `seriesLabel` is gone. `BookDto.series_label` is the engine's, so the two
 *   frontends cannot spell `#2.5` two ways.
 * - `authorsLabel` no longer touches `authors`. It joins `authors_display`,
 *   which the engine has already read the comma in — the *join* is still the
 *   phrasing this file is for.
 * - `readingStateLabel` switches on a typed union rather than three magic
 *   strings.
 * - `progressLabel` is new, and words `ProgressDto` — it does no arithmetic.
 *
 * The two that stayed are `titleLabel` and the absence handling in
 * `authorsLabel`, and that was decided rather than defaulted: **the engine
 * states the absence, the frontend words it**. `title` is `null` and `authors`
 * is `[]` on the wire; *Untitled* is a word, and a word is ours.
 */
import type {
  NoteDto,
  ProgressDto,
  RatingDto,
  ReadingDto,
  ReadingSortDto,
  ReadingStateDto,
} from '$lib/api/bindings';

/** The current reading's state, phrased. */
export function readingStateLabel(state: ReadingStateDto | null): string | null {
  if (state === null) {
    // No reading at all — the commonest state in a real library. It gets no
    // label rather than "Unread", which frames the shelf as a list of things
    // not done. Note the engine deliberately has no variant for this: it is
    // absence, so that nothing can filter or count on it.
    return null;
  }
  switch (state.state) {
    case 'reading':
      return 'Reading';
    case 'finished':
      return 'Read';
    case 'abandoned':
      // Not "Did not finish", not "Abandoned" with a warning colour. The axiom:
      // abandoning a book is not failure and is never styled as one. This is a
      // book you might pick up, which is also why `abandon_reading` leaves the
      // reading open.
      return 'Put down';
    case 'other':
      // A status this build does not know. An importer can write one, which is
      // why the engine keeps the raw word — showing it verbatim beats inventing
      // one for it.
      //
      // Item 26's screenshot review reported the lowercase `paused` sitting in
      // a column of `Read` and `Put down` as a defect — an enum value leaking
      // to the user. Considered and **declined**. `raw` is another application's
      // word, not ours to restyle, and the case is part of what it said; the
      // rule that the frontend words a value covers *our* vocabulary, and this
      // is the one label that is deliberately not in it. Title-casing also only
      // flatters the single-word case — the fixture's real shape is
      // `paused-by-some-other-app`, which capitalises into something worse.
      return state.raw;
    default:
      // ts-rs drops `#[serde(other)]`, so this union is exhaustive over *today's*
      // variants while the wire is not. tsc believes this arm is unreachable and
      // it is what makes a newer build's state degrade instead of crashing.
      return null;
  }
}

/**
 * A book's title, for display.
 *
 * `title` is genuinely nullable: a sidecar-seeded book whose `doc_props` had no
 * title has none, and the dev library contains one on purpose. **The engine
 * states the absence; this words it.**
 */
export function titleLabel(title: string | null): string {
  return title && title.trim() !== '' ? title : 'Untitled';
}

/**
 * Authors, joined.
 *
 * Takes `BookDto.authors_display` — the names already read the right way round
 * — and **never `BookDto.authors`**, which is the origin's own spelling and is
 * the record rather than the rendering. Whether `Borges, Jorge Luis` displays as
 * `Jorge Luis Borges` is author-name parsing and is `readingbuddy::names`'
 * (item 17); the join between names is wording and is this.
 */
export function authorsLabel(authorsDisplay: string[]): string | null {
  return authorsDisplay.length === 0 ? null : authorsDisplay.join(', ');
}

/**
 * How far in, in words.
 *
 * Every number here was computed by the engine. This function chooses **which
 * of them to say**, which is the frontend's whole half of item 17b — the TUI
 * words the same value four different ways on four different screens and is
 * right to.
 *
 * `null` for `Untouched`: a book with nothing recorded gets no tag rather than
 * "Not started", because a shelf full of *Not started* is a list of things you
 * have not done.
 */
export function progressLabel(p: ProgressDto): string | null {
  switch (p.progress) {
    case 'finished':
      return null;
    case 'untouched':
      return null;
    case 'started':
      if (p.percent !== null) return `${p.percent}%`;
      // A page with no honest denominator. Not `p.page / 0`, and not a
      // percentage invented from a length nobody recorded.
      if (p.page !== null) return `p. ${p.page}`;
      return null;
    default:
      return null;
  }
}

/**
 * The same value, at length — for a screen that has room for the page a reader
 * actually recognises: `p. 500 of 1408 · 35%`.
 *
 * Two phrasings of one value, and that is the frontend's half of item 17b
 * working as intended rather than a duplication: the TUI words the same
 * `Progress` four ways across four screens. Nothing here decides *which case*
 * the book is in — that is the tag it switches on — and nothing here divides.
 */
export function progressDetail(p: ProgressDto): string | null {
  if (p.progress !== 'started') return progressLabel(p);
  const pct = p.percent === null ? null : `${p.percent}%`;
  if (p.page === null) return pct;
  // `of` is absent for a book with no length **and** for one whose `page_count`
  // is zero; the engine already collapsed those, so there is no `of 0` to guard.
  const where = p.of === null ? `p. ${p.page}` : `p. ${p.page} of ${p.of}`;
  return pct === null ? where : `${where} · ${pct}`;
}

// ---------------------------------------------------------------------------
// Item 27's words. Everything below phrases a value some other layer decided.
// ---------------------------------------------------------------------------

/**
 * A day, in UTC, as `2025-01-14`.
 *
 * Item 17 decided dates stay in the frontend, and stated the limit of that
 * ruling: *relative* time needs an answer to "what is today", the engine's day
 * convention is UTC, and inventing a local-time answer is what item 31 refused
 * for reading minutes. So this does the half that is safe — an absolute day,
 * on the engine's own convention — and there is deliberately no "3 days ago"
 * anywhere in this app.
 *
 * Formatted rather than handed to `Intl`, because a locale-dependent rendering
 * makes the committed screenshots depend on the machine that took them.
 */
export function dayLabel(unixSeconds: number | null): string | null {
  if (unixSeconds === null) return null;
  const d = new Date(unixSeconds * 1000);
  return Number.isNaN(d.getTime()) ? null : d.toISOString().slice(0, 10);
}

/**
 * When one reading ran.
 *
 * `finished_at` being null means **open**, not unknown — at most one reading
 * per book may be — so an open read says when it started and stops there rather
 * than drawing a dash toward a date it is waiting for. Nothing here counts days
 * or measures a run against anything.
 */
export function readingSpan(r: ReadingDto): string | null {
  const from = dayLabel(r.started_at);
  const to = dayLabel(r.finished_at);
  if (from && to) return `${from} – ${to}`;
  if (from) return `from ${from}`;
  if (to) return `to ${to}`;
  return null;
}

// ---------------------------------------------------------------------------
// Item 47's words. The read ordinal and the wall's three orders — both of them
// phrasings of values items 41 and 43 put on the wire.
// ---------------------------------------------------------------------------

const ORDINALS = [
  'first',
  'second',
  'third',
  'fourth',
  'fifth',
  'sixth',
  'seventh',
  'eighth',
  'ninth',
  'tenth',
];

/**
 * Which read this is — *your second read* — or `null` for a book read once.
 *
 * **The number is the engine's** (item 41). `read_number` and `of_reads` come
 * off `ReadingRowDto`, counted over every reading of the book by two correlated
 * subqueries so that a wall filtered to one year still calls a reread's second
 * read the second. `readings.indexOf(id) + 1` is what this replaces, and it was
 * wrong twice over: it re-implemented a domain rule *and* silently re-acquired a
 * dependency on `list_readings`' oldest-first ordering, which is stated nowhere
 * on the wire — with nothing on the screen looking wrong.
 *
 * `of_reads > 1` is the whole of the frontend's half, and it is the **same test
 * the TUI's gutter makes**: `ReadCount::ordinal` is "a lone read has no number",
 * so a book read once gets no ordinal here rather than a lonely *your first
 * read*. Neither field is an `Option`, so there is no third case to word.
 *
 * Past tense, about one book, on a page you chose to open — which is the three
 * things that make a number allowed at all.
 */
export function readOrdinalLabel(readNumber: number, ofReads: number): string | null {
  if (ofReads <= 1) return null;
  const name = ORDINALS[readNumber - 1];
  return `Your ${name ?? `${readNumber}${ordinalSuffix(readNumber)}`} read`;
}

/** `1st`, `2nd`, `3rd`, `4th` — and `11th`/`12th`/`13th`, which the teens break. */
function ordinalSuffix(n: number): string {
  const tens = n % 100;
  if (tens >= 11 && tens <= 13) return 'th';
  switch (n % 10) {
    case 1:
      return 'st';
    case 2:
      return 'nd';
    case 3:
      return 'rd';
    default:
      return 'th';
  }
}

/**
 * What one of the wall's three orders is called.
 *
 * All three are descending — most recent first — so the labels name the *event*
 * rather than the direction: the engine has no ascending arm and a label saying
 * "newest" would be describing a choice nobody made. There are three and there
 * is deliberately no fourth: a title sort would order the list by a `books`
 * column no index on `readings` can serve, and sorting is the engine's anyway.
 */
export function readingSortLabel(sort: ReadingSortDto): string {
  switch (sort) {
    case 'finished':
      return 'Finished';
    case 'started':
      return 'Started';
    case 'last_modified':
      return 'Touched';
    default:
      // Exhaustive in TypeScript, open on the wire — see `readingStateLabel`.
      return sort;
  }
}

/**
 * Which kind of note a row is — and `null` for an ordinary one.
 *
 * The four kinds live in one list rather than in four tabs (the TUI's ruling: a
 * section is a *collection*, and there is exactly one reflection), so a row has
 * to say which it is. A plain note gets no label because the whole list is
 * notes; labelling every row would be a column of the same word.
 *
 * `kind` is a `String` on the wire and stays one, so an unknown value shows
 * verbatim — the same call `readingStateLabel` makes about another app's word.
 */
export function noteKindLabel(kind: string): string | null {
  switch (kind) {
    case 'note':
      return null;
    case 'session':
      return 'Session';
    case 'reflection':
      return 'Reflection';
    case 'review':
      return 'Review';
    default:
      return kind;
  }
}

/**
 * What a note is hung off: a page, a location, or a passage.
 *
 * The TUI's `anchor_tag`, ported — including its ordering, and its rule that a
 * note anchored to a highlight with no page says so with an arrow rather than
 * with nothing. Choosing which of three absences to word is phrasing; the
 * absences themselves are the engine's columns.
 */
export function noteAnchorLabel(n: NoteDto): string | null {
  const parts: string[] = [];
  if (n.page !== null) parts.push(`p. ${n.page}`);
  if (n.location !== null) parts.push(n.location);
  if (parts.length === 0 && n.highlight_id !== null) parts.push('↳ passage');
  return parts.length === 0 ? null : parts.join(' · ');
}

/**
 * That some note quotes this passage (item 48).
 *
 * **No number, at any count.** The fact a reader wants from a band of passages
 * is *this one is in my notes*, and a `3` beside it would be the library
 * counting itself on a screen that exists to show what was written. The mark is
 * drawn from `CitationsForNotes`, which answers with ids, so a count is
 * available and deliberately not spoken.
 *
 * Singular by design and true at every count: a passage quoted in three notes is
 * still quoted in a note. It claims only what was asked about — the batch is
 * scoped to the note ids the screen holds, which is this book's notes, and a
 * note filed under no book could quote a passage with nothing here to say so.
 */
export const QUOTED = 'Quoted in a note';

/**
 * The words already taken off a passage as cards (item 49).
 *
 * **A sentence in the past tense, and deliberately not `Cards: a, b`.** The
 * `label: value` shape is what the About rail uses (`Publisher`,
 * `Language`), and set in the same dim ink two lines under `Chapter 9 · p. 640`
 * it scanned as a third metadata field rather than as a record of something the
 * reader did — measured, in the item 49 screenshot review, as byte-identical
 * colour at the same size. *You kept* is the same voice as the `YOU` chip
 * above it and cannot be read as a property of the passage.
 *
 * The pluralisation and the joining are the frontend's half; the list is the
 * engine's — these are `FlashcardDto.word`s in the order
 * `list_flashcards_for_book` gave them (`created_at ASC`), not sorted here.
 * `null` for none, so a passage nobody has captured from gets **no element**
 * rather than an empty label: absence is not a zero, and *no cards yet* would be
 * the completion framing the axiom forbids.
 */
export function cardWordsLabel(words: string[]): string | null {
  const marked = words.map((w) => `“${w}”`);
  const last = marked.pop();
  if (last === undefined) return null;
  return `You kept ${marked.length === 0 ? last : `${marked.join(', ')} and ${last}`}`;
}

/**
 * A rating, against the scale it was recorded on.
 *
 * The scale travels **with** the value on the wire and this is why: the
 * Goodreads map is user-editable, so `4.5` alone is not re-derivable into
 * anything. `4.5 / 5`, never `4.50`.
 */
export function ratingLabel(r: RatingDto): string {
  return `${trimNumber(r.value)} / ${trimNumber(r.scale.max)}`;
}

/** `4.5` and `4`, never `4.50` or `4.0`. A `step` of 0.5 makes both reachable. */
export function trimNumber(v: number): string {
  return String(Math.round(v * 100) / 100);
}

/**
 * A file's size, roughly. Binary units, because that is what a file browser on
 * either platform will say about the same file.
 */
export function fileSizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/**
 * Who supplied a field.
 *
 * `FieldSourceDto.source` is read straight off the column, whose vocabulary
 * lives in a comment rather than a `CHECK` — so an unrecognised token is a row
 * a newer engine wrote and is shown as it was stored. `user` is the rank that
 * outranks every provider, and *You* is what that reads as on a screen you own.
 */
export function sourceLabel(source: string): string {
  switch (source) {
    case 'open_library':
      return 'Open Library';
    case 'google_books':
      return 'Google Books';
    case 'koreader':
      return 'KOReader';
    case 'goodreads':
      return 'Goodreads';
    case 'calibre':
      return 'calibre';
    case 'epub':
      return 'the EPUB';
    case 'pdf':
      return 'the PDF';
    case 'user':
      return 'You';
    default:
      return source;
  }
}

/** A column name, said out loud. `publish_year` is a schema word, not a English one. */
export function fieldLabel(field: string): string {
  const said = field.replace(/_/g, ' ');
  return said.charAt(0).toUpperCase() + said.slice(1);
}

// ---------------------------------------------------------------------------
// Item 28's words. The reading-life page is the one surface counts appear on,
// so this is where the pluralisation lives — and where the difference between
// *nothing measured* and *zero* becomes a sentence.
// ---------------------------------------------------------------------------

/**
 * What a `null` measurement says.
 *
 * The single most important string on the reading-life page. `minutes` and
 * `pages` are `Option` at every level of item 21's log, and item 42 exists
 * because bucketing days into months in a client **collapses that `null` to
 * `0`** — telling a reader they read for no time at all in a month they simply
 * read off-device.
 *
 * Past tense and no apology: it says what the record holds, and the hint beside
 * it names where minutes come from. It is deliberately not *missing*, *no data
 * yet*, or anything else that frames an absence as an omission somebody owes.
 */
export const NOT_MEASURED = 'not measured';

/**
 * Minutes read, or the absence.
 *
 * **`0` is a measurement and prints as one.** Item 31: *"a measured
 * twenty-second session records `Some(0)`, not `None`"* — the device is saying
 * something, and rendering that as [`NOT_MEASURED`] would throw away the one
 * distinction the whole column is nullable to keep.
 */
export function minutesLabel(minutes: number | null): string {
  if (minutes === null) return NOT_MEASURED;
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** Pages turned, or the absence. Independent of minutes — either may be null alone. */
export function pagesLabel(pages: number | null): string {
  return pages === null ? NOT_MEASURED : countLabel(pages, 'page');
}

/**
 * What a month with nothing from a device says instead of a number.
 *
 * *"A month with no device data **says so**; it does not show a zero."* It is a
 * statement about provenance and not an apology or an omission — minutes and
 * pages come from a reader's own `statistics.sqlite3` (item 31) and a library
 * built from a Goodreads CSV has none, for ever, which is not a gap anybody is
 * going to fill.
 */
export const NO_DEVICE_DATA = 'no device data';

/**
 * The device's two figures for a period, worded — one chip or two.
 *
 * Collapsed to one when **both** are absent, because two chips reading *not
 * measured* side by side say the same thing twice and neither says which is
 * which. Named when only one is, because `minutes` and `pages` are independent
 * `Option`s — the fixture has a month with pages and no minutes, and a page
 * treating them as a pair would be wrong about it.
 *
 * Never empty: a period that measured nothing still gets a sentence, which is
 * the difference between rendering an absence and rendering nothing.
 */
export function deviceFigures(minutes: number | null, pages: number | null): string[] {
  if (minutes === null && pages === null) return [NO_DEVICE_DATA];
  return [
    minutes === null ? `minutes ${NOT_MEASURED}` : minutesLabel(minutes),
    pages === null ? `pages ${NOT_MEASURED}` : pagesLabel(pages),
  ];
}

/**
 * A count and its noun, agreed.
 *
 * Pluralisation is the frontend's half of item 17 and this is the whole of it:
 * nothing here computes the number, and every caller hands one the engine
 * counted. `0` is a legitimate answer for the fields the engine can count
 * honestly — `books_finished` is ours, so a zero there is *knowable* — which is
 * exactly what separates it from the nullable ones above.
 */
export function countLabel(n: number, singular: string, plural = `${singular}s`): string {
  return `${n} ${n === 1 ? singular : plural}`;
}

/**
 * A list of things, joined the way a sentence joins them: *A*, *A and B*,
 * *A, B and C*.
 *
 * The reading-life page writes its months as sentences rather than as a stat
 * block, and this is the join those sentences need. No Oxford comma, and no
 * `Intl.ListFormat` — for `monthLabel`'s reason: a locale-dependent rendering
 * would make the committed screenshots depend on the machine that took them.
 *
 * **Never truncates.** A month in which eleven books were finished says eleven
 * titles; "and 3 more" would be a count of the part the page decided not to
 * show, which is the one kind of number this app does not write.
 */
export function joinList(items: string[]): string {
  if (items.length === 0) return '';
  if (items.length === 1) return items[0]!;
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]!}`;
}

/**
 * `2025-03` as *March 2025*.
 *
 * Spelled from a fixed table rather than handed to `Intl`, for `dayLabel`'s own
 * reason: a locale-dependent rendering makes the committed screenshots depend on
 * the machine that took them. An unparseable month is shown verbatim — it came
 * off `substr(day, 1, 7)` over a zero-padded ISO date, so it cannot be wrong,
 * and inventing a correction for a case that cannot happen would hide one that
 * could.
 */
const MONTH_NAMES = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

export function monthLabel(month: string): string {
  const m = /^(\d{4})-(\d{2})$/.exec(month);
  if (m === null) return month;
  const name = MONTH_NAMES[Number(m[2]) - 1];
  return name === undefined ? month : `${name} ${m[1]}`;
}
