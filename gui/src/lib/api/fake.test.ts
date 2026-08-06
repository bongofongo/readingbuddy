/**
 * The frontend's half of item 38 — one fixture, two consumers.
 *
 * `crates/corpus/edge-cases.json` declares the hostile set once. `devdb.rs`'s
 * `the_edge_cases_are_the_declared_ones` asserts the database fixture against
 * it; this asserts the frontend's. Neither fixture is the declaration, and
 * nothing generates it — a declaration generated from one of the two things it
 * governs would agree with that one no matter how wrong it was, which is
 * `crates/corpus`' own rule about oracles.
 *
 * Before this pair existed, adding a hostile case to one fixture and not the
 * other failed nowhere. It had already happened, in both directions: `fake.ts`
 * carried a twenty-first case (`reading_state: other`) the database had no
 * counterpart for, and the database's covers had no counterpart here at all.
 *
 * The declaration is imported rather than read through `node:fs`, so this suite
 * needs no node types and states no path-walking of its own; `vite.config.ts`
 * allows the one directory it lives in.
 */
import { describe, expect, it } from 'vitest';

import declaration from '../../../../crates/corpus/edge-cases.json';
import { FakeClient } from './fake';
import type { StoredBook } from './client';

type Declared = {
  id: number;
  title: string;
  authors: string[];
  page_count: number | null;
  cover: boolean;
  state: 'unread' | 'reading' | 'finished' | 'abandoned' | 'reread' | 'other';
  series: 'none' | 'indexed' | 'unnumbered';
  subjects: number;
  what: string;
};

const DECLARED = declaration.cases as Declared[];

const client = new FakeClient();

/** Every book the fake serves, which is the declared set and nothing else. */
async function books(): Promise<StoredBook[]> {
  return client.listBooks(500);
}

/** The one book a declared case names, or a failure that says which is missing. */
function at(all: StoredBook[], id: number): StoredBook {
  const b = all.find((x) => x.id === id);
  if (!b) throw new Error(`no fixture book with id ${id}`);
  return b;
}

/**
 * The declaration's state vocabulary against what a `BookDto` actually carries.
 *
 * `reread` is the one that is not a `reading_state` at all: the state reports
 * the *open* reading, and what makes a reread a reread is that `listReadings`
 * returns two. So it is checked through the readings, which is the only place
 * that fact lives.
 */
function stateOf(b: StoredBook): string {
  return b.reading_state === null ? 'unread' : b.reading_state.state;
}

function seriesOf(b: StoredBook): string {
  if (b.series === null) return 'none';
  return b.series_index === null ? 'unnumbered' : 'indexed';
}

describe('the fake models exactly the declared edge cases', () => {
  it('has one book per declared case, in the declared order', async () => {
    const all = await books();
    // Length first, and against the *whole* list: a case removed from the
    // declaration then fails as loudly as one added. That is the direction a
    // per-case loop misses, and the direction the two fixtures would otherwise
    // drift apart by deletion rather than by addition.
    expect(all.length).toBe(DECLARED.length);
    expect(all.map((b) => b.id)).toEqual(DECLARED.map((d) => d.id));
  });

  it.each(DECLARED.map((d) => [d.id, d.title || '(untitled)', d] as const))(
    'case %i (%s) has the declared shape',
    async (id, _label, want) => {
      const b = at(await books(), id);

      expect(b.title, 'title').toBe(want.title);
      expect(b.authors, 'authors').toEqual(want.authors);
      expect(b.page_count, 'page_count').toBe(want.page_count);
      expect(seriesOf(b), 'series').toBe(want.series);
      expect(b.subjects.length, 'subjects').toBe(want.subjects);

      if (want.state === 'reread') {
        // The open reading is what `reading_state` reports; the second reading
        // is the whole fact, and item 28 mints a card per reading.
        expect(stateOf(b), 'a reread reports its open reading').toBe('reading');
        expect((await client.listReadings(id)).length, 'a reread has two readings').toBe(2);
      } else {
        expect(stateOf(b), 'state').toBe(want.state);
      }
    },
  );
});

describe('a cover-bearing row carries the whole cluster', () => {
  it('states all four cover fields, or none of them', async () => {
    const all = await books();
    for (const want of DECLARED) {
      const b = at(all, want.id);
      const cluster = [b.cover_path, b.cover_shelf_path, b.cover_aspect, b.cover_accent];
      const stated = cluster.filter((v) => v !== null).length;
      expect(stated, `case ${want.id}: all four cover fields or none`).toBe(want.cover ? 4 : 0);
    }
  });

  it('falls back to the original when there is no shelf tier', async () => {
    // The **rule**, not the fixture. `make dev-db` writes 240×360 covers — both
    // dimensions under `images::THUMB_MAX` (400) — so no tier is written and
    // `Book::shelf_cover_path` yields the original. A test asserting a thumb
    // *exists* would be asserting the fixture instead, and would go red the day
    // the seed's covers changed size for an unrelated reason.
    for (const b of await books()) {
      if (b.cover_path === null) continue;
      expect(b.cover_shelf_path, `case ${b.id}`).toBe(b.cover_path);
    }
  });

  it('ties width_source to whether an aspect was measured', async () => {
    // `EditionShape` reads `Some(aspect) => Recorded`, `None => (PAPERBACK_ASPECT,
    // Assumed)`, so these two cannot disagree in the engine and must not here.
    // This is what stops a later edit giving a book a `cover_aspect` and leaving
    // its shape still claiming it guessed.
    for (const b of await books()) {
      const measured = b.cover_aspect !== null;
      expect(b.shape.width_source, `case ${b.id}`).toBe(measured ? 'recorded' : 'assumed');
    }
  });

  it('withholds the bytes and only the bytes', async () => {
    // Layer 2 has no asset protocol, so a URL here would render as a broken
    // image. The *shape* still has to cross — that is the half item 26 builds
    // on, and the half that was missing entirely until item 38.
    const withCover = (await books()).find((b) => b.cover_path !== null);
    expect(withCover, 'some declared case is cover-bearing').toBeDefined();
    expect(withCover?.cover_aspect, 'and it reserves a box').not.toBeNull();
    expect(withCover?.cover_accent, 'and names a placeholder colour').not.toBeNull();
    expect(client.coverSrc(), 'while the bytes stay withheld').toBeNull();
  });
});

describe('the untitled case', () => {
  it('is the empty string, which is the only absence a stored title can be', async () => {
    // `books.title` is `TEXT NOT NULL DEFAULT ''`, so a stored title is never
    // null however `Option`-shaped `BookDto` is. This fixture modelled `null`
    // until item 38 — a state no library can produce.
    const all = await books();
    expect(all.filter((b) => b.title === '').length).toBe(1);
    expect(all.some((b) => b.title === null)).toBe(false);
  });
});
