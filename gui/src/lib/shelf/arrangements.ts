/**
 * How the wall is arranged, and where its groups fall.
 *
 * ## What replaced what
 *
 * This module is the old `layouts.ts` seam, turned ninety degrees. That seam
 * offered two *shapes of the page* — a cover grid and a list of rows — and the
 * layout redesign dropped the second on the grounds that a shelf of rows is a
 * list, and the list is what the tile grid already is at a smaller size. What
 * the switch offers now is an **arrangement**: the same wall of jackets, cut
 * into groups at a different place.
 *
 * The seam did not disappear with `Rows`; it narrowed. A layout used to own the
 * whole band, so the deferred spine shelf (item 19, item 26) would have been a
 * third entry here. It is now an alternative renderer of **one group's field** —
 * `Wall.svelte` draws the headings and the runs, and what fills a run is one
 * component. That is a smaller contract than the old one and it is the one the
 * ray tracer actually needs: books in, an event out, never a per-frame boundary.
 *
 * ## Three arrangements, and the one that is missing on purpose
 *
 * `Recent` was in the redesign draft and is **not** here. It is one of two
 * things and both are bad: recency of *finishing* is what `Year` already puts at
 * the top of the wall, and recency of *adding* is the backlog sort by name — the
 * single arrangement most likely to turn a record of reading into a to-do list.
 * There is a third reading, recency of *interaction*, which is genuinely useful
 * and is what the "Reading now" band above is; a second answer to it here would
 * compete with that band on the same page.
 *
 * ## Grouping never reorders
 *
 * **The order is the engine's** (item 17) and this module does not touch it. A
 * group is a *run* in the order the engine already returned: consecutive books
 * whose group key is the same. So `Author` shows whatever `BookSort::Author`
 * decided, with a rule drawn wherever the initial changes — and if the engine
 * changes its mind about how names sort, this file needs no edit and cannot
 * disagree with it.
 *
 * ## And why `Author` and `Title` show fewer books than `Year`
 *
 * This is the finding that is easy to undo by accident, so it is written here
 * rather than in a doc: **the arrangement switch is not axiom-neutral.**
 *
 * Under `Year`, books with no reading are quarantined into one group at the
 * bottom of a long scroll — read and unread are spatially separated, and the
 * wall reads as an accumulating record with an appendix. Under `Author` or
 * `Title` they *interleave*: an imported `to-read` sits between two books you
 * finished, identical in every respect except that it lacks the thing the others
 * have. That is the textbook backlog rendering, and it arrives without a single
 * number or label changing. Adjacency is the mechanism, and styling does not
 * hold the line against it.
 *
 * So `Author` and `Title` are arrangements **of the reading life**: a book with
 * no reading is not hidden, not toggled and not counted — it simply has no
 * answer to contribute to "whose work have I read". It is on the wall under
 * `Year`, in the group that says so. The same discipline as `goodreads.rs`
 * refusing to invent a start date.
 */
import type { BookSortDto } from '$lib/api/bindings';
import type { StoredBook } from '$lib/api/client';

export type ArrangementId = 'year' | 'author' | 'title';

export type Arrangement = {
  id: ArrangementId;
  /** What the switch calls it. A noun, not an instruction. */
  label: string;
  /** What the engine is asked for. The ordering is never computed here. */
  sort: BookSortDto;
};

/** The registry. Order is the order the switch offers them; the first is the default. */
export const ARRANGEMENTS: [Arrangement, ...Arrangement[]] = [
  // `last_modified` and not `year`: `BookSort::Year` is the **publication**
  // year, which is a different fact from the year a reading closed and must not
  // share a name with it. The wall's own order under this arrangement comes
  // from the readings that closed, which is `finishYears`' job below.
  { id: 'year', label: 'Year', sort: 'last_modified' },
  { id: 'author', label: 'Author', sort: 'author' },
  { id: 'title', label: 'Title', sort: 'title' },
];

export const DEFAULT_ARRANGEMENT: ArrangementId = ARRANGEMENTS[0].id;

/** The arrangement with this id, or the default. Never throws: see [`recallArrangement`]. */
export function arrangementById(id: string | null): Arrangement {
  return ARRANGEMENTS.find((a) => a.id === id) ?? ARRANGEMENTS[0];
}

/**
 * One run of the wall: a heading, and the books under it.
 *
 * `captions` travels with the group rather than with the wall, and that is the
 * whole of the captions decision. A caption earns its space when the image alone
 * cannot identify the item — and at ~91px a printed title renders around 8–14px,
 * often reversed, often in a display face, so these tiles are **not**
 * self-captioning. But identification is only the task for books you do not
 * know, and those are all in one group: the year groups are recognition
 * surfaces, where you already hold a template of the cover and a caption buys a
 * text row per tile for nothing.
 *
 * So captions are off, except in "No reading recorded", where they are the only
 * thing making the group usable — and where the covers are likeliest to be
 * missing or wrong, since nothing in the import pipeline guarantees art for a
 * book nobody has opened. **Not a setting.** A toggle would be a way to avoid
 * choosing, would double the layout states to test, and interacts badly with
 * tile size: a caption that fits at 91px truncates at 86px.
 */
export type ShelfGroup = {
  /** Stable across renders, so `{#each}` keys on it rather than on an index. */
  key: string;
  heading: string;
  books: StoredBook[];
  captions: boolean;
};

/**
 * The year each book's most recent reading **closed** in, from a list of closed
 * readings.
 *
 * The rows arrive newest-finish-first (`ReadingSort::Finished`), so the first
 * row naming a book is that book's most recent close and every later one is an
 * earlier read of the same book. A reread therefore lands on the wall once, in
 * the year you last finished it, which is the year a shelf would put it in.
 *
 * A row with no `finished_at` cannot be here — the caller asks for `open: false`
 * — but the guard is kept rather than asserted, because the filter is the
 * caller's to pass and a wall silently grouping open readings under `1970` is
 * the kind of thing nobody notices for a wave.
 */
export function finishYears(
  rows: { book: { id: number | null }; reading: { finished_at: number | null } }[],
): Map<number, number> {
  const years = new Map<number, number>();
  for (const row of rows) {
    const id = row.book.id;
    const at = row.reading.finished_at;
    if (id === null || at === null) continue;
    if (years.has(id)) continue;
    years.set(id, new Date(at * 1000).getUTCFullYear());
  }
  return years;
}

/**
 * Where a book with no closed reading goes, and what that group is called.
 *
 * Three terminal groups rather than the one the redesign drafted, and the
 * difference is an **engine fact rather than a design change**: the draft says a
 * put-down reading "appears in the year it was put down", and there is no such
 * year on the wire. `Storage::abandon_reading` sets the status and deliberately
 * leaves `finished_at` NULL — a put-down reading stays *open*, which is also why
 * `ReadingFilter::open` is not redundant with `status`. So the shelf can say
 * *this reading has not ended* or it can invent a date, and inventing one from
 * `last_modified` would put a book in the year you last edited a note about it.
 *
 * **A date the engine does not record is an engine item, not a frontend
 * workaround.** Until there is one, these are three true statements:
 *
 * - *Still reading* — the word `/cards`' own picker already uses for an open
 *   reading, chosen there for this reason: the state you are in, never
 *   *unfinished*, which is the same fact framed as something owed.
 * - *Put down* — the phrase this codebase pins as the one that is not
 *   failure. Same tiles, same size, same treatment as a finished book; the
 *   distinction it cannot carry at 91px lives on the card, one click away.
 * - *No reading recorded* — a statement of fact, not a queue. Goodreads
 *   `to-read` arrives as zero readings and this is what zero readings looks
 *   like.
 */
function terminalGroup(book: StoredBook): { key: string; heading: string; captions: boolean } {
  const state = book.reading_state;
  if (state === null) {
    return { key: 'none', heading: 'No reading recorded', captions: true };
  }
  if (state.state === 'abandoned') return { key: 'put-down', heading: 'Put down', captions: false };
  if (state.state === 'finished') {
    // Finished, and yet in no year: the reading's `finished_at` is absent, which
    // a Goodreads import can produce. It is a read that happened on a day nobody
    // recorded, so it is not a year and it is not "no reading" either.
    return { key: 'undated', heading: 'Read, undated', captions: false };
  }
  return { key: 'still-reading', heading: 'Still reading', captions: false };
}

/** The first letter a run is grouped under, or `#` for anything that is not one. */
function initial(text: string | null): string {
  const first = (text ?? '').trim().charAt(0);
  if (first === '') return '#';
  const upper = first.toLocaleUpperCase();
  // `\p{L}` rather than A–Z: the shelf holds CJK and Arabic titles on purpose,
  // and bucketing every one of them under `#` would be this file deciding that
  // a library is Latin.
  return /\p{L}/u.test(upper) ? upper : '#';
}

/**
 * The wall, cut into groups.
 *
 * `years` is only read under `year`; the caller fetches it once and keeps it,
 * because switching arrangement must not re-ask the library what it finished.
 */
export function shelfGroups(
  books: StoredBook[],
  years: Map<number, number>,
  arrangement: ArrangementId,
): ShelfGroup[] {
  if (arrangement === 'year') return byYear(books, years);
  const key = (b: StoredBook) =>
    arrangement === 'author'
      ? initial(b.authors_display[0] ?? null)
      : initial(b.sort_title ?? b.title);
  // The reading life only — see the header. `reading_state` is `null` for a book
  // with no reading at all, which is exactly the set that has no answer here.
  return runs(
    books.filter((b) => b.reading_state !== null),
    key,
    (k) => k,
  );
}

/**
 * Years descending, then the books no year holds.
 *
 * The years are not sorted here either: `finishYears` was built from rows the
 * engine returned newest-first, so walking the books in *their* order and
 * bucketing by year would scatter a year across the wall. This walks the years
 * in the order the readings gave them and takes each year's books in the order
 * `listBooks` returned them, which keeps both orderings the engine's.
 */
function byYear(books: StoredBook[], years: Map<number, number>): ShelfGroup[] {
  const seen = [...new Set(books.map((b) => (b.id === null ? null : years.get(b.id))))]
    .filter((y): y is number => y !== undefined && y !== null)
    .sort((a, b) => b - a);

  const groups: ShelfGroup[] = seen.map((year) => ({
    key: String(year),
    heading: String(year),
    books: books.filter((b) => b.id !== null && years.get(b.id) === year),
    captions: false,
  }));

  const rest = books.filter((b) => b.id === null || years.get(b.id) === undefined);
  // The four terminal groups in a fixed order — the two that are a reading in
  // progress, then the one that is a reading with no date, then the one that is
  // no reading. Nothing here counts them and nothing marks them as owed.
  for (const key of ['still-reading', 'put-down', 'undated', 'none']) {
    const inThis = rest.filter((b) => terminalGroup(b).key === key);
    if (inThis.length === 0) continue;
    const { heading, captions } = terminalGroup(inThis[0]!);
    groups.push({ key, heading, books: inThis, captions });
  }
  return groups;
}

/** Consecutive books sharing a key, in the order given. Never a re-sort. */
function runs(
  books: StoredBook[],
  key: (b: StoredBook) => string,
  heading: (k: string) => string,
): ShelfGroup[] {
  const groups: ShelfGroup[] = [];
  for (const book of books) {
    const k = key(book);
    const last = groups[groups.length - 1];
    if (last && last.key === k) last.books.push(book);
    else groups.push({ key: k, heading: heading(k), books: [book], captions: false });
  }
  return groups;
}

const STORAGE_KEY = 'readingbuddy.shelf.arrangement';

/**
 * Which arrangement to open with.
 *
 * A view preference, so it lives in the frontend — the one class of state
 * `gui/CLAUDE.md`'s "no decisions in Svelte" rule does not reach, because the
 * TUI does not want the answer and nothing is derived from it.
 *
 * **An unreadable store is not an error.** A private-mode webview throws on
 * `localStorage` access rather than returning null, and a shelf that failed to
 * render because it could not remember a preference would be the tail wagging
 * the dog. Both halves degrade to the default and say nothing.
 */
export function recallArrangement(): ArrangementId {
  try {
    return arrangementById(localStorage.getItem(STORAGE_KEY)).id;
  } catch {
    return DEFAULT_ARRANGEMENT;
  }
}

export function rememberArrangement(id: ArrangementId): void {
  try {
    localStorage.setItem(STORAGE_KEY, id);
  } catch {
    /* see `recallArrangement` — a preference that cannot be saved is not a failure. */
  }
}
