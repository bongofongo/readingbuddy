/**
 * The wall's row list, against the fake (items 43, 41, 47).
 *
 * These are the assertions the year picker and the read ordinal would otherwise
 * rest on a screenshot for. The fake states the engine's clauses rather than
 * re-deriving them (see `fake.ts`), so what is pinned here is that the two agree
 * about the four things a wall can get wrong: which rows a filter matched, what
 * the count says about the same filter, where a page starts, and what number a
 * card puts on a read.
 */
import { describe, expect, it } from 'vitest';

import { FakeClient } from './fake';

const YEAR_2024 = { from: '2024-01-01', to: '2024-12-31' };
const YEAR_2025 = { from: '2025-01-01', to: '2025-12-31' };
const YEAR_2023 = { from: '2023-01-01', to: '2023-12-31' };

function filter(over: Partial<Parameters<FakeClient['countReadings']>[0] & object> = {}) {
  return { book_id: null, status: null, open: null, finished_in: null, ...over };
}

describe('the read number', () => {
  it('is 1-based over every reading of the book', async () => {
    const c = new FakeClient();
    const rows = await c.listReadingRows({ limit: -1, filter: filter({ book_id: 12 }) });
    expect(rows.map((r) => r.read_number).sort()).toEqual([1, 2]);
    expect(rows.every((r) => r.of_reads === 2)).toBe(true);
  });

  it('says one read is one read, which is what makes an ordinal absent', async () => {
    // `ReadCount::ordinal` is "a lone read has no number", so the frontend's
    // whole test is `of_reads > 1`. A fixture where every book had two would
    // never render the ordinary card.
    const c = new FakeClient();
    const rows = await c.listReadingRows({ limit: -1, filter: filter({ book_id: 3 }) });
    expect(rows).toHaveLength(1);
    expect(rows[0]?.of_reads).toBe(1);
  });

  it('survives a filter that hides the first read', async () => {
    // Item 43's correction, restated: `ROW_NUMBER() OVER (PARTITION BY …)` is
    // computed over the rows that survived the `WHERE`, so a wall filtered to
    // 2024 would hold the reread's *first* read alone and be right by accident.
    // Filter to the open one instead — its sibling is excluded, and it must
    // still be the second.
    const c = new FakeClient();
    const rows = await c.listReadingRows({
      limit: -1,
      filter: filter({ book_id: 12, open: true }),
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]?.read_number).toBe(2);
    expect(rows[0]?.of_reads).toBe(2);
  });
});

describe('the passage on a row', () => {
  it('is the one a single card would have asked for', async () => {
    // The engine picks both from `card_passage_order`, so the wall and one card
    // cannot show a different sentence for the same reading. The fake must not
    // be the place that becomes untrue.
    const c = new FakeClient();
    const rows = await c.listReadingRows({ limit: -1, filter: filter({ book_id: 12 }) });
    for (const row of rows) {
      expect(row.passage?.id).toBe((await c.cardPassage(row.reading.id))?.id);
    }
  });

  it('is null for a read whose marks are all unattributed', async () => {
    const c = new FakeClient();
    const rows = await c.listReadingRows({ limit: -1, filter: filter({ book_id: 11 }) });
    expect(rows[0]?.passage).toBeNull();
  });
});

describe('the year filter', () => {
  it('selects on when a read finished, and agrees with its count', async () => {
    const c = new FakeClient();
    for (const year of [YEAR_2023, YEAR_2024, YEAR_2025]) {
      const f = filter({ finished_in: year });
      const rows = await c.listReadingRows({ limit: -1, filter: f });
      expect(await c.countReadings(f), `${year.from} disagreed`).toBe(rows.length);
      for (const r of rows) {
        expect(r.reading.finished_at).not.toBeNull();
      }
    }
  });

  it('partitions the library rather than merely matching it', async () => {
    // A fixture where every finish fell in one year would pass a filter that
    // ignored the span entirely.
    const c = new FakeClient();
    const in2024 = await c.countReadings(filter({ finished_in: YEAR_2024 }));
    const in2025 = await c.countReadings(filter({ finished_in: YEAR_2025 }));
    expect(in2024).toBeGreaterThan(0);
    expect(in2025).toBeGreaterThan(0);
    expect(in2024).not.toBe(await c.countReadings(null));
  });

  it('offers a year the wall has nothing for, and that is not an error', async () => {
    // A year you read in and closed nothing in. It is the state the empty wall
    // words, and it is reachable because the years come from the activity log
    // rather than from the readings themselves.
    const c = new FakeClient();
    expect(await c.countReadings(filter({ finished_in: YEAR_2023 }))).toBe(0);
    expect(await c.listReadingRows({ limit: 24, filter: filter({ finished_in: YEAR_2023 }) })).toEqual(
      [],
    );
  });

  it('excludes an open reading from every year', async () => {
    const c = new FakeClient();
    const all = await c.listReadingRows({ limit: -1, filter: null });
    const open = all.filter((r) => r.reading.finished_at === null);
    expect(open.length, 'the fixture has open reads to exclude').toBeGreaterThan(0);
    const in2024 = await c.listReadingRows({ limit: -1, filter: filter({ finished_in: YEAR_2024 }) });
    for (const r of in2024) expect(r.reading.finished_at).not.toBeNull();
  });

  it('refuses an inverted span at both doors', async () => {
    // Fallible where `BookFilter` is not, and only because of the year: a
    // backwards range is an `InvalidInput` rather than a confident empty wall.
    const c = new FakeClient();
    const f = filter({ finished_in: { from: '2025-12-31', to: '2025-01-01' } });
    await expect(c.listReadingRows({ limit: 24, filter: f })).rejects.toThrow();
    await expect(c.countReadings(f)).rejects.toThrow();
  });
});

describe('paging', () => {
  it('is an offset, and the pages partition the list', async () => {
    const c = new FakeClient();
    const all = await c.listReadingRows({ limit: -1, filter: null });
    expect(all.length).toBeGreaterThan(4);
    const first = await c.listReadingRows({ limit: 4, offset: 0 });
    const second = await c.listReadingRows({ limit: 4, offset: 4 });
    expect(first.map((r) => r.reading.id)).toEqual(all.slice(0, 4).map((r) => r.reading.id));
    expect(second.map((r) => r.reading.id)).toEqual(all.slice(4, 8).map((r) => r.reading.id));
    // No reading on two pages and none on neither, which is what a total order
    // buys and what a missing tie-break silently costs.
    expect(new Set([...first, ...second].map((r) => r.reading.id)).size).toBe(8);
  });

  it('reads a negative limit as no limit and zero as a page of nothing', async () => {
    const c = new FakeClient();
    expect((await c.listReadingRows({ limit: -1 })).length).toBe(await c.countReadings(null));
    expect(await c.listReadingRows({ limit: 0 })).toEqual([]);
  });
});

describe('the three orders', () => {
  it('puts the most recently finished first and the open reads last', async () => {
    const c = new FakeClient();
    const rows = await c.listReadingRows({ limit: -1, sort: 'finished' });
    const ends = rows.map((r) => r.reading.finished_at);
    const closed = ends.filter((e): e is number => e !== null);
    expect(closed.length).toBeGreaterThan(1);
    // Every closed read comes before every open one — where a read that has not
    // ended belongs on a list of reads that did.
    expect(ends.slice(0, closed.length).every((e) => e !== null)).toBe(true);
    expect([...closed].sort((a, b) => b - a)).toEqual(closed);
  });

  it('orders the other two descending on their own key', async () => {
    const c = new FakeClient();
    const started = await c.listReadingRows({ limit: -1, sort: 'started' });
    const begun = started.map((r) => r.reading.started_at ?? r.reading.created_at);
    expect([...begun].sort((a, b) => b - a)).toEqual(begun);

    const touched = await c.listReadingRows({ limit: -1, sort: 'last_modified' });
    const at = touched.map((r) => r.reading.last_modified);
    expect([...at].sort((a, b) => b - a)).toEqual(at);
  });

  it('holds the same rows whichever order is asked for', async () => {
    const c = new FakeClient();
    const ids = async (sort: 'finished' | 'started' | 'last_modified') =>
      new Set((await c.listReadingRows({ limit: -1, sort })).map((r) => r.reading.id));
    expect(await ids('started')).toEqual(await ids('finished'));
    expect(await ids('last_modified')).toEqual(await ids('finished'));
  });
});

describe('the count', () => {
  it('agrees with the page for every filter the wall can send', async () => {
    const c = new FakeClient();
    const filters = [
      null,
      filter({ book_id: 12 }),
      filter({ open: true }),
      filter({ open: false }),
      filter({ finished_in: YEAR_2024 }),
      filter({ finished_in: YEAR_2025 }),
      filter({ book_id: 12, finished_in: YEAR_2024 }),
      filter({ status: { state: 'finished' } }),
    ];
    for (const f of filters) {
      const rows = await c.listReadingRows({ limit: -1, filter: f });
      expect(await c.countReadings(f), JSON.stringify(f)).toBe(rows.length);
    }
  });
});
