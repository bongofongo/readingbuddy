/**
 * The chain and the reading life, against the fake (item 28).
 *
 * These are the assertions that would otherwise be made by looking at a screen
 * once: a moment is drawn one time and never again, and an absence rendered as a
 * zero looks like an ordinary quiet month.
 */
import { describe, expect, it } from 'vitest';

import { FakeClient } from './fake';

describe('the moment', () => {
  it('hands back one at a time, newest first', async () => {
    const c = new FakeClient();
    const [m] = await c.pendingMoments(1);
    expect(m?.kind.kind).toBe('reading_closed');
    // The kind the chain is drawn around is the one that carries a reading:
    // a card is minted per read, and a moment identified by its book alone
    // cannot select between a reread's two.
    expect(m?.book_id).toBe(12);
    expect(m?.reading_id).toBe(1);
  });

  it('does not offer one that has been surfaced', async () => {
    const c = new FakeClient();
    const [first] = await c.pendingMoments(1);
    await c.acknowledgeMoment(first!.id);
    const [next] = await c.pendingMoments(1);
    expect(next?.id).not.toBe(first!.id);
  });

  it('is idempotent, so a double render costs nothing', async () => {
    const c = new FakeClient();
    const [m] = await c.pendingMoments(1);
    await c.acknowledgeMoment(m!.id);
    await expect(c.acknowledgeMoment(m!.id)).resolves.toBeUndefined();
    // And an id for a moment nobody has seen is inert rather than an error —
    // `run_ended` depends on the clock, so refusing an acknowledgement for a
    // moment that stopped being derivable would replay it for ever.
    await expect(c.acknowledgeMoment('reading_closed:99999')).resolves.toBeUndefined();
  });

  it('runs out rather than repeating itself', async () => {
    const c = new FakeClient();
    for (const m of await c.pendingMoments(50)) await c.acknowledgeMoment(m.id);
    expect(await c.pendingMoments(1)).toEqual([]);
  });
});

describe('the card', () => {
  it('gives each read of one book its own passage', async () => {
    const c = new FakeClient();
    const first = await c.cardPassage(1);
    const second = await c.cardPassage(2);
    expect(first?.text).not.toBe(second?.text);
    // Scoped to the reading, never the book. Selecting over `book_id` would
    // hand both cards the same sentence and the side-by-side comparison the
    // card exists for would show two identical passages.
    expect(first?.reading_id).toBe(1);
    expect(second?.reading_id).toBe(2);
  });

  it('is not the first highlight of the read', async () => {
    // `highlights[0]` is the rule a frontend invents. The engine's is longest,
    // ties to the lowest id (item 44), and the fixture states its answer — so a
    // component reaching for `[0]` renders visibly different text.
    const c = new FakeClient();
    const marks = await c.highlightsForReading(1);
    const chosen = await c.cardPassage(1);
    expect(marks[0]?.id).not.toBe(chosen?.id);
    expect(
      marks.some((m) => m.id === chosen?.id),
      "the chosen passage is one of that read's own marks",
    ).toBe(true);
  });

  it('does not reach a mark that belongs to no read', async () => {
    const c = new FakeClient();
    const all = await c.listHighlights(12);
    const orphan = all.find((h) => h.reading_id === null);
    expect(orphan, 'the fixture needs an unattributed mark').toBeDefined();
    for (const readingId of [1, 2]) {
      expect((await c.cardPassage(readingId))?.id).not.toBe(orphan!.id);
      expect((await c.highlightsForReading(readingId)).map((h) => h.id)).not.toContain(orphan!.id);
    }
  });

  it('has no passage when the read has no attributed mark', async () => {
    // Book 11's one highlight is unattributed. `null` is an ordinary answer,
    // the same absence `highlightsForReading` reports as `[]`, and a card draws
    // it as an absence rather than as an error.
    const c = new FakeClient();
    const [reading] = await c.listReadings(11);
    expect(await c.cardPassage(reading!.id)).toBeNull();
    expect(await c.highlightsForReading(reading!.id)).toEqual([]);
  });

  it('scopes notes to the read, with no fall-back to the book', async () => {
    // `NoteScope::Reading` is literally `WHERE reading_id = ?`. A note created
    // without one appears on no card, which is a real property of a real vault.
    const c = new FakeClient();
    const ofRead = await c.notesForReading(1);
    expect(ofRead.map((n) => n.reading_id)).toEqual([1, 1]);
    const ofBook = await c.listNotes(12);
    expect(ofBook.length).toBeGreaterThan(ofRead.length);
  });

  it('carries a different rating for each read, which is what makes it a comparison', async () => {
    const c = new FakeClient();
    const first = await c.noteForReading(1, 'review');
    const second = await c.noteForReading(2, 'review');
    expect((await c.reviewRating(first!.id))?.value).toBe(3);
    expect((await c.reviewRating(second!.id))?.value).toBe(4.5);
  });

  it('opens the reflection of the read it was asked about', async () => {
    // A moment about a closed read must not open the *current* read's note.
    const c = new FakeClient();
    const opened = await c.openReflection(12, 1);
    const note = await c.getNote(opened.id);
    expect(note?.reading_id).toBe(1);
  });
});

describe('the reading life', () => {
  it('answers a year with that year and the whole span with everything', async () => {
    const c = new FakeClient();
    const all = await c.activitySummary('1970-01-01', '2025-05-14');
    const y = await c.activitySummary('2024-01-01', '2024-12-31');
    expect(all.books_finished).toBe(7);
    expect(y.books_finished).toBe(3);
    // The range is echoed back, so a client reading it gets what it asked.
    expect(y.range).toEqual({ from: '2024-01-01', to: '2024-12-31' });
  });

  it('reports only months carrying an event, oldest first', async () => {
    const c = new FakeClient();
    const ms = await c.activityByMonth('1970-01-01', '2025-05-14');
    expect(ms.map((m) => m.month)).toEqual([
      '2024-11',
      '2024-12',
      '2025-01',
      '2025-02',
      '2025-03',
      '2025-04',
      '2025-05',
    ]);
  });

  it('reports a month at the edge of a range rather than widening to it', async () => {
    const c = new FakeClient();
    const ms = await c.activityByMonth('2025-02-10', '2025-03-05');
    expect(ms.map((m) => m.month)).toEqual(['2025-02', '2025-03']);
  });

  /**
   * The three shapes of *nothing measured*, which is the whole point of the page.
   */
  it('keeps a measured zero apart from an absence', async () => {
    const c = new FakeClient();
    const ms = await c.activityByMonth('1970-01-01', '2025-05-14');
    const feb = ms.find((m) => m.month === '2025-02');
    // Item 31: a measured twenty-second session records `Some(0)`, not `None`.
    // The device *is* saying something and it must not become "not measured".
    expect(feb?.minutes).toBe(0);
    const nov = ms.find((m) => m.month === '2024-11');
    expect(nov?.minutes).toBeNull();
    expect(nov?.activity_days).toBeGreaterThan(0);
  });

  it('lets minutes be absent while pages are present', async () => {
    const c = new FakeClient();
    const ms = await c.activityByMonth('1970-01-01', '2025-05-14');
    const apr = ms.find((m) => m.month === '2025-04');
    expect(apr?.minutes).toBeNull();
    expect(apr?.pages).toBe(120);
  });
});

/**
 * A reader with no device data at all — a required case, not an edge case.
 *
 * A library built from a Goodreads CSV, from calibre, or from bare epub imports
 * has no `statistics.sqlite3` behind it and therefore no minutes and no pages
 * anywhere, at any grain, for ever. That reader still has a reading life: item
 * 21's three device-free fillers record highlight days, vault days and reading
 * endpoints, so the counts this app originates are all there.
 *
 * This is the case the absence rendering exists for, and it is the one where
 * folding days into months in TypeScript would produce a whole calendar reading
 * *you read for zero minutes*.
 */
describe('a reader with no device at all', () => {
  const c = new FakeClient({ device: false });

  it('has minutes and pages absent everywhere, at every grain', async () => {
    const s = await c.activitySummary('1970-01-01', '2025-05-14');
    expect(s.minutes).toBeNull();
    expect(s.pages).toBeNull();
    for (const m of await c.activityByMonth('1970-01-01', '2025-05-14')) {
      expect(m.minutes, `${m.month} minutes`).toBeNull();
      expect(m.pages, `${m.month} pages`).toBeNull();
    }
  });

  it('still has a reading life recorded without a device', async () => {
    const s = await c.activitySummary('1970-01-01', '2025-05-14');
    expect(s.activity_days).toBeGreaterThan(0);
    expect(s.books_finished).toBeGreaterThan(0);
    expect(s.notes_created).toBeGreaterThan(0);
    const ms = await c.activityByMonth('1970-01-01', '2025-05-14');
    expect(ms.length).toBeGreaterThan(0);
    expect(ms.every((m) => m.activity_days > 0)).toBe(true);
  });
});
