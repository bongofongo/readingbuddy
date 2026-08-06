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
