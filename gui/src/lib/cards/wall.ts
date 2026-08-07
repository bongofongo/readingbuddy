/**
 * The wall's arithmetic: a page, a year, and the filter the two are asked with.
 *
 * Pure, so it is tested at layer 1 rather than looked at. Nothing here decides
 * which readings exist, in what order, or which read a card is — those are the
 * engine's (items 17, 41, 43). What is left is offsets, which are the frontend's
 * because the page size is.
 */
import type { DayRangeDto, ReadingFilterDto, ReadingSortDto } from '$lib/api/bindings';
import { yearRange } from '$lib/life/period';

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
 * One year, as the span the engine's `finished_in` wants.
 *
 * **`yearRange` from `$lib/life/period`, not a second spelling of it.** A year is
 * two days, and the reading-life page already had to decide which two — including
 * the clamp to today, which is not defensive coding: it keeps the current year a
 * genuine subset of the span it sits inside. `finished_in` matches `finished_at`
 * and nothing finishes in the future, so the clamp is free here and the shared
 * spelling is the point.
 *
 * The span is sent as-is. **There is deliberately no validation above this
 * seam**: `DayRange` refuses an inverted range at both doors, a year picker
 * cannot construct one, and a second dialect of the rule up here would be a copy
 * that can only drift.
 */
export function yearSpan(year: number, today: Date): DayRangeDto {
  return yearRange(year, today);
}

/**
 * What the wall is asking for — `null` for the whole library.
 *
 * `null` rather than a filter with four nulls in it, because that is the shape
 * the wire calls *every reading* and it is what makes the filtered and
 * unfiltered call sites one code path.
 */
export function wallFilter(year: number | null, today: Date): ReadingFilterDto | null {
  if (year === null) return null;
  return { book_id: null, status: null, open: null, finished_in: yearSpan(year, today) };
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
