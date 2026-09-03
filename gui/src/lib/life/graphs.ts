/**
 * The shapes behind *Everything*'s charts.
 *
 * `facets.ts` holds the derivations that read as lists; this holds the ones that
 * read as pictures. Same two caveats apply to every function here — bounded by
 * the caller's row ceiling, closed readings only — and the same line: past-tense
 * description is allowed, forward pressure is not.
 *
 * ## The one arithmetic rule that governs this whole file
 *
 * **`books` is distinct over a period and cannot be summed out of the periods
 * inside it.** Item 42 states it and `MonthActivityDto`'s own doc repeats it: a
 * reader who opened the same two books on twelve days read two books, not
 * twenty-four. So nothing here adds `books` across months — [`seasonality`] is
 * the function that wanted to and is built on `activity_days` instead, which
 * *is* summable because a day belongs to exactly one month.
 *
 * `minutes` and `pages` are nullable at every grain and an absent month is
 * skipped rather than folded to `0`, which is the same rule one level down.
 */
import type { DayActivityDto, MonthActivityDto } from '$lib/api/bindings';
import type { ReadingRow, StoredBook } from '$lib/api/client';

/** One bar: what it is, how big, and what to say about it. */
export type Bar = { key: string; label: string; value: number };

/** One point in a scatter, carrying the book so a tooltip can name it. */
export type Point = { x: number; y: number; book: StoredBook };

/** One bin of a histogram, with the bounds it covers. */
export type Bin = { label: string; from: number; to: number | null; count: number };

/** SECONDS_PER_DAY, named because a bare 86400 in a date expression is a puzzle. */
const DAY = 86_400;

/**
 * Books finished in each month, in time order.
 *
 * The months arrive from the engine and are passed through one for one. Nothing
 * averages: *books per month* as a mean would be a pace, and a pace is a target
 * with the number left off.
 */
export function perMonth(months: MonthActivityDto[]): Bar[] {
  return [...months]
    .sort((a, b) => a.month.localeCompare(b.month))
    .map((m) => ({ key: m.month, label: m.month, value: m.books }));
}

/**
 * The running total of books finished, month by month.
 *
 * A cumulative line is the one chart here that *only* ever goes up, which is
 * exactly why it is safe: there is no month it can make look like a failure. It
 * is the shape of a shelf filling, which is the thing the app is for.
 *
 * It sums `books_finished` **per month from the closed readings**, not
 * `MonthActivityDto.books` — that field is *distinct books with an event*, which
 * is not a count of finishing and cannot be added up (see the module doc). The
 * rows carry one `finished_at` each, so counting them per month is a count of
 * events and adds correctly.
 */
export function cumulative(rows: ReadingRow[]): Bar[] {
  const per = new Map<string, number>();
  for (const row of rows) {
    const at = row.reading.finished_at;
    if (at === null) continue;
    const key = new Date(at * 1000).toISOString().slice(0, 7);
    per.set(key, (per.get(key) ?? 0) + 1);
  }
  let running = 0;
  return [...per.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([key, n]) => {
      running += n;
      return { key, label: key, value: running };
    });
}

/**
 * Days with something on them, by calendar month, across every year in the span.
 *
 * **Built on `activity_days` and deliberately not on `books`.** Summing `books`
 * across two Januaries would double-count a book read in both, which is the
 * exact error item 42 names; days cannot overlap between months, so they add.
 * A month nobody read in is absent from the engine's answer and lands here as a
 * zero-height bar, which is honest: the *calendar month* exists even in a year
 * that carried nothing.
 */
export function seasonality(months: MonthActivityDto[]): Bar[] {
  const NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const by = new Array<number>(12).fill(0);
  for (const m of months) {
    const i = Number(m.month.slice(5, 7)) - 1;
    if (i >= 0 && i < 12) by[i] = by[i]! + m.activity_days;
  }
  return NAMES.map((label, i) => ({ key: label, label, value: by[i]! }));
}

/**
 * A measure the device recorded, month by month — or an empty list.
 *
 * **Months the device never measured are dropped, never drawn as zero.** That is
 * the whole reason these fields are nullable, and a gap in the line is the
 * truthful rendering of it. If nothing in the span was measured the caller gets
 * `[]` and draws no chart at all, rather than a flat line along the axis
 * claiming the reader read for no minutes.
 */
export function measured(months: MonthActivityDto[], which: 'minutes' | 'pages'): Bar[] {
  return [...months]
    .sort((a, b) => a.month.localeCompare(b.month))
    .flatMap((m) => {
      const value = which === 'minutes' ? m.minutes : m.pages;
      return value === null ? [] : [{ key: m.month, label: m.month, value }];
    });
}

/**
 * How long the books were, as a histogram.
 *
 * Fixed bins rather than quantiles: a reader knows what "400–600 pages" means
 * and does not know what their own third quintile is. The top bin is open-ended
 * because one 1,200-page book should not stretch the axis over everything else.
 */
export function lengths(rows: ReadingRow[]): Bin[] {
  const EDGES = [0, 150, 250, 350, 500, 700];
  return histogram(
    rows.flatMap((r) => {
      const p = r.book.page_count;
      // A zero page count is absent, not a book of no length — the same rule
      // `pagesOf` follows, and `make dev-db` carries a `0` to catch a caller
      // that forgets it.
      return p === null || p <= 0 ? [] : [p];
    }),
    EDGES,
    (a, b) => (b === null ? `${a}+` : `${a}–${b}`),
  );
}

/**
 * How many days a read was open, as a histogram.
 *
 * Only readings with **both** ends recorded, and only where the end is not
 * before the beginning — a Goodreads import can carry either. A read that closed
 * the day it opened is one day, not zero, because the reader read on that day.
 *
 * This describes the books, not the reader. There is no "average time to finish"
 * anywhere: an average of this is a pace, and a pace shown beside a current read
 * is a deadline.
 */
export function durations(rows: ReadingRow[]): Bin[] {
  const EDGES = [1, 3, 8, 15, 31, 91];
  return histogram(
    rows.flatMap((r) => {
      const { started_at: from, finished_at: to } = r.reading;
      if (from === null || to === null || to < from) return [];
      return [Math.floor((to - from) / DAY) + 1];
    }),
    EDGES,
    (a, b) => (b === null ? `${a}+` : b === a + 1 ? `${a}` : `${a}–${b - 1}`),
  );
}

/**
 * When the book was written against when you finished it.
 *
 * The most expressive thing on the page and the least summarisable: a column at
 * the right edge is a season spent on new books, a band along the bottom is a
 * year in one century. It states no relationship — there is no trend line, and
 * there must not be one, because a regression through *what you read* is a
 * verdict on taste.
 */
export function pubVsRead(rows: ReadingRow[]): Point[] {
  return rows.flatMap((r) => {
    const year = r.book.publish_year;
    const at = r.reading.finished_at;
    return year === null || at === null ? [] : [{ x: at, y: year, book: r.book }];
  });
}

/**
 * How long a book was against what you gave it.
 *
 * Both axes are facts about one book, so nothing here is a judgement of the
 * reader. Unrated and unmeasured books are absent rather than plotted at zero.
 */
export function lengthVsRating(rows: ReadingRow[]): Point[] {
  return rows.flatMap((r) => {
    const p = r.book.page_count;
    const rating = r.reading.ko_rating;
    return p === null || p <= 0 || rating === null ? [] : [{ x: p, y: rating, book: r.book }];
  });
}

/** One cell of the calendar: the day, and what was on it. */
export type Cell = { day: string; books: number };

/**
 * The days with something on them, as a calendar of weeks.
 *
 * Returns whole ISO weeks from the first day to the last, so the grid is
 * rectangular and a gap is a real gap. Days the engine did not return are
 * **absent from the map**, not zero — the component draws them as empty surface,
 * which is what "nothing was recorded" looks like.
 *
 * All arithmetic is UTC, the engine's own day convention. `getDay` and
 * `getUTCDay` differ by one for every reader west of Greenwich, which would put
 * the whole calendar in the wrong column.
 */
export function calendar(days: DayActivityDto[]): { weeks: (Cell | null)[][]; peak: number } {
  if (days.length === 0) return { weeks: [], peak: 0 };
  const by = new Map(days.map((d) => [d.day, d.books]));
  const sorted = [...by.keys()].sort();
  const first = startOfWeek(sorted[0]!);
  const last = sorted[sorted.length - 1]!;
  const weeks: (Cell | null)[][] = [];
  let cursor = first;
  while (cursor <= last) {
    const week: (Cell | null)[] = [];
    for (let i = 0; i < 7; i += 1) {
      const books = by.get(cursor);
      week.push(books === undefined ? null : { day: cursor, books });
      cursor = shift(cursor, 1);
    }
    weeks.push(week);
  }
  return { weeks, peak: Math.max(...by.values()) };
}

/** The Monday of the ISO week containing `day`, in UTC. */
function startOfWeek(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  // `getUTCDay` is 0 on Sunday; ISO weeks start on Monday, so Sunday steps back
  // six days rather than none.
  const back = (d.getUTCDay() + 6) % 7;
  return shift(day, -back);
}

/** `day` moved by `n` calendar days, in UTC. */
function shift(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/**
 * Bucket values into bins whose last edge is open-ended.
 *
 * Empty bins are **kept**: a histogram with a hole in the middle says something,
 * and dropping the hole would close the gap and change the shape. Bins beyond
 * the data are trimmed from the tail only, so the axis does not run on into
 * lengths nobody read.
 */
function histogram(
  values: number[],
  edges: number[],
  label: (from: number, to: number | null) => string,
): Bin[] {
  if (values.length === 0) return [];
  const bins: Bin[] = edges.map((from, i) => ({
    label: label(from, edges[i + 1] ?? null),
    from,
    to: edges[i + 1] ?? null,
    count: 0,
  }));
  for (const v of values) {
    let i = 0;
    while (i + 1 < edges.length && v >= edges[i + 1]!) i += 1;
    bins[i]!.count += 1;
  }
  let end = bins.length;
  while (end > 1 && bins[end - 1]!.count === 0) end -= 1;
  return bins.slice(0, end);
}
