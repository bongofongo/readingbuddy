/**
 * The spans, and the grouping that must never become an aggregation.
 *
 * `today` is injected into every one of these, which is the reason it is a
 * parameter at all: a suite that read the clock would pass in August and be a
 * different test in December.
 */
import { describe, expect, it } from 'vitest';

import type { MonthActivityDto } from '$lib/api/bindings';
import { dayOf, wholeLife, yearRange, yearsOf } from './period';

const TODAY = new Date('2025-05-14T09:00:00Z');

function month(m: string, over: Partial<MonthActivityDto> = {}): MonthActivityDto {
  return { month: m, books: 2, activity_days: 5, minutes: 100, pages: 60, ...over };
}

describe('the spans a reading-life page asks about', () => {
  it("is UTC, which is the engine's own day convention", () => {
    // 23:30 UTC is the next day locally in half the world and the day before in
    // the other half. The engine's `day` is UTC and item 31 refused to correct
    // for the machine's offset, so this must not either.
    expect(dayOf(new Date('2025-05-14T23:30:00Z'))).toBe('2025-05-14');
  });

  it('reaches back further than any real reading, and forward to today', () => {
    // Not "five years back": a Goodreads export can carry a book read in 1998,
    // and a page called *your reading life* that quietly began in 2021 would be
    // lying about the part it dropped. Only months with an event come back, so
    // the width is free.
    expect(wholeLife(TODAY)).toEqual({ from: '1970-01-01', to: '2025-05-14' });
  });

  it('gives a past year its whole twelve months', () => {
    expect(yearRange(2024, TODAY)).toEqual({ from: '2024-01-01', to: '2024-12-31' });
  });

  /**
   * The current year stops at today, and that is not defensive coding.
   *
   * The months come from one call over the whole life and a year's figures from
   * another over the year, and item 42 warns that two calls whose spans disagree
   * *"would quietly disagree"* about a month at the edge. Ending the current
   * year in December would ask about days outside the span the months came from.
   */
  it('never asks about days that have not happened', () => {
    expect(yearRange(2025, TODAY)).toEqual({ from: '2025-01-01', to: '2025-05-14' });
  });

  it('produces a span the engine will accept, never an inverted one', () => {
    // `DayRange::new` refuses `from > to` because a backwards range selects
    // nothing and every aggregate reports a confident, wrong zero.
    for (const y of [1970, 2024, 2025]) {
      const r = yearRange(y, TODAY);
      expect(r.from <= r.to, `${y} inverted`).toBe(true);
    }
  });
});

describe('months are grouped by year, and nothing is folded', () => {
  const MONTHS = [
    month('2024-11'),
    month('2024-12'),
    month('2025-01'),
    month('2025-02'),
    month('2025-03'),
  ];

  it('reads the year off the string rather than through a date function', () => {
    // `substr(day, 1, 7)` is the whole grouping in SQL because the string is
    // fixed-width and zero-padded; the same property is used again here rather
    // than a second date function that could disagree with the one upstream.
    expect(yearsOf(MONTHS).map((y) => y.year)).toEqual([2025, 2024]);
  });

  it("keeps the engine's order inside a year, oldest first", () => {
    const y2025 = yearsOf(MONTHS).find((y) => y.year === 2025);
    expect(y2025?.months.map((m) => m.month)).toEqual(['2025-01', '2025-02', '2025-03']);
  });

  /**
   * The rule this module exists to hold.
   *
   * `books` is **distinct over a period**, so a year's is not the sum of its
   * months' — a reader who opened the same two books in twelve months read two
   * books that year, not twenty-four. A `Year` therefore carries no totals at
   * all, and the page asks `activitySummary` for them.
   */
  it('carries no totals of its own', () => {
    const [y] = yearsOf(MONTHS);
    expect(Object.keys(y ?? {}).sort()).toEqual(['months', 'year']);
  });

  it('drops nothing, and invents no empty year between two that have data', () => {
    // Only months carrying an event come back, so a gap year has no months and
    // therefore no heading. Inventing one would be drawing a zero.
    const sparse = [month('2019-04'), month('2025-01')];
    expect(yearsOf(sparse).map((y) => y.year)).toEqual([2025, 2019]);
  });

  it('is empty for an empty log rather than guessing at this year', () => {
    expect(yearsOf([])).toEqual([]);
  });
});
