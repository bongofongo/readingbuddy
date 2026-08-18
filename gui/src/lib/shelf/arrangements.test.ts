/**
 * The wall's groups — the module that decides where a book lands, and the one
 * place a layout decision in this app is checkable rather than reviewable.
 *
 * Two of these assertions are about the **axiom** rather than about layout, and
 * they are the reason this file exists at all: a book with no reading may not
 * appear under `Author` or `Title`, and no group heading may carry a number.
 * Both are one careless line away at any time, and neither is visible in a
 * screenshot of a fixture that happens not to contain the case.
 */
import { describe, expect, it } from 'vitest';

import type { StoredBook } from '$lib/api/client';

import {
  arrangementById,
  ARRANGEMENTS,
  DEFAULT_ARRANGEMENT,
  finishYears,
  recallArrangement,
  shelfGroups,
} from './arrangements';

/** A book with only the fields the wall reads. */
function book(id: number, over: Partial<StoredBook> = {}): StoredBook {
  return {
    id,
    title: `Book ${id}`,
    sort_title: `Book ${id}`,
    authors_display: ['Ann Author'],
    reading_state: { state: 'finished' },
    ...over,
  } as StoredBook;
}

/** A closed reading row, as `listReadingRows` hands one back. */
function row(bookId: number, finishedAt: number | null) {
  return { book: { id: bookId }, reading: { finished_at: finishedAt } };
}

const JAN_2026 = Date.UTC(2026, 0, 3) / 1000;
const JUN_2025 = Date.UTC(2025, 5, 9) / 1000;
const DEC_2024 = Date.UTC(2024, 11, 31) / 1000;

describe('finishYears', () => {
  it('takes the most recent close of a reread, not the first row it sees', () => {
    // The rows arrive newest-finish-first, so the *second* mention of book 1 is
    // an earlier read of it. A reread belongs on the wall once, in the year you
    // last finished it — which is where a shelf would put it.
    const years = finishYears([row(1, JAN_2026), row(2, JUN_2025), row(1, DEC_2024)]);
    expect(years.get(1)).toBe(2026);
    expect(years.get(2)).toBe(2025);
  });

  it("is UTC, which is the engine's own day convention", () => {
    // 2024-12-31 is 2025 in half the world's local time. The engine dates
    // everything in UTC and `dayLabel` phrases it in UTC; a wall that grouped in
    // local time would put a book in a year its own detail page denies.
    expect(finishYears([row(1, DEC_2024)]).get(1)).toBe(2024);
  });

  it('ignores a row with no finish date rather than grouping it under 1970', () => {
    // The caller filters to closed readings, so this cannot arrive — and that is
    // exactly why the guard is asserted rather than trusted.
    expect(finishYears([row(1, null)]).size).toBe(0);
  });
});

describe('shelfGroups, by year', () => {
  const years = new Map([
    [1, 2026],
    [2, 2025],
    [3, 2026],
  ]);

  it("puts the years newest first and keeps the engine's order inside one", () => {
    const groups = shelfGroups([book(3), book(1), book(2)], years, 'year');
    expect(groups.map((g) => g.heading)).toEqual(['2026', '2025']);
    // 3 before 1, which is the order `listBooks` returned them in. A group is a
    // cut of the engine's list, never a re-sort of it.
    expect(groups[0]!.books.map((b) => b.id)).toEqual([3, 1]);
  });

  it('sends each kind of unfinished book to a group that is true about it', () => {
    const groups = shelfGroups(
      [
        book(10, { reading_state: { state: 'reading' } }),
        book(11, { reading_state: { state: 'abandoned' } }),
        book(12, { reading_state: null }),
        book(13, { reading_state: { state: 'finished' } }),
      ],
      new Map(),
      'year',
    );
    expect(groups.map((g) => g.heading)).toEqual([
      'Still reading',
      'Put down',
      'Read, undated',
      'No reading recorded',
    ]);
  });

  it('captions only the group of books you have not read', () => {
    // The year groups are recognition surfaces and a caption there is a text row
    // per tile for nothing. "No reading recorded" is an identification surface —
    // a cover you have never seen, at 86px, is not an identifier.
    const groups = shelfGroups([book(1), book(9, { reading_state: null })], years, 'year');
    expect(groups.find((g) => g.heading === '2026')!.captions).toBe(false);
    expect(groups.find((g) => g.heading === 'No reading recorded')!.captions).toBe(true);
  });

  it('puts no number on any heading, in any arrangement', () => {
    // The rule, asserted: a row of groups each carrying a figure is a scoreboard,
    // and this is the surface `docs/decisions.md` forbids one on by name.
    const books = [book(1), book(2), book(9, { reading_state: null })];
    for (const id of ARRANGEMENTS.map((a) => a.id)) {
      for (const group of shelfGroups(books, years, id)) {
        if (/^\d{4}$/.test(group.heading)) continue; // a year *is* the heading
        expect(group.heading, 'a count on a group heading is a scoreboard').not.toMatch(/\d/);
      }
    }
  });
});

describe('shelfGroups, by author and title', () => {
  it('leaves out books with no reading, because they have no answer to give', () => {
    // The finding this module exists to hold: under `Year` a book with no
    // reading is quarantined at the bottom of a long scroll, and under `Author`
    // it interleaves with books you finished — which is the backlog rendering,
    // arriving without a label or a number changing. Adjacency is the mechanism.
    const books = [book(1), book(9, { reading_state: null }), book(2)];
    for (const id of ['author', 'title'] as const) {
      const shown = shelfGroups(books, new Map(), id).flatMap((g) => g.books.map((b) => b.id));
      expect(shown).not.toContain(9);
      expect(shown).toEqual([1, 2]);
    }
  });

  it("cuts a group where the engine's order changes initial, and never re-sorts", () => {
    const books = [
      book(1, { authors_display: ['Ann Author'] }),
      book(2, { authors_display: ['Alan Other'] }),
      book(3, { authors_display: ['Bea Writer'] }),
      // Back to A, which a *sort* would move up and a *run* leaves where it is.
      book(4, { authors_display: ['Anne Later'] }),
    ];
    const groups = shelfGroups(books, new Map(), 'author');
    expect(groups.map((g) => [g.heading, g.books.map((b) => b.id)])).toEqual([
      ['A', [1, 2]],
      ['B', [3]],
      ['A', [4]],
    ]);
  });

  it('keeps a non-Latin initial rather than bucketing the shelf as Latin', () => {
    const groups = shelfGroups(
      [book(1, { sort_title: '海辺のカフカ' }), book(2, { sort_title: '「引用」' })],
      new Map(),
      'title',
    );
    expect(groups.map((g) => g.heading)).toEqual(['海', '#']);
  });

  it('falls back to the title when there is no sort title, and to # when there is neither', () => {
    const groups = shelfGroups(
      [book(1, { sort_title: null, title: 'Zed' }), book(2, { sort_title: null, title: null })],
      new Map(),
      'title',
    );
    expect(groups.map((g) => g.heading)).toEqual(['Z', '#']);
  });
});

describe('the registry', () => {
  it('does not offer Recent', () => {
    // Deliberate, and the reason is written in the module: recency of finishing
    // is what `Year` already puts at the top, and recency of *adding* is the
    // backlog sort by name.
    expect(ARRANGEMENTS.map((a) => a.id)).not.toContain('recent');
  });

  it('never asks the engine for the publication year', () => {
    // `BookSort::Year` is the year the book was *published*, which is a different
    // fact from the year a reading closed. They must not share a name and this
    // wall must not accidentally ask for the wrong one.
    expect(ARRANGEMENTS.map((a) => a.sort)).not.toContain('year');
  });

  it('falls back to the default rather than throwing on an unknown id', () => {
    expect(arrangementById('spine-shelf').id).toBe(DEFAULT_ARRANGEMENT);
    expect(arrangementById(null).id).toBe(DEFAULT_ARRANGEMENT);
  });

  it('degrades to the default when the store cannot be read', () => {
    // A private-mode webview throws on `localStorage` access rather than
    // returning null, and a shelf that failed to render because it could not
    // remember a preference would be the tail wagging the dog.
    const store = globalThis.localStorage;
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      get() {
        throw new Error('denied');
      },
    });
    expect(recallArrangement()).toBe(DEFAULT_ARRANGEMENT);
    Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: store });
  });
});
