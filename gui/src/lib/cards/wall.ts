/**
 * The wall's arithmetic: a page, a year, and the filter the two are asked with.
 *
 * Pure, so it is tested at layer 1 rather than looked at. Nothing here decides
 * which readings exist, in what order, or which read a card is — those are the
 * engine's (items 17, 41, 43). What is left is offsets, which are the frontend's
 * because the page size is.
 */
import type { DayRangeDto, ReadingFilterDto, ReadingSortDto } from '$lib/api/bindings';

/**
 * How many cards a page holds.
 *
 * A number the frontend owns, and the only reason it may: `limit` is required on
 * the wire and has **no** serde default, precisely so a client states a real page
 * size rather than inheriting one. 24 is three or four rows of a substantial
 * object at a desktop width, which is a page you can skim without losing your
 * place.
 *
 * **`FakeClient` holds ten readings, so nothing below ever pages in a
 * screenshot** — and neither number moves to fix that. The fake's books are the
 * hostile set `crates/corpus/edge-cases.json` declares and its size is not this
 * screen's to choose; shrinking the page until a control appears would be
 * picking a page size to flatter a picture, which is the same mistake wearing
 * the other hat. The arithmetic is unit-tested below and the markup is looked at
 * with `make dev-db` against a real library. Said here rather than left for
 * somebody to discover, because a comment claiming a control is reviewed when it
 * is not is worse than no comment.
 */
export const PAGE = 24;

/** The three orders, in the order the switch offers them. Default first. */
export const SORTS: ReadingSortDto[] = ['finished', 'started', 'last_modified'];

/**
 * One year, as the span the engine's `finished_in` wants — **the whole year**.
 *
 * It used to be `/life`'s `yearRange`, clamped to today, and the shared spelling
 * was the argument: a year is two days and that page had already decided which
 * two. The clamp belongs to `/life` and not here, though, and item 51 is what
 * made the difference reachable. `/life`'s year is a subset of the
 * `activityByMonth` span it was grouped out of, so ending the current year at
 * today keeps the two calls talking about the same days. This span is matched
 * against `finished_at` and the years now come from `readings` themselves, so a
 * `finished_at` in the future — a Goodreads `Date Read`, a device clock ahead of
 * this machine — puts a year in the picker that `yearRange` then clamps into an
 * **inverted** span (`2027-01-01` … today), which `DayRange` refuses and which
 * would replace the wall with an error. *Nothing finishes in the future* was
 * true of the proxy and is not true of the column.
 *
 * The span is sent as-is. **There is deliberately no validation above this
 * seam**: `DayRange` refuses an inverted range at both doors, this cannot
 * construct one, and a second dialect of the rule up here would be a copy that
 * can only drift.
 */
export function yearSpan(year: number): DayRangeDto {
  return { from: `${year}-01-01`, to: `${year}-12-31` };
}

/**
 * What a reader is looking at: the whole wall, one year, or the reads that have
 * not ended.
 *
 * Three cases and **not a nullable year**, because *still reading* is not a
 * year and cannot be spelled as one — an open reading has no `finished_at`, so
 * it is in no year at all, which is the fact `ReadingYearsDto.open` crosses the
 * seam to state. A picker that offered only years would leave those cards
 * reachable from *All* and from nowhere else, and a reader who visited every
 * year in turn would never see the book they are in the middle of.
 */
export type WallScope = { kind: 'all' } | { kind: 'year'; year: number } | { kind: 'open' };

/**
 * What the wall is asking for — `null` for the whole library.
 *
 * `null` rather than a filter with four nulls in it, because that is the shape
 * the wire calls *every reading* and it is what makes the filtered and
 * unfiltered call sites one code path.
 *
 * **The same object goes to the page, the count and the year list**, which is
 * how the three agree: the engine builds all three clauses from one predicate,
 * so a disagreement could only be a caller sending three different filters.
 */
export function wallFilter(scope: WallScope): ReadingFilterDto | null {
  switch (scope.kind) {
    case 'all':
      return null;
    case 'year':
      return { book_id: null, status: null, open: null, finished_in: yearSpan(scope.year) };
    case 'open':
      // `open: true` is `finished_at IS NULL`, which is what open *means* — not
      // a status word. `abandon_reading` leaves a reading open deliberately, so
      // `status: 'reading'` would be a different and narrower question.
      return { book_id: null, status: null, open: true, finished_in: null };
  }
}

/**
 * How many pages a total makes, at a given size — at least one.
 *
 * One even for a total of zero, because an empty wall is still a page you are
 * looking at, and a control reading *page 1 of 0* is arithmetic leaking through
 * a screen.
 */
export function pageCount(total: number, size = PAGE): number {
  return Math.max(1, Math.ceil(total / size));
}

/**
 * The offset a page index starts at, clamped into the pages that exist.
 *
 * The clamp is what stops a filter change stranding the reader: picking a year
 * with three cards while on page four asks for offset 72 and gets an empty list,
 * which reads as *this year has nothing* and is a lie. Callers reset through
 * here rather than trusting the offset they were on.
 */
export function offsetOf(pageIndex: number, total: number, size = PAGE): number {
  const last = pageCount(total, size) - 1;
  return Math.min(Math.max(0, pageIndex), last) * size;
}
