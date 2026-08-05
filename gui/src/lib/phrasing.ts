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
import type { ProgressDto, ReadingStateDto } from '$lib/api/bindings';

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
