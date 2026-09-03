/**
 * The chart derivations.
 *
 * As in `facets.test.ts`, the assertions that earn their place are the negative
 * ones — a `books` count is never summed across months, an unmeasured month
 * never becomes a zero, a zero page count is never a book of no length, and the
 * calendar never invents a day the engine did not return.
 */
import { describe, expect, it } from 'vitest';

import type { DayActivityDto, MonthActivityDto } from '$lib/api/bindings';
import type { ReadingRow, StoredBook } from '$lib/api/client';

import {
  calendar,
  cumulative,
  durations,
  lengths,
  lengthVsRating,
  measured,
  perMonth,
  pubVsRead,
  seasonality,
} from './graphs';

const DAY = 86_400;

function month(m: string, over: Partial<MonthActivityDto> = {}): MonthActivityDto {
  return { month: m, books: 1, activity_days: 1, minutes: null, pages: null, ...over };
}

function day(d: string, books = 1): DayActivityDto {
  return { day: d, books, minutes: null, pages: null };
}

function row(over: {
  id: number;
  pages?: number | null;
  year?: number | null;
  rating?: number | null;
  startedAt?: number | null;
  finishedAt?: number | null;
}): ReadingRow {
  return {
    book: {
      id: over.id,
      title: `Book ${over.id}`,
      page_count: over.pages === undefined ? null : over.pages,
      publish_year: over.year === undefined ? null : over.year,
    } as unknown as StoredBook,
    reading: {
      started_at: over.startedAt === undefined ? null : over.startedAt,
      finished_at: over.finishedAt === undefined ? null : over.finishedAt,
      ko_rating: over.rating === undefined ? null : over.rating,
    },
  } as unknown as ReadingRow;
}

describe('perMonth', () => {
  it('is in time order however the engine returned them', () => {
    expect(perMonth([month('2025-03'), month('2025-01')]).map((b) => b.key)).toEqual([
      '2025-01',
      '2025-03',
    ]);
  });
});

describe('cumulative', () => {
  it('runs from the readings, not from MonthActivityDto.books', () => {
    // Two readings closing in January, one in March. `books` on the month DTO
    // is "distinct books with an event" and is a different question entirely.
    const rows = [
      row({ id: 1, finishedAt: Date.parse('2025-01-05T00:00:00Z') / 1000 }),
      row({ id: 2, finishedAt: Date.parse('2025-01-20T00:00:00Z') / 1000 }),
      row({ id: 3, finishedAt: Date.parse('2025-03-02T00:00:00Z') / 1000 }),
    ];
    expect(cumulative(rows)).toEqual([
      { key: '2025-01', label: '2025-01', value: 2 },
      { key: '2025-03', label: '2025-03', value: 3 },
    ]);
  });

  it('only ever goes up', () => {
    const rows = [
      row({ id: 1, finishedAt: Date.parse('2025-01-05T00:00:00Z') / 1000 }),
      row({ id: 2, finishedAt: Date.parse('2025-02-05T00:00:00Z') / 1000 }),
      row({ id: 3, finishedAt: Date.parse('2025-03-05T00:00:00Z') / 1000 }),
    ];
    const values = cumulative(rows).map((b) => b.value);
    expect(values).toEqual([...values].sort((a, b) => a - b));
  });

  it('skips a reading that never closed', () => {
    expect(cumulative([row({ id: 1, finishedAt: null })])).toEqual([]);
  });
});

describe('seasonality', () => {
  it('adds days and never books — two Januaries can hold the same book', () => {
    const months = [
      month('2025-01', { books: 3, activity_days: 4 }),
      month('2026-01', { books: 3, activity_days: 5 }),
    ];
    const jan = seasonality(months).find((b) => b.key === 'Jan');
    // Days add: 4 + 5. Books do not — 3 + 3 would double-count a book read in
    // both Januaries, which is the error item 42 names.
    expect(jan?.value).toBe(9);
    expect(jan?.value).not.toBe(6);
  });

  it('always returns twelve months, so the axis is a year', () => {
    const bars = seasonality([month('2025-06', { activity_days: 2 })]);
    expect(bars).toHaveLength(12);
    expect(bars.find((b) => b.key === 'Jan')?.value).toBe(0);
  });
});

describe('measured', () => {
  it('drops an unmeasured month rather than drawing it as zero', () => {
    const months = [
      month('2025-01', { minutes: 60 }),
      month('2025-02', { minutes: null }),
      month('2025-03', { minutes: 0 }),
    ];
    // February is absent; March is a *measured* zero and stays — item 31's
    // twenty-second session is the device saying something.
    expect(measured(months, 'minutes').map((b) => [b.key, b.value])).toEqual([
      ['2025-01', 60],
      ['2025-03', 0],
    ]);
  });

  it('is empty when nothing in the span was measured, so no chart is drawn', () => {
    expect(measured([month('2025-01'), month('2025-02')], 'pages')).toEqual([]);
  });
});

describe('lengths', () => {
  it('bins by page count and keeps an empty bin in the middle', () => {
    const rows = [row({ id: 1, pages: 100 }), row({ id: 2, pages: 120 }), row({ id: 3, pages: 300 })];
    const bins = lengths(rows);
    expect(bins[0]).toMatchObject({ from: 0, count: 2 });
    // 150–250 has nothing in it and must survive: a hole in a histogram says
    // something, and closing it changes the shape.
    expect(bins[1]).toMatchObject({ from: 150, count: 0 });
    expect(bins[2]).toMatchObject({ from: 250, count: 1 });
  });

  it('treats a zero page count as absent', () => {
    expect(lengths([row({ id: 1, pages: 0 })])).toEqual([]);
  });
});

describe('durations', () => {
  it('counts a same-day read as one day, not zero', () => {
    const at = Date.parse('2025-01-05T00:00:00Z') / 1000;
    expect(durations([row({ id: 1, startedAt: at, finishedAt: at })])[0]).toMatchObject({
      from: 1,
      count: 1,
    });
  });

  it('skips a reading whose end is before its beginning', () => {
    const at = Date.parse('2025-01-05T00:00:00Z') / 1000;
    expect(durations([row({ id: 1, startedAt: at, finishedAt: at - DAY })])).toEqual([]);
  });

  it('skips a reading missing either end', () => {
    const at = Date.parse('2025-01-05T00:00:00Z') / 1000;
    expect(durations([row({ id: 1, startedAt: at, finishedAt: null })])).toEqual([]);
  });
});

describe('pubVsRead and lengthVsRating', () => {
  it('drop a book missing either axis rather than plotting it at zero', () => {
    const at = Date.parse('2025-01-05T00:00:00Z') / 1000;
    expect(pubVsRead([row({ id: 1, year: null, finishedAt: at })])).toEqual([]);
    expect(lengthVsRating([row({ id: 1, pages: 300, rating: null })])).toEqual([]);
    expect(lengthVsRating([row({ id: 1, pages: 0, rating: 4 })])).toEqual([]);
  });
});

describe('calendar', () => {
  it('starts the grid on a Monday and returns whole weeks', () => {
    // 2025-01-08 is a Wednesday; its week begins Monday 2025-01-06.
    const { weeks } = calendar([day('2025-01-08')]);
    expect(weeks[0]![0]).toBeNull();
    expect(weeks.every((w) => w.length === 7)).toBe(true);
    expect(weeks.flat().find((c) => c !== null)?.day).toBe('2025-01-08');
  });

  it('leaves an unrecorded day null rather than zero', () => {
    const { weeks } = calendar([day('2025-01-06'), day('2025-01-08')]);
    const tuesday = weeks.flat().find((c) => c?.day === '2025-01-07');
    expect(tuesday).toBeUndefined();
  });

  it('steps a Sunday back six days, not none', () => {
    // 2025-01-12 is a Sunday and belongs to the week starting Monday the 6th.
    const { weeks } = calendar([day('2025-01-12')]);
    expect(weeks).toHaveLength(1);
    expect(weeks[0]![6]?.day).toBe('2025-01-12');
  });

  it('is empty over no days', () => {
    expect(calendar([])).toEqual({ weeks: [], peak: 0 });
  });
});
