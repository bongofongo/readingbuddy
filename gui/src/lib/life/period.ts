/**
 * The spans the reading-life page asks about, and the years it found.
 *
 * Two small pure functions with one rule between them: **nothing here folds a
 * month into a year.** That is not a style preference — item 42 states the
 * reason twice over. `minutes` collapses to `0` on the first `reduce`, which is
 * the lie the nullability exists to refuse; and `books` is *distinct over the
 * whole month*, so a year's book count cannot be recovered from its months at
 * all — a reader who opened the same two books in twelve months read two books
 * that year, not twenty-four.
 *
 * So [`yearsOf`] groups and counts nothing, and a year's totals come from
 * [`yearRange`] handed to `activitySummary`, which is one request per year the
 * data actually has. That is a handful of calls to draw a reading life, and it
 * is the *correct* handful: item 42's rejected option was an `ActivitySummary`
 * per **month** — sixty round trips for five years, each answering a slightly
 * different question.
 */
import type { MonthActivityDto } from '$lib/api/bindings';

/**
 * The first day the log could possibly hold.
 *
 * `DayRange::new` refuses anything that is not `YYYY-MM-DD` and refuses an
 * inverted span, and nothing else — so a floor far below any real reading is
 * legal and cheap. It is deliberately not "five years back": a reader whose
 * Goodreads export carries a book they read in 1998 has that in the log, and a
 * page called *your reading life* that quietly began in 2021 would be lying
 * about the part it dropped.
 *
 * Only months carrying an event come back, so the width costs nothing.
 */
const FLOOR = '1970-01-01';

/** A day, `YYYY-MM-DD`, on the engine's own UTC convention. */
export function dayOf(when: Date): string {
  return when.toISOString().slice(0, 10);
}

/**
 * Everything recorded, up to and including today.
 *
 * `today` is injected rather than read from the clock so a suite can pin it —
 * and so nothing in this module has an opinion about what timezone the reader
 * is in. Item 31 left the day skew unfixed on exactly that ground: correcting
 * by the machine's offset makes an answer depend on where the laptop is.
 */
export function wholeLife(today: Date): { from: string; to: string } {
  return { from: FLOOR, to: dayOf(today) };
}

/**
 * One year, clamped so it never asks about days that have not happened.
 *
 * The clamp is not defensive coding: `activityByMonth` is asked over the whole
 * life while a year's summary is asked over the year, and item 42 warns that
 * two calls whose spans disagree *"would quietly disagree"* about a month at the
 * edge. Ending the current year at today rather than at December keeps the year
 * a genuine subset of the span the months came from.
 */
export function yearRange(year: number, today: Date): { from: string; to: string } {
  const end = `${year}-12-31`;
  const now = dayOf(today);
  return { from: `${year}-01-01`, to: end > now ? now : end };
}

/** One year of the page, with the months that carried an event. Newest first. */
export type Year = {
  year: number;
  /** Oldest first inside the year, which is the order the engine returned. */
  months: MonthActivityDto[];
};

/**
 * The months, grouped by the year in their own `month` string.
 *
 * A grouping, not an aggregation — read the module doc. The year is taken from
 * the first four characters of `YYYY-MM` for `substr(day, 1, 7)`'s own reason:
 * the string is fixed-width and zero-padded, so its prefix *is* its year, and a
 * second date function here could disagree with the one that produced it.
 *
 * Input order is preserved inside each year (the engine returns oldest first);
 * the years themselves come back newest first, because a reading life is read
 * from where you are now backwards.
 */
export function yearsOf(months: MonthActivityDto[]): Year[] {
  const by = new Map<number, MonthActivityDto[]>();
  for (const m of months) {
    const year = Number(m.month.slice(0, 4));
    if (Number.isNaN(year)) continue;
    const bucket = by.get(year);
    if (bucket) bucket.push(m);
    else by.set(year, [m]);
  }
  return [...by.entries()]
    .map(([year, ms]) => ({ year, months: ms }))
    .sort((a, b) => b.year - a.year);
}
