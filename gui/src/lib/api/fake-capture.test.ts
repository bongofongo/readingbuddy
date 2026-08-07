/**
 * The two contracts the book view's new controls are built on (items 48, 49).
 *
 * Both are properties of the *reply*, not of the screen, and both are the kind
 * a fake breaks silently: a batch that dropped its empty rows still renders
 * marks, and a capture that answered `true` twice still says "kept". So they are
 * asserted here, against the fake, in the layer that has no browser — the same
 * arrangement `fake-notes` and `fake-rows` use.
 */
import { describe, expect, it } from 'vitest';

import { FakeClient } from './fake';

describe('the citation batch answers every id it was asked about', () => {
  it('gives one entry per id, in the order asked, empties and duplicates included', async () => {
    const client = new FakeClient();
    // Note 1 cites highlight 2, note 3 cites highlight 3, note 2 cites nothing,
    // and 4242 is not a note at all.
    const asked = [2, 4242, 1, 3, 3];
    const reply = await client.citationsForNotes(asked);

    expect(reply.map((c) => c.note_id)).toEqual(asked);
    // *No such note* and *cites nothing* are the same answer to this question,
    // so 4242 and note 2 come back alike. A dropped row would shift the
    // caller's zip against a page it already holds, which is the bug this
    // contract exists to make impossible — and the repeated `3` is the other
    // half of it, since a caller may legitimately ask twice.
    expect(reply.map((c) => c.highlight_ids)).toEqual([[], [], [2], [3], [3]]);
  });

  it('asks nothing and answers nothing for an empty list', async () => {
    expect(await new FakeClient().citationsForNotes([])).toEqual([]);
  });

  it('moves with a cite and with an uncite', async () => {
    const client = new FakeClient();
    await client.cite(2, 1);
    expect((await client.citationsForNotes([2])).map((c) => c.highlight_ids)).toEqual([[1]]);
    await client.uncite(2, 1);
    expect((await client.citationsForNotes([2])).map((c) => c.highlight_ids)).toEqual([[]]);
  });

  it('loses a deleted note its citations', async () => {
    const client = new FakeClient();
    await client.deleteNote(1);
    expect((await client.citationsForNotes([1])).map((c) => c.highlight_ids)).toEqual([[]]);
    // And takes nothing from the notes that were not deleted — the mark is a
    // union, so a citation lost from the wrong row is invisible on screen.
    expect((await client.citationsForNotes([3])).map((c) => c.highlight_ids)).toEqual([[3]]);
  });
});

describe('capturing a card', () => {
  it('is true once and false ever after, leaving the first card untouched', async () => {
    const client = new FakeClient();
    expect(
      await client.createFlashcard({ bookId: 3, highlightId: 1, word: 'place', context: 'first' }),
    ).toBe(true);
    // A second capture of the same word must not repoint the card at another
    // passage or overwrite its context — `ON CONFLICT DO NOTHING`, not
    // `DO UPDATE`. `false` is the whole reason the write answers anything.
    expect(
      await client.createFlashcard({ bookId: 3, highlightId: 3, word: 'place', context: 'second' }),
    ).toBe(false);

    const kept = (await client.listFlashcardsForBook(3)).filter((c) => c.word === 'place');
    expect(kept).toEqual([expect.objectContaining({ highlight_id: 1, context: 'first' })]);
  });

  it('trims before it dedupes, so a stray space is not a second word', async () => {
    const client = new FakeClient();
    expect(await client.createFlashcard({ bookId: 3, word: 'mot' })).toBe(true);
    expect(await client.createFlashcard({ bookId: 3, word: '  mot  ' })).toBe(false);
    expect((await client.listFlashcardsForBook(3)).filter((c) => c.word === 'mot')).toHaveLength(1);
  });

  it('refuses a word that is only whitespace', async () => {
    await expect(new FakeClient().createFlashcard({ bookId: 3, word: '   ' })).rejects.toThrow(
      /needs a word/,
    );
  });

  it('refuses a passage belonging to another book, and writes nothing', async () => {
    const client = new FakeClient();
    // Highlight 4 is book 11's. The frontend deliberately does not pre-validate
    // the pair — the point of a write taking ids is that the refusal lives
    // where the rows do — so the fake has to produce it or that path is dead.
    await expect(
      client.createFlashcard({ bookId: 3, highlightId: 4, word: 'elsewhere' }),
    ).rejects.toThrow(/belongs to book 11/);
    expect(await client.listFlashcardsForBook(3)).not.toContainEqual(
      expect.objectContaining({ word: 'elsewhere' }),
    );
  });

  it('refuses a passage that is not a passage', async () => {
    await expect(
      new FakeClient().createFlashcard({ bookId: 3, highlightId: 4242, word: 'ghost' }),
    ).rejects.toThrow(/no highlight/);
  });

  it('is instance state, so one page session accumulates its own cards', async () => {
    const client = new FakeClient();
    await client.createFlashcard({ bookId: 3, word: 'kept' });
    expect((await new FakeClient().listFlashcardsForBook(3)).map((c) => c.word)).not.toContain(
      'kept',
    );
  });
});

describe('the cards a book already has', () => {
  it('carries both an anchored card and one anchored to nothing', async () => {
    // The unanchored shape is every card minted before item 45 selected the
    // column, and the band must show it against no passage rather than guess.
    const cards = await new FakeClient().listFlashcardsForBook(20);
    expect(cards.filter((c) => c.highlight_id === 13).map((c) => c.word)).toEqual([
      'argument',
      'intends',
    ]);
    expect(cards.filter((c) => c.highlight_id === null).map((c) => c.word)).toEqual(['shelving']);
  });

  it('returns exported cards beside pending ones', async () => {
    // `list_flashcards_for_book` does not filter on `exported`, unlike
    // `list_flashcards`. A fake that hid them would make the band's silence
    // about that flag look like a decision it never had to make.
    const cards = await new FakeClient().listFlashcardsForBook(20);
    expect(cards.some((c) => c.exported)).toBe(true);
  });
});
