import { describe, expect, it } from 'vitest';

import {
  authorsLabel,
  progressDetail,
  progressLabel,
  readingStateLabel,
  titleLabel,
} from './phrasing';

describe('reading state', () => {
  it('gives a book nobody has opened no label at all', () => {
    // Not "Unread". The commonest state in a real library, and labelling it
    // frames the shelf as a list of things not done — which is the framing
    // `docs/decisions.md` bans by name. Note this is `null` on the wire and not
    // a variant: the engine has no `NeverOpened`, so there is nothing here to
    // filter or count on.
    expect(readingStateLabel(null)).toBeNull();
  });

  it('never styles putting a book down as failure', () => {
    const label = readingStateLabel({ state: 'abandoned' });
    expect(label).toBe('Put down');
    expect(label?.toLowerCase()).not.toContain('fail');
    expect(label?.toLowerCase()).not.toContain('did not finish');
  });

  it('shows an unknown state verbatim rather than inventing a word', () => {
    // The state is typed on the wire and still open: an importer can write a
    // word this build does not model, and `other` is how it degrades. Guessing
    // at it would be worse than repeating it.
    expect(readingStateLabel({ state: 'other', raw: 'paused-by-some-other-app' })).toBe(
      'paused-by-some-other-app',
    );
  });
});

describe('titles and authors', () => {
  it('names an untitled book rather than rendering an empty line', () => {
    // A sidecar-seeded book with no `doc_props.title` really has none, and the
    // dev library holds one on purpose. The engine states the absence; the word
    // for it is ours.
    expect(titleLabel(null)).toBe('Untitled');
    expect(titleLabel('   ')).toBe('Untitled');
  });

  it('joins the names the engine already read the comma in', () => {
    // `authors_display` is the parse — `Borges, Jorge Luis` arrives already
    // flipped, because the TUI needs the same answer (item 17). What is left
    // here is the join, which is wording.
    expect(authorsLabel(['Jorge Luis Borges', 'Colette'])).toBe('Jorge Luis Borges, Colette');
  });

  it('has nothing to say about a book with no author', () => {
    expect(authorsLabel([])).toBeNull();
  });
});

describe('progress', () => {
  it('says the engine’s percentage and does not recompute one', () => {
    // The integer division is the engine's: `29/100` is `0.28999999999999998` in
    // binary, so `Math.floor(fraction * 100)` would say 28 where the engine says
    // 29. This asserts the label follows `percent` and not `fraction`.
    expect(
      progressLabel({
        progress: 'started',
        page: 29,
        of: 100,
        fraction: 0.29,
        percent: 29,
        source: 'pages',
      }),
    ).toBe('29%');
  });

  it('names the page when there is no honest denominator', () => {
    // `page_count = 0` is a real book in the dev library. The engine normalises
    // it to absence, so there is no percentage to say and no `p/0` to print.
    expect(
      progressLabel({
        progress: 'started',
        page: 12,
        of: null,
        fraction: null,
        percent: null,
        source: null,
      }),
    ).toBe('p. 12');
  });

  it('says nothing about a book with nothing recorded', () => {
    // A shelf full of "Not started" is a list of things you have not done.
    expect(progressLabel({ progress: 'untouched' })).toBeNull();
  });

  it('leaves a finished book to its state label rather than saying 100%', () => {
    expect(progressLabel({ progress: 'finished' })).toBeNull();
  });
});

describe('progress, at length', () => {
  it('names the page a reader recognises beside the percentage', () => {
    expect(
      progressDetail({
        progress: 'started',
        page: 500,
        of: 1408,
        fraction: 500 / 1408,
        percent: 35,
        source: 'pages',
      }),
    ).toBe('p. 500 of 1408 · 35%');
  });

  it('has no "of 0" to print, because the engine collapsed it', () => {
    expect(
      progressDetail({
        progress: 'started',
        page: 12,
        of: null,
        fraction: null,
        percent: null,
        source: null,
      }),
    ).toBe('p. 12');
  });

  it('says the device percentage alone when there is no page at all', () => {
    // The commonest row in a KOReader-sourced library.
    expect(
      progressDetail({
        progress: 'started',
        page: null,
        of: null,
        fraction: 0.43,
        percent: 43,
        source: 'device',
      }),
    ).toBe('43%');
  });
});
