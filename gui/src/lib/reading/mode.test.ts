/**
 * Reading mode's rules, where they are checkable rather than reviewable.
 *
 * Two of these are about the axiom rather than about behaviour and they are why
 * the file exists: **no verb may carry a number**, and **a `?book=` nobody can
 * honour must not be a dead end**. Both are one careless line away, and neither
 * is visible in a screenshot of a fixture that happens to have three open reads
 * and a well-formed URL.
 */
import { describe, expect, it } from 'vitest';

import type { OpenReading, StoredBook } from '$lib/api/client';

import { chooseReading, paramBook, parsePage, VERBS } from './mode';

/** An open read, with only the fields this module looks at. */
function open(id: number): OpenReading {
  return {
    book: { id, title: `Book ${id}` } as unknown as StoredBook,
    reading: { id: id * 10 } as unknown as OpenReading['reading'],
  };
}

describe('the four verbs', () => {
  it('are four, and each has a key of its own', () => {
    expect(VERBS).toHaveLength(4);
    expect(new Set(VERBS.map((v) => v.key)).size).toBe(4);
  });

  it('never puts a number on a label', () => {
    // The surface a reader sits on while reading is the last place in the app
    // that may count anything. `Passages` says what is behind it; `Passages (12)`
    // would be the library counting itself with the book open.
    for (const v of VERBS) expect(v.label).not.toMatch(/\d/);
  });

  it('keeps its keys off the modifier-bearing platform shortcuts by being plain letters', () => {
    // The component rejects a modified keystroke before it asks; this is the
    // other half — a key here that was already a bare browser action would be
    // unfixable at the call site.
    for (const v of VERBS) expect(v.key).toMatch(/^[a-z]$/);
  });
});

describe('a page, as typed', () => {
  it('takes a whole number', () => {
    expect(parsePage('214')).toEqual({ page: 214 });
    expect(parsePage('  7 ')).toEqual({ page: 7 });
  });

  it('refuses the three things a reader did not mean, and each refusal names the move', () => {
    for (const raw of ['', 'two hundred', '0', '-4', '21.5']) {
      const got = parsePage(raw);
      expect('refusal' in got, `"${raw}" must not be sent`).toBe(true);
      // A refusal in this app says what would work. An empty string or a bare
      // "invalid" is the shape the whole codebase refuses.
      if ('refusal' in got) expect(got.refusal.length).toBeGreaterThan(10);
    }
  });

  it('refuses a number past what an integer can carry rather than sending Infinity', () => {
    expect('refusal' in parsePage('999999999999999999999')).toBe(true);
  });
});

describe('which read this is', () => {
  it('honours the one the URL named', () => {
    expect(chooseReading([open(1), open(2), open(3)], 2)?.book.id).toBe(2);
  });

  it('falls back to the engine’s first, not to nothing', () => {
    // A read closed in another window is the ordinary way to hold a stale
    // `?book=`. It is not a mistake anybody made and must not be a dead end.
    expect(chooseReading([open(1), open(2)], 99)?.book.id).toBe(1);
    expect(chooseReading([open(1), open(2)], null)?.book.id).toBe(1);
  });

  it('is null only when nothing is open at all', () => {
    expect(chooseReading([], 3)).toBeNull();
  });
});

describe('?book=', () => {
  it('reads a positive integer and nothing else', () => {
    expect(paramBook(new URLSearchParams('book=12'))).toBe(12);
    for (const q of ['', 'book=', 'book=abc', 'book=0', 'book=-3', 'book=1.5']) {
      expect(paramBook(new URLSearchParams(q)), q).toBeNull();
    }
  });
});
