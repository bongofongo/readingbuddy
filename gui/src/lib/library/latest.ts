/**
 * The newest thing you wrote against a book you have open — and the order the
 * "Reading now" band is in.
 *
 * ## Why the band is ordered by this, and capped
 *
 * A "continue reading" shelf is populated by **starting** and drained only by
 * **finishing**. People start far more than they finish, so its steady state is
 * a queue of things abandoned — and the observable evidence that this is real is
 * that every major implementation of the pattern eventually shipped a manual
 * "remove from this row", which is a confession that the automatic population
 * rule was wrong.
 *
 * An uncapped band is this app's largest live exposure to the framing
 * `docs/decisions.md` bans. No number appears anywhere on it; the arithmetic is
 * done by the reader, on the highest-salience region of the home surface, every
 * time they open the app.
 *
 * Two rules answer it, and they are here rather than in the component because
 * they are the substance rather than the layout:
 *
 * 1. **Ordered by the newest mark, not by when the reading began.** Stale
 *    readings fall off the visible end *without any act of dismissal*, so
 *    nothing ever asks the reader to decide, on the home screen, whether they
 *    are going to finish something. It is also simply the most useful order: the
 *    book you touched yesterday is the book you are reading.
 * 2. **Four, and the cut is silent.** No "and 3 others" — that would be a count
 *    of what is left, and it would be the only such count in the app. The
 *    overflow is on the wall immediately below, in the group that says *Still
 *    reading*. Nothing is hidden; it is just not promoted.
 *
 * The cap costs the fourth-most-recent book its preview, which is a real loss
 * and is worth knowing as a choice rather than discovering as a bug.
 *
 * ## The call this is fed from is an N+1, and that is recorded rather than hidden
 *
 * There is no request for "the latest mark for a book". Building this needs
 * `listHighlights(id)` **plus** `listNotes(id)` per open reading — every
 * highlight in the book fetched to render one line, times however many readings
 * are open. It works and it is wrong: it wants a request, or a field on
 * `OpenReadingDto`. A client-side aggregate that hid the shape would take it off
 * the next audit's list, so the shape is left visible and written down here.
 */
import type { HighlightDto, NoteDto } from '$lib/api/bindings';

/**
 * The reader's own material, whichever kind of it is newest.
 *
 * A passage is quoted; a note is named. The two are drawn differently and the
 * band labels which it is, because "the last thing you kept" and "the last thing
 * you wrote" are different events and a reader can tell at a glance which one
 * they are looking at only if the app says.
 */
export type Mark =
  | { kind: 'passage'; text: string; at: number }
  | { kind: 'note'; title: string; at: number };

/**
 * The newest of a book's highlights and notes, or `null` when it has neither.
 *
 * `null` is an ordinary answer and the band draws **nothing** in its place — not
 * "nothing yet", which would turn an open book into an omission.
 *
 * A note with no `created_at` is skipped rather than sorted as epoch zero: the
 * column is nullable on the wire, and a note that would sort to 1970 can only
 * ever lose, so treating it as ancient and treating it as unknown produce the
 * same band. Being explicit costs one line and stops the day someone reverses
 * the comparator.
 */
export function latestMark(highlights: HighlightDto[], notes: NoteDto[]): Mark | null {
  let best: Mark | null = null;
  const keep = (m: Mark) => {
    if (best === null || m.at > best.at) best = m;
  };
  for (const h of highlights) keep({ kind: 'passage', text: h.text, at: h.created_at });
  for (const n of notes) {
    if (n.created_at === null) continue;
    keep({ kind: 'note', title: n.title, at: n.created_at });
  }
  return best;
}

/** One open reading, with the newest thing written against it. */
export type Preview<T> = {
  reading: T;
  mark: Mark | null;
  /** The engine's own recency for a reading nothing has been written against. */
  touched: number;
};

/** How many previews the band promotes. See the header — the cut is silent. */
export const PROMOTED = 4;

/**
 * The band's order: newest mark first, then the readings nothing is written
 * against, newest-touched first.
 *
 * The fallback is `Reading::last_modified`, which is the engine's record of when
 * the reading itself last changed — a page turn imported from a device is a real
 * event and is the only recency signal a book with no marks has. It is never
 * mixed with the mark timestamps into one number: a reading with a mark always
 * sorts above one without, because *you wrote something* is a stronger statement
 * about what you are reading than *a sync moved a page number*.
 */
export function promoted<T>(previews: Preview<T>[]): Preview<T>[] {
  const marked = previews.filter((p) => p.mark !== null);
  const bare = previews.filter((p) => p.mark === null);
  marked.sort((a, b) => b.mark!.at - a.mark!.at);
  bare.sort((a, b) => b.touched - a.touched);
  return [...marked, ...bare].slice(0, PROMOTED);
}
