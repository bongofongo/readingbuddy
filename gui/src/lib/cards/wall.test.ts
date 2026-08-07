/**
 * The wall's arithmetic (item 47).
 *
 * Offsets and a year, which are the only two things on that screen the frontend
 * decides. Which readings exist, in what order, and which read a card is are all
 * the engine's, and are asserted against the fake in `api/fake-rows.test.ts`.
 */
import { describe, expect, it } from 'vitest';

import { PAGE, offsetOf, pageCount, wallFilter, yearSpan } from './wall';

describe('the year, as a span', () => {
  it('is the whole year, both ends', () => {
    expect(yearSpan(2024)).toEqual({ from: '2024-01-01', to: '2024-12-31' });
    expect(yearSpan(2025)).toEqual({ from: '2025-01-01', to: '2025-12-31' });
  });

  /**
   * **The clamp is gone, and item 51 is why.**
   *
   * This used to be `/life`'s `yearRange`, which ends the current year at today
   * — right there, where a year has to stay a subset of the `activityByMonth`
   * span it was grouped out of, and wrong here. The years now come from
   * `readings.finished_at`, and a `finished_at` in the future is reachable: a
   * Goodreads `Date Read`, or a device clock ahead of this machine. Clamped,
   * such a year produced `2027-01-01 … today` — **inverted**, which `DayRange`
   * refuses at the seam, so the picker would offer a year that replaced the wall
   * with an error. The whole year is both correct and un-invertible.
   */
  it('is never inverted, for any year a reading can name', () => {
    for (const y of [1970, 1999, 2024, 2025, 2027]) {
      const span = yearSpan(y);
      expect(span.from <= span.to, `${y} produced an inverted span`).toBe(true);
    }
  });
});

describe('the filter', () => {
  it('is absent for the whole library, not four nulls', () => {
    // `null` is the shape the wire calls *every reading*, and it is what makes
    // the filtered and unfiltered call sites one code path.
    expect(wallFilter({ kind: 'all' })).toBeNull();
  });

  it('names only the year, and asks nothing else', () => {
    expect(wallFilter({ kind: 'year', year: 2024 })).toEqual({
      book_id: null,
      status: null,
      // Deliberately **not** `open: false` beside the year: an open reading has
      // no `finished_at` and fails the span's comparisons already, so saying it
      // twice would be a second spelling of the same predicate.
      open: null,
      finished_in: { from: '2024-01-01', to: '2024-12-31' },
    });
  });

  it('asks for the open reads as open, never as a status', () => {
    // `abandon_reading` leaves a reading open on purpose, so `status: 'reading'`
    // is a narrower question than *has not ended* — and the cards a reader is
    // looking for under this chip include the book they put down.
    expect(wallFilter({ kind: 'open' })).toEqual({
      book_id: null,
      status: null,
      open: true,
      finished_in: null,
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
