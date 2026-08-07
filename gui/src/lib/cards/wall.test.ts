/**
 * The wall's arithmetic (item 47).
 *
 * Offsets and a year, which are the only two things on that screen the frontend
 * decides. Which readings exist, in what order, and which read a card is are all
 * the engine's, and are asserted against the fake in `api/fake-rows.test.ts`.
 */
import { describe, expect, it } from 'vitest';

import { PAGE, offsetOf, pageCount, wallFilter, yearSpan } from './wall';

const TODAY = new Date('2025-08-07T00:00:00Z');

describe('the year, as a span', () => {
  it('is the whole of a year that has finished', () => {
    expect(yearSpan(2024, TODAY)).toEqual({ from: '2024-01-01', to: '2024-12-31' });
  });

  it('stops at today in the year that has not', () => {
    // Not defensive coding: it is `$lib/life/period`'s clamp, borrowed rather
    // than respelled, and it keeps the current year a genuine subset of the span
    // the years themselves came from.
    expect(yearSpan(2025, TODAY)).toEqual({ from: '2025-01-01', to: '2025-08-07' });
  });

  /**
   * Every year a picker can offer produces a span the engine will accept.
   *
   * The one refusal lives at the seam — `DayRange` rejects an inverted span from
   * both doors — and this is why there is no second copy of it up here: the
   * picker cannot construct one.
   *
   * **A year in the future can**, and that is a finding rather than a hole.
   * `yearRange` clamps only the `to` end, so `yearRange(2030, today)` is
   * `2030-01-01 … today` — backwards. It is unreachable from either screen: the
   * years come from `activityByMonth`, a log of what has happened, so a future
   * year is not in the list. Guarding it here would be a guard that cannot fire,
   * which is this repo's own complaint; and if a wrong clock ever produced one,
   * the engine refuses it and the wall renders its failure state, which names
   * the move. So this asserts the reachable range and says the rest out loud.
   */
  it('is never inverted for a year that has begun', () => {
    for (const y of [1970, 1999, 2024, 2025]) {
      const span = yearSpan(y, TODAY);
      expect(span.from <= span.to, `${y} produced an inverted span`).toBe(true);
    }
  });
});

describe('the filter', () => {
  it('is absent for the whole library, not four nulls', () => {
    // `null` is the shape the wire calls *every reading*, and it is what makes
    // the filtered and unfiltered call sites one code path.
    expect(wallFilter(null, TODAY)).toBeNull();
  });

  it('names only the year, and asks nothing else', () => {
    expect(wallFilter(2024, TODAY)).toEqual({
      book_id: null,
      status: null,
      // Deliberately **not** `open: false` beside the year: an open reading has
      // no `finished_at` and fails the span's comparisons already, so saying it
      // twice would be a second spelling of the same predicate.
      open: null,
      finished_in: { from: '2024-01-01', to: '2024-12-31' },
    });
  });
});

describe('paging', () => {
  it('is one page even for nothing, so no control reads "of 0"', () => {
    expect(pageCount(0)).toBe(1);
  });

  it('does not open a second page for an exact fit', () => {
    expect(pageCount(PAGE)).toBe(1);
    expect(pageCount(PAGE + 1)).toBe(2);
  });

  it('clamps into the pages that exist', () => {
    // The case this exists for: the reader is on page four, picks a year with
    // three cards, and an unclamped offset of 72 returns nothing — which reads
    // as *this year is empty* and is a lie about the filter, not the year.
    expect(offsetOf(3, 3)).toBe(0);
    expect(offsetOf(1, PAGE * 3)).toBe(PAGE);
    expect(offsetOf(9, PAGE * 3)).toBe(PAGE * 2);
    expect(offsetOf(-2, PAGE * 3)).toBe(0);
  });
});
