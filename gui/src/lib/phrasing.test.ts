import { describe, expect, it } from 'vitest';

import type { NoteDto, ReadingDto } from './api/bindings';
import {
  authorsLabel,
  countLabel,
  dayLabel,
  deviceFigures,
  fieldLabel,
  fileSizeLabel,
  minutesLabel,
  monthLabel,
  NO_DEVICE_DATA,
  NOT_MEASURED,
  noteAnchorLabel,
  pagesLabel,
  noteKindLabel,
  progressDetail,
  progressLabel,
  ratingLabel,
  readingSpan,
  readingStateLabel,
  sourceLabel,
  titleLabel,
  trimNumber,
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

// ---------------------------------------------------------------------------
// Item 27's words.
// ---------------------------------------------------------------------------

function note(over: Partial<NoteDto>): NoteDto {
  return {
    id: 1,
    book_id: 3,
    reading_id: null,
    highlight_id: null,
    page: null,
    location: null,
    file_path: '1.md',
    title: 'A note',
    kind: 'note',
    created_at: 1735689600,
    ...over,
  };
}

function reading(over: Partial<ReadingDto>): ReadingDto {
  return {
    id: 1,
    book_id: 3,
    started_at: null,
    finished_at: null,
    status: { state: 'reading' },
    source: 'koreader',
    current_page: null,
    ko_status: null,
    ko_percent: null,
    ko_rating: null,
    created_at: 1735689600,
    last_modified: 1735689600,
    progress: { progress: 'untouched' },
    ...over,
  };
}

describe('days', () => {
  it('says a day in UTC, which is the day convention the engine already uses', () => {
    // Not `Intl`: a locale-dependent rendering makes the committed screenshots
    // depend on the machine that took them.
    expect(dayLabel(1735689600)).toBe('2025-01-01');
  });

  it('has no word for an absent date', () => {
    expect(dayLabel(null)).toBeNull();
  });

  it('says nothing rather than "Invalid Date" for a value it cannot read', () => {
    expect(dayLabel(Number.NaN)).toBeNull();
  });
});

describe('a reading, worded', () => {
  it('gives an open read a start and no dash toward a date it is waiting for', () => {
    // `finished_at: null` means **open**, not unknown. A range with a blank
    // right-hand side would read as a gap in the record.
    expect(readingSpan(reading({ started_at: 1735689600 }))).toBe('from 2025-01-01');
  });

  it('gives a closed read both ends', () => {
    expect(readingSpan(reading({ started_at: 1735689600, finished_at: 1738368000 }))).toBe(
      '2025-01-01 – 2025-02-01',
    );
  });

  it('says nothing at all about a reading with no dates', () => {
    expect(readingSpan(reading({}))).toBeNull();
  });
});

describe('notes', () => {
  it('labels the kinds that are singular and leaves an ordinary note bare', () => {
    // The four kinds share one list, so a row has to say which it is — except
    // the ordinary one, where a label would be a column of the same word.
    expect(noteKindLabel('note')).toBeNull();
    expect(noteKindLabel('reflection')).toBe('Reflection');
    expect(noteKindLabel('review')).toBe('Review');
  });

  it('shows a kind this build does not know verbatim', () => {
    // `notes.kind` is a `String` on the wire and stays one. The same call
    // `readingStateLabel` makes about another application's word.
    expect(noteKindLabel('marginalia')).toBe('marginalia');
  });

  it('anchors a note to its page, its location, or the passage it hangs off', () => {
    expect(noteAnchorLabel(note({ page: 212 }))).toBe('p. 212');
    expect(noteAnchorLabel(note({ location: 'ch4/p3' }))).toBe('ch4/p3');
    expect(noteAnchorLabel(note({ page: 212, location: 'ch4/p3' }))).toBe('p. 212 · ch4/p3');
  });

  it('says a passage anchor only where there is no page or location instead', () => {
    // The TUI's rule, ported: an arrow rather than nothing, so a note hung off a
    // highlight with no page does not look unanchored.
    expect(noteAnchorLabel(note({ highlight_id: 7 }))).toBe('↳ passage');
    expect(noteAnchorLabel(note({ page: 5, highlight_id: 7 }))).toBe('p. 5');
  });

  it('has no anchor to word for a note about the book as a whole', () => {
    expect(noteAnchorLabel(note({}))).toBeNull();
  });
});

describe('ratings', () => {
  it('says the value against the scale it was recorded on', () => {
    // A bare number is not re-derivable into anything: the Goodreads map is
    // user-editable, which is why the scale travels with the value.
    expect(
      ratingLabel({ scale: { id: 1, name: 'stars', min: 1, max: 5, step: 0.5 }, value: 4.5 }),
    ).toBe('4.5 / 5');
  });

  it('never prints a trailing zero', () => {
    expect(trimNumber(4)).toBe('4');
    expect(trimNumber(4.5)).toBe('4.5');
    expect(trimNumber(3.5000000000000004)).toBe('3.5');
  });
});

describe('files and provenance', () => {
  it('says a size the way a file browser would', () => {
    expect(fileSizeLabel(512)).toBe('512 B');
    expect(fileSizeLabel(4 * 1024 * 1024)).toBe('4.0 MB');
    expect(fileSizeLabel(40 * 1024 * 1024)).toBe('40 MB');
  });

  it('names the origins in the words a person uses for them', () => {
    expect(sourceLabel('open_library')).toBe('Open Library');
    // The rank that outranks every provider, on a screen you own.
    expect(sourceLabel('user')).toBe('You');
  });

  it('shows a source token this build does not know as it was stored', () => {
    // The column's vocabulary lives in a comment rather than a `CHECK`, so an
    // unrecognised token is a row a newer engine wrote — not an error.
    expect(sourceLabel('kobo')).toBe('kobo');
  });

  it('says a column name out loud', () => {
    expect(fieldLabel('publish_year')).toBe('Publish year');
  });
});

/**
 * The reading life's words, and the one distinction the page is built on.
 */
describe('what was measured, and what was not', () => {
  it('says an absence rather than a zero', () => {
    // The single most important line on the reading-life page. `minutes` is
    // `Option` at every level of item 21's log, and item 42 exists because
    // folding days into months in a client collapses that `null` to `0` —
    // telling a reader they read for no time at all in a month they read
    // off-device.
    expect(minutesLabel(null)).toBe(NOT_MEASURED);
    expect(pagesLabel(null)).toBe(NOT_MEASURED);
  });

  it('prints a measured zero as a zero', () => {
    // Item 31: a measured twenty-second session records `Some(0)`, not `None`.
    // The device is saying something, and collapsing it into the absence throws
    // away the distinction the column is nullable to keep.
    expect(minutesLabel(0)).toBe('0 min');
    expect(pagesLabel(0)).toBe('0 pages');
    expect(minutesLabel(0)).not.toBe(NOT_MEASURED);
  });

  it('says hours once there are hours', () => {
    expect(minutesLabel(45)).toBe('45 min');
    expect(minutesLabel(60)).toBe('1 h');
    expect(minutesLabel(620)).toBe('10 h 20 min');
  });

  it('collapses two absences into one sentence and names a single one', () => {
    // Two chips reading "not measured" side by side say the same thing twice
    // and neither says which is which. One absence is named, because minutes
    // and pages are independent `Option`s — the fixture has a month with pages
    // and no minutes.
    expect(deviceFigures(null, null)).toEqual([NO_DEVICE_DATA]);
    expect(deviceFigures(null, 120)).toEqual([`minutes ${NOT_MEASURED}`, '120 pages']);
    expect(deviceFigures(900, null)).toEqual(['15 h', `pages ${NOT_MEASURED}`]);
    expect(deviceFigures(60, 40)).toEqual(['1 h', '40 pages']);
  });

  it('never says nothing at all', () => {
    // A period that measured nothing still gets a sentence. That is the
    // difference between rendering an absence and rendering nothing.
    for (const pair of [
      [null, null],
      [0, 0],
      [null, 0],
    ] as const) {
      expect(deviceFigures(pair[0], pair[1]).length).toBeGreaterThan(0);
    }
  });

  it('agrees a count with its noun', () => {
    expect(countLabel(1, 'book')).toBe('1 book');
    // Zero is a legitimate answer for a count the engine originates — a zero in
    // `books_finished` is knowable, which is what separates it from the
    // nullable measurements above.
    expect(countLabel(0, 'book')).toBe('0 books');
    expect(countLabel(4, 'day')).toBe('4 days');
    expect(countLabel(1, 'passage')).toBe('1 passage');
  });

  it('spells a month from a fixed table rather than from the locale', () => {
    // A locale-dependent rendering makes the committed screenshots depend on
    // the machine that took them — `dayLabel`'s own rule, applied again.
    expect(monthLabel('2025-03')).toBe('March 2025');
    expect(monthLabel('2024-12')).toBe('December 2024');
  });

  it('shows a month it cannot parse verbatim', () => {
    // It came off `substr(day, 1, 7)` over a zero-padded ISO date, so it cannot
    // be wrong; inventing a correction here would hide a case that could be.
    expect(monthLabel('nonsense')).toBe('nonsense');
    expect(monthLabel('2025-13')).toBe('2025-13');
  });
});
