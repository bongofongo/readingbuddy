/**
 * The moment's sentence, and the two rules it exists to make assertable.
 *
 * A moment is drawn once and then never again, which makes it the hardest thing
 * in this app to review by looking at it — so the rules go in a pure function
 * and the function gets a suite.
 */
import { describe, expect, it } from 'vitest';

import type { MomentDto, MomentKindDto } from '$lib/api/bindings';
import { momentSentence } from './sentence';

const TITLES: Record<number, string> = { 3: 'The Doorstop', 12: 'A Book I Went Back To' };
const titleOf = (id: number) => TITLES[id] ?? null;

function moment(kind: MomentKindDto, over: Partial<MomentDto> = {}): MomentDto {
  return {
    id: 'x:1',
    kind,
    book_id: 12,
    reading_id: 1,
    day: '2025-01-31',
    occurred_at: 1738368000,
    ...over,
  };
}

describe('a moment ends in a move', () => {
  it('a closed reading invites the reflection, on that read', () => {
    const s = momentSentence(moment({ kind: 'reading_closed' }), titleOf);
    expect(s.said).toBe('You finished A Book I Went Back To.');
    // The **reading**, relayed from the DTO. A reread has two, and dropping it
    // would put the reflection on whichever read `open_anchored` guesses.
    expect(s.move).toEqual({
      move: 'reflect',
      bookId: 12,
      readingId: 1,
      invitation: 'Write what you thought',
    });
  });

  it('carries a null reading through rather than inventing one', () => {
    // `first_annotation` may have no reading — the evidence does not always
    // settle on one — and `null` on the wire must reach the wire again as null.
    const s = momentSentence(
      moment({ kind: 'first_annotation' }, { book_id: 3, reading_id: null }),
      titleOf,
    );
    expect(s.said).toBe('You marked your first passage in The Doorstop.');
    expect(s.move).toMatchObject({ move: 'reflect', bookId: 3, readingId: null });
  });

  it('a reflection that reached another book names both, and opens what you wrote', () => {
    const s = momentSentence(
      moment(
        { kind: 'reflection_reached', note_id: 2, reached_book_id: 12 },
        { book_id: 3, reading_id: null },
      ),
      titleOf,
    );
    expect(s.said).toBe('Your reflection on The Doorstop reached A Book I Went Back To.');
    expect(s.move).toMatchObject({ move: 'note', bookId: 3, noteId: 2 });
  });

  it('every kind goes somewhere, including one this build does not know', () => {
    // ts-rs drops `#[serde(other)]`, so a kind from a newer engine parses and
    // reaches here. It must still end in a move: nothing is a dead end.
    const unknown = { kind: 'something_new' } as unknown as MomentKindDto;
    const s = momentSentence(moment(unknown), titleOf);
    expect(s.said).not.toBe('');
    expect(s.move.move).toBe('life');
  });

  it('says "a book" for a title it has not loaded, rather than a gap', () => {
    const s = momentSentence(moment({ kind: 'reading_closed' }, { book_id: 999 }), titleOf);
    expect(s.said).toBe('You finished a book.');
  });
});

describe('a run is spoken as its span and never as a count', () => {
  const run = moment(
    { kind: 'run_ended', from: '2025-01-05', to: '2025-01-08', days: 4 },
    { book_id: null, reading_id: null },
  );

  it('names both ends of a span that is over', () => {
    expect(momentSentence(run, titleOf).said).toBe(
      'You read every day from 2025-01-05 to 2025-01-08.',
    );
  });

  /**
   * The argued line, asserted so a later thread has to argue with a red test
   * rather than with a comment.
   *
   * `days` is on the DTO and entry 23 defends it — a run is recognised only
   * after it is over, and a count of your own past days is the kind item 17
   * permits. But this sentence surfaces on the **home surface**, where the rule
   * sharpened this session allows a number that describes *one book* and nothing
   * else. A run of days describes a habit, and a habit with a number on it is a
   * streak one product decision later.
   *
   * The span carries the same fact and has nothing in it to beat.
   */
  it('does not put the number of days in the sentence', () => {
    const said = momentSentence(run, titleOf).said;
    expect(said, 'a run rendered as a count is a streak in a costume').not.toMatch(/\b4\b/);
    expect(said).not.toMatch(/\bday(s)? in a row\b/i);
    expect(said).not.toMatch(/\bstreak\b/i);
  });

  it('ends on the reading-life page, which is where a count is allowed', () => {
    expect(momentSentence(run, titleOf).move).toEqual({
      move: 'life',
      invitation: 'Your reading life',
    });
  });
});
