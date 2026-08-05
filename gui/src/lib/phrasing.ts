/**
 * Words for values the engine already decided.
 *
 * The line `gui/CLAUDE.md` draws: the engine does no *phrasing*, so wording,
 * pluralisation and layout are the frontend's. The **values** being phrased are
 * not. So `reading_status` arriving as `'abandoned'` is the engine's answer, and
 * calling it *Put down* is this file's.
 *
 * What must never appear here is a function that computes a state from other
 * columns. `!finished && current_page === null` is row-state **derivation** and
 * belongs in the engine (spec item 17) — every time that temptation came up while
 * building this slice it became a line in `docs/prompts/17-derived-facts.md`
 * instead of a line here.
 */

/** `readings.status`, phrased. */
export function readingStateLabel(status: string | null): string | null {
  switch (status) {
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
    case null:
      // No reading at all — the commonest state in a real library. It gets no
      // label rather than "Unread", which frames the shelf as a list of things
      // not done.
      return null;
    default:
      // A status this build does not know. The importer can write one, which is
      // why `reading_status` crosses as a string and not an enum — showing it
      // verbatim beats inventing a word for it.
      return status;
  }
}

/**
 * A book's title, for display.
 *
 * `title` is genuinely nullable: a sidecar-seeded book whose `doc_props` had no
 * title has none, and the dev library contains one on purpose.
 */
export function titleLabel(title: string | null): string {
  return title && title.trim() !== '' ? title : 'Untitled';
}

/**
 * Authors, joined.
 *
 * The join is phrasing. **Whether `Borges, Jorge Luis` should be displayed as
 * `Jorge Luis Borges` is not** — that is author-name parsing, which
 * `gui/CLAUDE.md` names as a derived fact, and the TUI needs the same answer. So
 * the stored string is shown as stored, and the reordering is an item 17 line.
 */
export function authorsLabel(authors: string[]): string | null {
  return authors.length === 0 ? null : authors.join(', ');
}

/**
 * The series and its place in it.
 *
 * `series_index` is a REAL and `2.0` must print as `2` — the engine already
 * settled that in `readingbuddy::series_index_text`, and `Book::series_label`
 * exists precisely so two frontends do not disagree. This is the phrasing around
 * a value that has NOT yet been surfaced on `BookDto` as a joined label, so it
 * reconstructs the pair from its two fields and nothing else.
 */
export function seriesLabel(series: string | null, index: number | null): string | null {
  if (!series) return null;
  if (index === null) return series;
  return `${series} #${index}`;
}
