/**
 * The two routing decisions, which are the ones a component would otherwise
 * take three times: where a book goes, and what *back* means.
 */
import { describe, expect, it } from 'vitest';

import { backTarget, bookHref, HOME, LIBRARY, readingHref } from './nav';

function book(id: number | null, state: string | null) {
  return {
    id,
    reading_state: state === null ? null : ({ state } as never),
  } as Parameters<typeof bookHref>[0];
}

describe('bookHref', () => {
  it('opens a book you are in the middle of into reading mode', () => {
    expect(bookHref(book(3, 'reading'))).toBe(readingHref(3));
  });

  it('opens every other state onto the book page', () => {
    // Finished, put down, a status the engine did not recognise, and a book
    // with no reading at all: four different facts, one destination. Reading
    // mode follows the engine's open set, and a book that is not in it has
    // nothing for that route to show.
    for (const state of ['finished', 'abandoned', 'other', null]) {
      expect(bookHref(book(7, state))).toBe('/book/7');
    }
  });

  it('sends a book with no id to the library rather than to /book/null', () => {
    expect(bookHref(book(null, 'reading'))).toBe(LIBRARY);
  });
});

describe('backTarget', () => {
  const here = new URL('http://x/reading?book=3');

  it('names the page you came from and keeps its query string', () => {
    expect(backTarget(here, new URL('http://x/library?a=1'))).toEqual({
      href: '/library?a=1',
      label: 'Library',
    });
  });

  it('falls back to the entrance when there is no previous page', () => {
    // A reload, a pasted URL, or the window opening straight into this route.
    expect(backTarget(here, null)).toEqual({ href: HOME, label: 'Reading now' });
  });

  it('refuses to point at the surface you are standing on', () => {
    // Reading mode switching book writes `?book=`, and so does the book page
    // opening a note. Back to here is the dead end this whole module exists to
    // stop, so it degrades to the entrance instead.
    expect(backTarget(here, new URL('http://x/reading?book=9')).href).toBe(HOME);
  });

  it('does not name anywhere outside the app', () => {
    expect(backTarget(here, new URL('https://openlibrary.org/x')).href).toBe(HOME);
  });

  it('has a word for every place in the nav, and a plain one for the rest', () => {
    const label = (path: string) => backTarget(here, new URL(`http://x${path}`)).label;
    expect(label('/')).toBe('Reading now');
    expect(label('/notes')).toBe('Notes');
    expect(label('/cards')).toBe('Cards');
    expect(label('/life')).toBe('Reading life');
    expect(label('/devices')).toBe('Devices');
    expect(label('/book/12')).toBe('The book');
    // The longer path must not be eaten by the book page's own pattern, which
    // is why that one is anchored and ordered after this.
    expect(label('/book/12/cards')).toBe('The cards');
    expect(label('/somewhere-later')).toBe('Back');
  });
});
