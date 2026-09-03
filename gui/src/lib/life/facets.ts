/**
 * What a period held, read off the readings that closed in it.
 *
 * ## Why these live above the seam, and what that costs
 *
 * Item 17's rule is that derived facts belong in the engine. These are
 * derivations — distinct authors, a rating tally, a partial page sum — and the
 * engine has **no request for any of them**, so they are built here from rows
 * `/life` already fetches for a different reason (`listReadingRows` with a
 * `finished_in` filter, which the months' finishing sentence needs).
 *
 * That is a compromise and it is written down rather than hidden, the way
 * `latest.ts` records its N+1. What it means concretely:
 *
 * - Every figure here is **bounded by `READINGS_PAGE`**. A period with more
 *   closed readings than the page ceiling gives short answers, and nothing in
 *   this file can tell that it happened. An engine-side aggregate would not
 *   have that failure mode. This is the strongest argument for moving them
 *   down, and it is the reason none of these numbers is presented as a total of
 *   the library.
 * - They are all over **closed readings only**. A book you are in the middle of
 *   is in none of them, which is correct for a page about what a period held
 *   and would be wrong for anything else.
 *
 * ## Ranking is now allowed here, and the reason is where it is drawn
 *
 * An earlier version of this file forbade it outright: sets sorted
 * alphabetically, distributions on their own axis, never by size, on the
 * argument that a frequency-ordered list "announces a winner nobody entered".
 * That file also said the reversal would be "a product decision and not a sort
 * order" — and `docs/decisions.md` entry 58 is that decision.
 *
 * What changed is not the app's opinion about leaderboards; it is **where these
 * figures are drawn**. Everything in this module now feeds the *Everything* tab,
 * which a reader opens on purpose and which the page does not start on. The
 * timeline the page does open on carries none of it. Material you go and ask for
 * is a different act from material you are met with, and that difference is the
 * whole permission.
 *
 * ## The line that did not move
 *
 * The axiom is *"the app tells you what you did; it never tells you what you
 * have left"*, and that is about **forward pressure**, not about description. So
 * every function here may say precisely what happened, in the past tense, and
 * none of them may produce a goal, a target, a pace, an "on track", a "behind",
 * or any count of what is undone. Two consequences worth stating because they
 * are easy to lose:
 *
 * - **A run of days is only a fact once it is over.** [`longestRunOf`] refuses
 *   any run touching today. That is not caution; it is the exact condition
 *   `docs/decisions.md` entry 23 attached to `run_ended` when it permitted a run
 *   to be spoken at all — a run still going is something a reader can be made to
 *   feel they must protect.
 * - **Nothing here is a rate.** *Books per month* as an average would be a pace,
 *   and a pace is a target with the number left off. The trend is the months
 *   themselves, drawn as they were.
 *
 * ## What every figure here inherits
 *
 * These are built above the seam from rows fetched for another purpose, so:
 * every one is **bounded by the caller's row ceiling** and cannot tell when it
 * was truncated, and every one is over **closed readings only**. An engine-side
 * aggregate would have neither limitation, and that remains the right home for
 * them.
 */
import type { DayActivityDto, HighlightDto, MonthActivityDto } from '$lib/api/bindings';
import type { ReadingRow, StoredBook } from '$lib/api/client';

/** A rereading: which read this was, and of what. */
export type Reread = { book: StoredBook; readNumber: number; ofReads: number };

/** One bar of a distribution: what it is, and how many. */
export type Tally = { key: string; count: number };

/** The partial page sum, and the honesty that has to travel with it. */
export type Pages = {
  /** Summed over the books that state a length. */
  pages: number;
  /** How many books stated one. */
  stated: number;
  /** How many closed in the period at all. */
  total: number;
};

/**
 * One passage from the period, or `null`.
 *
 * `passage` is item 44's chosen highlight and is already on every row, so this
 * costs nothing. The pick is **the most recently finished reading that has
 * one** rather than a random draw: a random passage changes on every render and
 * makes the panel look broken, and "the last book you closed" is the one a
 * reader is most likely to recognise.
 *
 * `rows` arrive sorted by `finished`, so the first hit is that book — but the
 * order is not assumed, because a caller passing an unsorted list would
 * silently get a different meaning. The comparison is explicit.
 */
export function passageOf(rows: ReadingRow[]): { passage: HighlightDto; book: StoredBook } | null {
  let best: { passage: HighlightDto; book: StoredBook; at: number } | null = null;
  for (const row of rows) {
    if (row.passage === null) continue;
    const at = row.reading.finished_at;
    if (at === null) continue;
    if (best === null || at > best.at) best = { passage: row.passage, book: row.book, at };
  }
  return best === null ? null : { passage: best.passage, book: best.book };
}

/**
 * The books that closed, newest first, one entry per reading.
 *
 * A reread appears twice on purpose: two readings of one book are two things
 * that happened, and a wall of covers that silently deduplicated them would be
 * a different claim from the one the rereads panel makes a few inches away.
 */
export function coversOf(rows: ReadingRow[]): StoredBook[] {
  return rows.map((r) => r.book);
}

/**
 * Count the strings each row contributes, most first.
 *
 * The tie-break is **alphabetical**, and it is not cosmetic: `Map` iteration is
 * insertion-ordered, so without it two authors with three books each would be
 * ranked by which one the engine happened to return first — an order that means
 * nothing and that changes when a row is edited. A stable list is what makes it
 * readable as a fact rather than as noise.
 */
function tallyOf(rows: ReadingRow[], pick: (row: ReadingRow) => string[]): Tally[] {
  const by = new Map<string, number>();
  for (const row of rows) {
    // Per row, not per occurrence: a book listing the same subject twice is one
    // book about it. `Set` also makes the figure "books" rather than "mentions",
    // which is what the label claims.
    for (const raw of new Set(pick(row))) {
      const key = raw.trim();
      if (key === '') continue;
      by.set(key, (by.get(key) ?? 0) + 1);
    }
  }
  return [...by.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([key, count]) => ({ key, count }));
}

/**
 * Everyone whose book you finished, most first.
 *
 * `authors_display` is the engine's parse (item 17) and `authors` is the
 * record — `gui/CLAUDE.md` says to join the first and never reorder the second,
 * so this reads the display form.
 */
export function authorsOf(rows: ReadingRow[]): Tally[] {
  return tallyOf(rows, (row) => row.book.authors_display);
}

/**
 * What the period was about, most first.
 *
 * Providers are generous with subjects and a busy year can produce hundreds, so
 * a caller is expected to cap what it draws. The cap is the component's, not
 * this function's: truncating here would make the count and the list disagree
 * for any caller that wanted both.
 */
export function subjectsOf(rows: ReadingRow[]): Tally[] {
  return tallyOf(rows, (row) => row.book.subjects);
}

/**
 * The longest books you finished, longest first.
 *
 * A fact about the books and not about the reading: how long a book is has
 * nothing to do with how well it was read, which is what keeps this clear of
 * the effort framing. A book with no stated length is absent rather than sorted
 * as zero, and a zero is treated as absent for [`pagesOf`]'s reason.
 */
export function longestOf(rows: ReadingRow[]): { book: StoredBook; pages: number }[] {
  return rows
    .flatMap((row) => {
      const pages = row.book.page_count;
      return pages === null || pages <= 0 ? [] : [{ book: row.book, pages }];
    })
    .sort((a, b) => b.pages - a.pages);
}

/**
 * The books you went back to — every closed reading that was not the first.
 *
 * `read_number` and `of_reads` are the engine's (item 41) and neither is
 * optional, so there is no arithmetic here: the filter is the whole function.
 * Newest first, which is the order the rows arrive in.
 */
export function rereadsOf(rows: ReadingRow[]): Reread[] {
  return rows
    .filter((r) => r.read_number > 1)
    .map((r) => ({ book: r.book, readNumber: r.read_number, ofReads: r.of_reads }));
}

/**
 * The ratings you gave, by rating value.
 *
 * **Sorted by the rating, never by the count** — see the module doc. There is
 * no average and there must not be one: a mean rating is a single number that
 * summarises your taste as a score, which is the scoreboard this page is
 * written to avoid. Readings with no rating are simply absent; a `0` bar for
 * "unrated" would put the books you did not rate on the same axis as the ones
 * you did.
 *
 * `ko_rating` is the device's own scale and is an `i64` on the wire, so the key
 * is its decimal spelling and the caller phrases it.
 */
export function ratingsOf(rows: ReadingRow[]): Tally[] {
  const by = new Map<number, number>();
  for (const row of rows) {
    const rating = row.reading.ko_rating;
    if (rating === null) continue;
    by.set(rating, (by.get(rating) ?? 0) + 1);
  }
  return [...by.entries()].sort((a, b) => a[0] - b[0]).map(([k, count]) => ({ key: String(k), count }));
}

/**
 * The mean of the ratings you gave, or `null` when you gave none.
 *
 * Over **rated readings only** — an unrated read is not a zero, and folding it
 * in would be the `null`-as-`0` lie item 42 exists to prevent, applied to taste.
 * The caller is expected to print the count beside it, because a mean of one
 * rating is not a mean.
 */
export function meanRatingOf(rows: ReadingRow[]): { mean: number; of: number } | null {
  const given = rows.flatMap((r) => (r.reading.ko_rating === null ? [] : [r.reading.ko_rating]));
  if (given.length === 0) return null;
  return { mean: given.reduce((a, b) => a + b, 0) / given.length, of: given.length };
}

/**
 * The months of the period, as they were — the trend.
 *
 * A pass-through with a rename, and it is here so the component does not reach
 * into `MonthActivityDto` and start folding: **months are not summable**
 * (`books` is distinct over each) and this returns them one for one, in the
 * order the engine gave them. Nothing averages, and there is deliberately no
 * "books per month" figure — an average over time is a pace, and a pace is a
 * target with the number left off.
 */
export function trendOf(months: MonthActivityDto[]): Tally[] {
  return months.map((m) => ({ key: m.month, count: m.books }));
}

/**
 * The month that held the most books, or `null`.
 *
 * This is the "best month" an earlier version of this file refused, and it is
 * permitted now for `docs/decisions.md` entry 58's reason: it is on a tab the
 * reader opened on purpose, and it is a description of the past with no target
 * beside it. The word the caller uses is *busiest* and never *best* — the first
 * says what happened, the second grades it.
 *
 * Ties go to the **earliest** month, so the answer does not move when a later
 * month draws level. Arbitrary either way; being stable is what matters.
 */
export function busiestOf(months: MonthActivityDto[]): MonthActivityDto | null {
  let best: MonthActivityDto | null = null;
  for (const m of months) if (best === null || m.books > best.books) best = m;
  return best;
}

/**
 * The longest run of consecutive days with something on them — **once it is
 * over**.
 *
 * A run whose last day is today, or later, is **not returned**. That is the
 * whole safety of this function and it is not caution: `docs/decisions.md` entry
 * 23 permitted a run to be spoken at all only on the condition that it is
 * recognised after it ends, because a run still going is a thing a reader can be
 * made to feel they must protect — which is the streak this app is built without.
 * Entry 58 carries that condition forward rather than dropping it.
 *
 * A run of one day is not a run; the floor is two, for entry 23's reason again —
 * two is what *consecutive* means, and a 3 or a 7 would be this module deciding
 * what counts as enough reading.
 *
 * `days` arrive as `YYYY-MM-DD` and only days carrying an event exist, so the
 * gaps are the absences. Dates are compared in UTC — the engine's own day
 * convention — and never through the local-time `Date` accessors, which would
 * shift a run by a day for every reader west of Greenwich.
 */
export function longestRunOf(
  days: DayActivityDto[],
  today: string,
): { from: string; to: string; days: number } | null {
  const sorted = [...new Set(days.map((d) => d.day))].sort();
  let best: { from: string; to: string; days: number } | null = null;
  let start = 0;
  // From 1, so there is always a previous day to compare against — at `i === 0`
  // there is none, and reaching for one is how this read `sorted[-1]` and handed
  // `isNextDay` an undefined date.
  for (let i = 1; i <= sorted.length; i += 1) {
    const broken = i === sorted.length || !isNextDay(sorted[i - 1]!, sorted[i]!);
    if (!broken) continue;
    const from = sorted[start]!;
    const to = sorted[i - 1]!;
    const length = i - start;
    // Still running, so not yet a fact. `<` against today and not `!==`: a
    // fixture or a device clock can carry a day in the future, and a run
    // reaching past today is running by any reading of it.
    if (length >= 2 && to < today && (best === null || length > best.days)) {
      best = { from, to, days: length };
    }
    start = i;
  }
  return best;
}

/** Whether `b` is the calendar day after `a`, both `YYYY-MM-DD`, in UTC. */
function isNextDay(a: string, b: string): boolean {
  const next = new Date(`${a}T00:00:00Z`);
  next.setUTCDate(next.getUTCDate() + 1);
  return next.toISOString().slice(0, 10) === b;
}

/**
 * Which decades the books themselves came from, oldest first.
 *
 * A fact about the shelf rather than about the reader, which is what makes it
 * safe to draw as a distribution: nobody's decade is better than anybody's. A
 * book with no `publish_year` is absent rather than bucketed into an *unknown*
 * bar, for `Months`' reason — an absence is not a value.
 */
export function decadesOf(rows: ReadingRow[]): Tally[] {
  const by = new Map<number, number>();
  for (const row of rows) {
    const year = row.book.publish_year;
    if (year === null) continue;
    const decade = Math.floor(year / 10) * 10;
    by.set(decade, (by.get(decade) ?? 0) + 1);
  }
  return [...by.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([k, count]) => ({ key: `${k}s`, count }));
}

/**
 * Pages, over the books that state a length.
 *
 * The three numbers travel together because the sum alone is a lie by
 * omission: `page_count` is nullable, a period's books may state it unevenly,
 * and a figure that says *4,120 pages* over eleven books when only six of them
 * carry a length is the same class of error as folding `null` into `0` — which
 * is the error item 42 exists to prevent, one grain up. The caller must say
 * `stated` of `total`.
 *
 * A zero `page_count` is treated as absent, not as a book of no length: the
 * engine already collapses a zero length to absence elsewhere (`Preview`'s rail
 * relies on it) and one book in `make dev-db` carries a `0` precisely to catch
 * a caller that does not.
 */
export function pagesOf(rows: ReadingRow[]): Pages {
  let pages = 0;
  let stated = 0;
  for (const row of rows) {
    const count = row.book.page_count;
    if (count === null || count <= 0) continue;
    pages += count;
    stated += 1;
  }
  return { pages, stated, total: rows.length };
}
