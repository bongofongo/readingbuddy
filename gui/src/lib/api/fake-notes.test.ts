/**
 * The half of the fake the book view runs on — notes, links, citations, the
 * reader's own annotation.
 *
 * Separate from `fake.test.ts`, which is item 38's declaration check against
 * `crates/corpus/edge-cases.json` and is about the *book* set. This is about
 * the things hanging off those books, and about the one invariant a fixture can
 * violate silently: the graph has two directions and they have to agree.
 */
import { describe, expect, it } from 'vitest';

import { FakeClient } from './fake';

describe('the graph, read in both directions', () => {
  it('gives a resolved edge to the note that wrote it and to the note it names', () => {
    // Declared once, as an edge, and read as `outgoingLinks` from one end and
    // `backlinks` from the other. Stating the two separately is how a fixture
    // comes to claim an edge one of its notes denies writing — and the engine
    // cannot be in that state, because back-resolution keeps `to_note` complete.
    const c = new FakeClient();
    return Promise.all([c.outgoingLinks(1), c.backlinks(2)]).then(([out, back]) => {
      expect(out.map((l) => l.target_title)).toContain('Reflection: The Doorstop');
      expect(out.find((l) => l.target_title === 'Reflection: The Doorstop')?.note?.id).toBe(2);
      expect(back.map((n) => n.id)).toContain(1);
    });
  });

  it('keeps a wikilink to a note nobody has written, as text', async () => {
    // A forward reference, not an error. It is the vault's best trick and a
    // fixture without one would let the pane drop it and stay green.
    const out = await new FakeClient().outgoingLinks(1);
    const pending = out.find((l) => l.note === null);
    expect(pending?.target_title).toBe('The Long Eighteenth Century');
  });

  it('rewrites the edges when the body changes, not just the text', async () => {
    // `update_note_body` reindexes the FTS row **and** the wikilink edges. A
    // fake that only stored the text would make an edit adding a `[[link]]`
    // look like it did nothing, which is the one thing the links pane is for.
    const c = new FakeClient();
    await c.updateNoteBody(3, 'Now it points at [[On The Doorstop]].');
    const out = await c.outgoingLinks(3);
    expect(out).toHaveLength(1);
    expect(out[0]?.note?.id).toBe(1);
    expect((await c.backlinks(1)).map((n) => n.id)).toContain(3);
  });

  it('drops a note out of the graph when it is deleted', async () => {
    const c = new FakeClient();
    await c.deleteNote(1);
    expect(await c.outgoingLinks(1)).toEqual([]);
    expect((await c.backlinks(2)).map((n) => n.id)).not.toContain(1);
  });
});

describe('citations', () => {
  it('starts from what the fixture says is cited', async () => {
    const cited = await new FakeClient().citationsFor(1);
    expect(cited.map((h) => h.id)).toEqual([2]);
  });

  it('cites and uncites by reference', async () => {
    const c = new FakeClient();
    await c.cite(1, 3);
    expect((await c.citationsFor(1)).map((h) => h.id)).toEqual([2, 3]);
    expect(await c.uncite(1, 3)).toBe(true);
    expect((await c.citationsFor(1)).map((h) => h.id)).toEqual([2]);
  });

  it('reports an uncite of something never cited rather than pretending', async () => {
    expect(await new FakeClient().uncite(1, 99)).toBe(false);
  });

  it('cites idempotently, so a double click is not two citations', async () => {
    const c = new FakeClient();
    await c.cite(1, 3);
    await c.cite(1, 3);
    expect((await c.citationsFor(1)).map((h) => h.id)).toEqual([2, 3]);
  });
});

describe('the two notes on a passage', () => {
  it('carries the device note and the reader note at once', async () => {
    // The case the ownership seam exists for, and the one nothing had ever
    // rendered: `ko_note` is theirs and is rewritten on every import,
    // `annotation` is ours and no import touches it.
    const h = (await new FakeClient().listHighlights(3)).find((x) => x.id === 1);
    expect(h?.ko_note).toBeTruthy();
    expect(h?.annotation).toBeTruthy();
  });

  it('shows an edited annotation on the next read', async () => {
    const c = new FakeClient();
    await c.setAnnotation(2, 'Mine.');
    expect((await c.listHighlights(3)).find((h) => h.id === 2)?.annotation).toBe('Mine.');
  });

  it('clears with null rather than with an empty string', async () => {
    // An empty annotation is *no annotation*, not one that is blank — `null` is
    // what clears the column, and a row claiming the reader wrote nothing is a
    // different thing from a row where they did not write.
    const c = new FakeClient();
    await c.setAnnotation(1, null);
    expect((await c.listHighlights(3)).find((h) => h.id === 1)?.annotation).toBeNull();
  });

  it('never touches the half the device owns', async () => {
    const c = new FakeClient();
    await c.setAnnotation(1, 'Mine.');
    expect((await c.listHighlights(3)).find((h) => h.id === 1)?.ko_note).toBe(
      'What did they mean by this?',
    );
  });
});

describe('writing a note', () => {
  it('takes the first few words as a title when none is given', async () => {
    // The engine's `derive_title`, which is what the composer's placeholder
    // promises. A fake that ignored it would let the promise rot unnoticed.
    const c = new FakeClient();
    const made = await c.createNote({
      book_id: 3,
      reading_id: null,
      highlight_id: null,
      page: null,
      location: null,
      kind: 'note',
      title: null,
      body: 'The argument only starts on page four hundred, which is late.',
    });
    expect(made.title).toBe('The argument only starts on page');
  });

  it('titles an empty body rather than leaving a blank row', async () => {
    const c = new FakeClient();
    const made = await c.createNote({
      book_id: 3,
      reading_id: null,
      highlight_id: null,
      page: null,
      location: null,
      kind: 'note',
      title: null,
      body: '',
    });
    expect(made.title).toBe('Untitled');
  });

  it('puts the new note in the book it was written against', async () => {
    const c = new FakeClient();
    const before = (await c.listNotes(3)).length;
    await c.createNote({
      book_id: 3,
      reading_id: null,
      highlight_id: null,
      page: null,
      location: null,
      kind: 'note',
      title: 'A new one',
      body: 'x',
    });
    expect((await c.listNotes(3)).length).toBe(before + 1);
  });
});

describe('the reflection and the review', () => {
  it('opens the existing one rather than minting a second', async () => {
    // `open_anchored` is open-**or**-mint, one call. A frontend that created
    // unconditionally would give a book two reflections, and the reflection is
    // the graph hub.
    const c = new FakeClient();
    const a = await c.openReflection(3);
    const b = await c.openReflection(3);
    expect(a.id).toBe(2);
    expect(b.id).toBe(a.id);
  });

  it('mints one for a book that has none', async () => {
    const c = new FakeClient();
    const made = await c.openReview(3);
    expect(made.id).toBeGreaterThan(0);
    expect((await c.getNote(made.id))?.kind).toBe('review');
  });
});

describe('a rating belongs to a review', () => {
  it('carries the scale beside the value', async () => {
    const r = await new FakeClient().reviewRating(4);
    expect(r?.value).toBe(4.5);
    expect(r?.scale.max).toBe(5);
  });

  it('has none for a note that is not a review', async () => {
    expect(await new FakeClient().reviewRating(1)).toBeNull();
  });

  it('sets and clears, and says whether there was one to clear', async () => {
    const c = new FakeClient();
    await c.setRating(4, 3);
    expect((await c.reviewRating(4))?.value).toBe(3);
    expect(await c.clearReviewRating(4)).toBe(true);
    expect(await c.reviewRating(4)).toBeNull();
    expect(await c.clearReviewRating(4)).toBe(false);
  });
});

describe('a reading carries its own progress', () => {
  it('does not put the current page under a read that already closed', async () => {
    // `BookDto.progress` is the *current* read's. On a reread, showing it under
    // an older read's row is exactly what `Progress::of_book` warns about — so
    // the closed read here is `finished` and the open one is halfway.
    const rs = await new FakeClient().listReadings(12);
    expect(rs.map((r) => r.progress.progress)).toEqual(['finished', 'started']);
  });

  it('gives a book with one read the same progress the book reports', async () => {
    // One read is the one case where the two genuinely are the same value.
    const c = new FakeClient();
    const book = await c.getBook(3);
    const rs = await c.listReadings(3);
    expect(rs).toHaveLength(1);
    expect(rs[0]?.progress).toEqual(book?.progress);
  });
});

describe('one client is one library', () => {
  it('does not leak an edit into the next one', async () => {
    // The books are module state and read-only; everything item 27 writes to is
    // per-instance. Otherwise one test's annotation is another test's fixture.
    const a = new FakeClient();
    await a.setAnnotation(2, 'a');
    await a.deleteNote(1);
    const b = new FakeClient();
    expect((await b.listHighlights(3)).find((h) => h.id === 2)?.annotation).toBeNull();
    expect((await b.listNotes(3)).map((n) => n.id)).toContain(1);
  });
});
