/**
 * The *Everything* tab's derivations.
 *
 * The assertions that matter here are the **negative** ones: that an absence
 * never becomes a zero, that a reread stays two events, and — the one this file
 * exists for — that **a run still going is never returned**. That last is the
 * condition `docs/decisions.md` entry 23 attached to speaking a run at all, and
 * entry 58 kept when it lifted the ranking ban; it is one `<` away from becoming
 * the streak this app is built without.
 *
 * The ordering tests changed meaning with entry 58. They used to assert
 * *alphabetical* order, to prove nothing was ranked. They now assert the
 * ranking is correct **and stable** — a tie broken by insertion order would make
 * the list reshuffle when an unrelated row was edited.
 */
import { describe, expect, it } from 'vitest';

import type { DayActivityDto, HighlightDto, MonthActivityDto } from '$lib/api/bindings';
import type { ReadingRow, StoredBook } from '$lib/api/client';

import {
  authorsOf,
  busiestOf,
  coversOf,
  decadesOf,
  longestOf,
  longestRunOf,
  meanRatingOf,
  pagesOf,
  passageOf,
  ratingsOf,
  rereadsOf,
  subjectsOf,
  trendOf,
} from './facets';

function day(d: string): DayActivityDto {
  return { day: d, books: 1, minutes: null, pages: null };
}

function month(m: string, books: number): MonthActivityDto {
  return { month: m, books, activity_days: books, minutes: null, pages: null };
}

function book(over: Partial<StoredBook> & { id: number }): StoredBook {
  return {
    title: `Book ${over.id}`,
    authors: [],
    authors_display: [],
    subjects: [],
    publish_year: null,
    page_count: null,
    ...over,
  } as unknown as StoredBook;
}

function highlight(id: number): HighlightDto {
  return { id, text: `passage ${id}` } as unknown as HighlightDto;
}

function row(over: {
  id: number;
  finishedAt?: number | null;
  readNumber?: number;
  ofReads?: number;
  rating?: number | null;
  passage?: HighlightDto | null;
  book?: Partial<StoredBook>;
}): ReadingRow {
  return {
    book: book({ id: over.id, ...over.book }),
    reading: {
      id: over.id,
      finished_at: over.finishedAt === undefined ? 1000 + over.id : over.finishedAt,
      ko_rating: over.rating === undefined ? null : over.rating,
    },
    read_number: over.readNumber ?? 1,
    of_reads: over.ofReads ?? 1,
    passage: over.passage ?? null,
  } as unknown as ReadingRow;
}

describe('passageOf', () => {
  it('takes the passage from the most recently finished read, not the first row', () => {
    // Deliberately out of order: the function must compare, not trust position.
    const rows = [
      row({ id: 1, finishedAt: 100, passage: highlight(11) }),
      row({ id: 2, finishedAt: 900, passage: highlight(22) }),
      row({ id: 3, finishedAt: 400, passage: highlight(33) }),
    ];
    expect(passageOf(rows)?.passage.id).toBe(22);
  });

  it('skips rows with no passage and rows that never closed', () => {
    const rows = [
      row({ id: 1, finishedAt: null, passage: highlight(11) }),
      row({ id: 2, finishedAt: 500, passage: null }),
      row({ id: 3, finishedAt: 200, passage: highlight(33) }),
    ];
    expect(passageOf(rows)?.passage.id).toBe(33);
  });

  it('is null when nothing carries one', () => {
    expect(passageOf([row({ id: 1 })])).toBeNull();
  });
});

describe('coversOf', () => {
  it('keeps a reread as two entries — two readings are two events', () => {
    const rows = [
      row({ id: 7, readNumber: 2, ofReads: 2 }),
      row({ id: 7, readNumber: 1, ofReads: 2 }),
    ];
    expect(coversOf(rows)).toHaveLength(2);
  });
});

describe('authorsOf', () => {
  it('ranks by how many of theirs you finished', () => {
    const rows = [
      row({ id: 1, book: { authors_display: ['Zadie Smith'] } }),
      row({ id: 2, book: { authors_display: ['Anne Carson'] } }),
      row({ id: 3, book: { authors_display: ['Zadie Smith'] } }),
      row({ id: 4, book: { authors_display: ['Zadie Smith'] } }),
    ];
    expect(authorsOf(rows)).toEqual([
      { key: 'Zadie Smith', count: 3 },
      { key: 'Anne Carson', count: 1 },
    ]);
  });

  it('breaks a tie alphabetically, so the list cannot reshuffle on an edit', () => {
    // Inserted in reverse. Without the tie-break these come back in Map
    // insertion order, which is an ordering that means nothing and that moves
    // when an unrelated row changes.
    const rows = [
      row({ id: 1, book: { authors_display: ['Zoe'] } }),
      row({ id: 2, book: { authors_display: ['Ada'] } }),
    ];
    expect(authorsOf(rows).map((t) => t.key)).toEqual(['Ada', 'Zoe']);
  });

  it('drops blank and whitespace-only names', () => {
    const rows = [row({ id: 1, book: { authors_display: ['', '   ', 'Real Name'] } })];
    expect(authorsOf(rows)).toEqual([{ key: 'Real Name', count: 1 }]);
  });
});

describe('subjectsOf', () => {
  it('counts books, not mentions — a subject listed twice on one book is one', () => {
    const rows = [
      row({ id: 1, book: { subjects: ['Art', 'Art', 'Travel'] } }),
      row({ id: 2, book: { subjects: ['Art'] } }),
    ];
    expect(subjectsOf(rows)).toEqual([
      { key: 'Art', count: 2 },
      { key: 'Travel', count: 1 },
    ]);
  });
});

describe('longestOf', () => {
  it('is longest first, and a book with no stated length is absent not zero', () => {
    const rows = [
      row({ id: 1, book: { page_count: 200 } }),
      row({ id: 2, book: { page_count: null } }),
      row({ id: 3, book: { page_count: 900 } }),
      row({ id: 4, book: { page_count: 0 } }),
    ];
    expect(longestOf(rows).map((x) => x.pages)).toEqual([900, 200]);
  });
});

describe('meanRatingOf', () => {
  it('averages over rated readings only — an unrated read is not a zero', () => {
    const rows = [row({ id: 1, rating: 4 }), row({ id: 2, rating: null }), row({ id: 3, rating: 2 })];
    expect(meanRatingOf(rows)).toEqual({ mean: 3, of: 2 });
  });

  it('is null when nothing was rated', () => {
    expect(meanRatingOf([row({ id: 1, rating: null })])).toBeNull();
  });
});

describe('trendOf', () => {
  it('passes the months through one for one, in order, summing nothing', () => {
    const months = [month('2025-01', 2), month('2025-02', 5)];
    expect(trendOf(months)).toEqual([
      { key: '2025-01', count: 2 },
      { key: '2025-02', count: 5 },
    ]);
  });
});

describe('busiestOf', () => {
  it('finds the month holding the most books', () => {
    const months = [month('2025-01', 2), month('2025-02', 5), month('2025-03', 3)];
    expect(busiestOf(months)?.month).toBe('2025-02');
  });

  it('gives a tie to the earlier month, so the answer does not move', () => {
    const months = [month('2025-01', 4), month('2025-02', 4)];
    expect(busiestOf(months)?.month).toBe('2025-01');
  });

  it('is null over no months', () => {
    expect(busiestOf([])).toBeNull();
  });
});

describe('longestRunOf', () => {
  const TODAY = '2025-06-01';

  it('finds the longest run of consecutive days', () => {
    const days = [
      day('2025-03-04'),
      day('2025-03-05'),
      day('2025-03-06'),
      // broken
      day('2025-03-10'),
      day('2025-03-11'),
    ];
    expect(longestRunOf(days, TODAY)).toEqual({ from: '2025-03-04', to: '2025-03-06', days: 3 });
  });

  it('does not join days across a gap', () => {
    // Five days present but one missing in the middle. A derivation that
    // forgot adjacency reports 5; the answer is 3.
    const days = ['2025-03-04', '2025-03-05', '2025-03-06', '2025-03-08', '2025-03-09'].map(day);
    expect(longestRunOf(days, TODAY)?.days).toBe(3);
  });

  it('refuses a run that is still going — this is the streak guard', () => {
    // The run reaches today, so it is not over and must not be shown. This is
    // the condition decisions.md entry 23 attached to speaking a run at all.
    const days = [day('2025-05-30'), day('2025-05-31'), day(TODAY)];
    expect(longestRunOf(days, TODAY)).toBeNull();
  });

  it('refuses a run reaching past today, which a device clock can produce', () => {
    const days = [day('2025-06-01'), day('2025-06-02')];
    expect(longestRunOf(days, TODAY)).toBeNull();
  });

  it('still reports an older finished run when a live one exists', () => {
    const days = [
      day('2025-03-04'),
      day('2025-03-05'),
      day('2025-03-06'),
      day('2025-05-31'),
      day(TODAY),
    ];
    expect(longestRunOf(days, TODAY)?.days).toBe(3);
  });

  it('does not call a single day a run', () => {
    expect(longestRunOf([day('2025-01-01')], TODAY)).toBeNull();
  });

  it('crosses a month boundary correctly', () => {
    const days = [day('2025-01-30'), day('2025-01-31'), day('2025-02-01')];
    expect(longestRunOf(days, TODAY)).toEqual({ from: '2025-01-30', to: '2025-02-01', days: 3 });
  });
});

describe('rereadsOf', () => {
  it('keeps only reads past the first, with the engine’s own numbering', () => {
    const rows = [
      row({ id: 1, readNumber: 1, ofReads: 1 }),
      row({ id: 2, readNumber: 3, ofReads: 3 }),
    ];
    expect(rereadsOf(rows)).toEqual([
      { book: expect.objectContaining({ id: 2 }), readNumber: 3, ofReads: 3 },
    ]);
  });
});

describe('ratingsOf', () => {
  it('sorts by the rating, never by the count', () => {
    const rows = [
      row({ id: 1, rating: 5 }),
      row({ id: 2, rating: 3 }),
      row({ id: 3, rating: 3 }),
      row({ id: 4, rating: 3 }),
      row({ id: 5, rating: 4 }),
    ];
    // By size this would be 3, 5, 4. By value it is 3, 4, 5 — and it must be
    // the second, or the panel has silently become a ranking.
    expect(ratingsOf(rows).map((t) => t.key)).toEqual(['3', '4', '5']);
    expect(ratingsOf(rows).find((t) => t.key === '3')?.count).toBe(3);
  });

  it('omits unrated readings rather than giving them a bar', () => {
    const rows = [row({ id: 1, rating: null }), row({ id: 2, rating: 4 })];
    expect(ratingsOf(rows)).toEqual([{ key: '4', count: 1 }]);
  });
});

describe('decadesOf', () => {
  it('buckets by decade, oldest first', () => {
    const rows = [
      row({ id: 1, book: { publish_year: 1999 } }),
      row({ id: 2, book: { publish_year: 1972 } }),
      row({ id: 3, book: { publish_year: 1978 } }),
    ];
    expect(decadesOf(rows)).toEqual([
      { key: '1970s', count: 2 },
      { key: '1990s', count: 1 },
    ]);
  });

  it('omits a book with no year rather than inventing an "unknown" bucket', () => {
    const rows = [row({ id: 1, book: { publish_year: null } })];
    expect(decadesOf(rows)).toEqual([]);
  });
});

describe('pagesOf', () => {
  it('reports what it summed and over how many, so the total cannot mislead', () => {
    const rows = [
      row({ id: 1, book: { page_count: 300 } }),
      row({ id: 2, book: { page_count: null } }),
      row({ id: 3, book: { page_count: 120 } }),
    ];
    expect(pagesOf(rows)).toEqual({ pages: 420, stated: 2, total: 3 });
  });

  it('treats a zero page count as absent, not as a book of no length', () => {
    // `make dev-db` carries a book with `page_count: 0` to catch exactly this.
    const rows = [row({ id: 1, book: { page_count: 0 } }), row({ id: 2, book: { page_count: 90 } })];
    expect(pagesOf(rows)).toEqual({ pages: 90, stated: 1, total: 2 });
  });
});
