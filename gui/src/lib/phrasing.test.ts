import { describe, expect, it } from 'vitest';

import { authorsLabel, readingStateLabel, seriesLabel, titleLabel } from './phrasing';

describe('reading state', () => {
  it('gives a book nobody has opened no label at all', () => {
    // Not "Unread". The commonest state in a real library, and labelling it
    // frames the shelf as a list of things not done — which is the framing
    // `docs/decisions.md` bans by name.
    expect(readingStateLabel(null)).toBeNull();
  });

  it('never styles putting a book down as failure', () => {
    const label = readingStateLabel('abandoned');
    expect(label).toBe('Put down');
    expect(label?.toLowerCase()).not.toContain('fail');
    expect(label?.toLowerCase()).not.toContain('did not finish');
  });

  it('shows an unknown status verbatim rather than inventing a word', () => {
    // `reading_status` crosses as a string precisely because an importer can
    // write a status this build does not know. Guessing at it would be worse
    // than repeating it.
    expect(readingStateLabel('paused-by-some-other-app')).toBe('paused-by-some-other-app');
  });
});

describe('titles and authors', () => {
  it('names an untitled book rather than rendering an empty line', () => {
    // A sidecar-seeded book with no `doc_props.title` really has none, and the
    // dev library holds one on purpose.
    expect(titleLabel(null)).toBe('Untitled');
    expect(titleLabel('   ')).toBe('Untitled');
  });

  it('leaves a stored author string exactly as stored', () => {
    // Turning `Borges, Jorge Luis` into `Jorge Luis Borges` is author-name
    // parsing — a derived fact the TUI needs the same answer for, so it belongs
    // in the engine (item 17) and must not appear here.
    expect(authorsLabel(['Borges, Jorge Luis'])).toBe('Borges, Jorge Luis');
  });

  it('has nothing to say about a book with no author', () => {
    expect(authorsLabel([])).toBeNull();
  });
});

describe('the series pair', () => {
  it('prints a whole index without a decimal point', () => {
    // `series_index` is a REAL, so a naive render gives `#2.0`. The engine
    // settled this in `series_index_text` and JS agrees for whole numbers —
    // asserted rather than assumed, because it is the reason the engine owns
    // `Book::series_label` at all.
    expect(seriesLabel('The Book of the New Sun', 2)).toBe('The Book of the New Sun #2');
  });

  it('keeps a fractional index, because novellas are 0.5 everywhere', () => {
    expect(seriesLabel('Dune', 1.5)).toBe('Dune #1.5');
  });

  it('names a series that gives no number', () => {
    expect(seriesLabel('An Unnumbered Sequence', null)).toBe('An Unnumbered Sequence');
  });

  it('says nothing when there is no series, whatever the index says', () => {
    // calibre writes `series_index = 1.0` on a book with no series at all — its
    // default, not a claim. The index is read only where the name is.
    expect(seriesLabel(null, 1)).toBeNull();
  });
});
