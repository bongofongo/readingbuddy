/**
 * The "Reading now" page's two rules, which are the axiom's largest live
 * exposure on the surface the app opens on: what the newest mark is, and what
 * order the open books come in.
 */
import { describe, expect, it } from 'vitest';

import type { HighlightDto, NoteDto } from '$lib/api/bindings';

import { latestMark, ordered, type Preview } from './latest';

function highlight(id: number, at: number, text = `passage ${id}`): HighlightDto {
  return { id, text, created_at: at } as HighlightDto;
}

function note(id: number, at: number | null, title = `note ${id}`): NoteDto {
  return { id, title, created_at: at } as NoteDto;
}

function preview(id: number, mark: Preview<number>['mark'], touched = 0): Preview<number> {
  return { reading: id, mark, touched };
}

describe('latestMark', () => {
  it('takes the newest of the two kinds and says which kind it is', () => {
    expect(latestMark([highlight(1, 100)], [note(1, 200)])).toEqual({
      kind: 'note',
      title: 'note 1',
      at: 200,
    });
    expect(latestMark([highlight(1, 300)], [note(1, 200)])).toEqual({
      kind: 'passage',
      text: 'passage 1',
      at: 300,
    });
  });

  it('is null for a book with neither, and the band draws nothing there', () => {
    // Not "nothing yet". An open book you have not written against is not an
    // omission, and `yet` is the word `src/lib/axiom.test.ts` bans by name.
    expect(latestMark([], [])).toBeNull();
  });

  it('skips a note with no date rather than sorting it as 1970', () => {
    // `created_at` is nullable on the wire. Treating an unknown date as ancient
    // and skipping it produce the same band today; being explicit is what stops
    // that changing the day somebody reverses the comparator.
    expect(latestMark([], [note(1, null)])).toBeNull();
    expect(latestMark([highlight(1, 5)], [note(2, null)])).toMatchObject({ kind: 'passage' });
  });
});

describe('ordered', () => {
  it('orders by the newest mark, so a stale reading falls off without being dismissed', () => {
    const band = ordered([
      preview(1, { kind: 'note', title: 'a', at: 10 }),
      preview(2, { kind: 'passage', text: 'b', at: 90 }),
      preview(3, { kind: 'passage', text: 'c', at: 50 }),
    ]);
    expect(band.map((p) => p.reading)).toEqual([2, 3, 1]);
  });

  it('keeps every open book, because this page is the only place they are', () => {
    // The cap of four went with the band. On a page whose whole subject is the
    // books you have open, a silent cut is not restraint — it is the entrance
    // declining to show you what you are reading, with the overflow no longer
    // one scroll below on the same surface.
    const many = Array.from({ length: 9 }, (_, i) =>
      preview(i, { kind: 'passage', text: 'x', at: i }),
    );
    expect(ordered(many)).toHaveLength(9);
  });

  it('puts a reading you have written against above one only a sync touched', () => {
    // The two timestamps are never mixed into one number: *you wrote something*
    // is a stronger statement about what you are reading than *a device moved a
    // page*, however recent the second is.
    const band = ordered([
      preview(1, null, 9_000),
      preview(2, { kind: 'passage', text: 'b', at: 1 }),
    ]);
    expect(band.map((p) => p.reading)).toEqual([2, 1]);
  });

  it('falls back to the engine’s own recency for readings with no marks', () => {
    const band = ordered([preview(1, null, 10), preview(2, null, 40), preview(3, null, 30)]);
    expect(band.map((p) => p.reading)).toEqual([2, 3, 1]);
  });
});
