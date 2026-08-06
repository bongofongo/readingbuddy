/**
 * A moment, said out loud — and the move it ends in.
 *
 * `MomentKindDto`'s own doc draws the line this module sits on: *"the wording is
 * a frontend's and the fact is the engine's… `run_ended` carries a span and a
 * number of days, and the sentence built out of them — including whether '3' is
 * spelled *three* — belongs to whoever is drawing."* This is that sentence, kept
 * out of the component so the two rules below can be **asserted** rather than
 * reviewed.
 *
 * ## Rule one: a moment ends in a move, never in a dismissal
 *
 * `gui-vision.md:121`: *"a moment that ended in a dismissable dialog would be a
 * task-completion popup wearing a costume. A moment that ends with a cursor in
 * an empty reflection is the app doing what it exists for."* So every kind
 * resolves to a [`MomentMove`], and there is deliberately no `dismiss` arm — a
 * moment that names no book still names a place you can go.
 *
 * ## Rule two: `run_ended` is spoken as its span and never as a count
 *
 * This is the one line in item 28 that had to be argued rather than written.
 * `days` is on the DTO and entry 23 defends it: the run is recognised only
 * *after* it is over, `RUN_MIN_DAYS` is 2 because that is what "consecutive"
 * means, and a count of your own past days is the kind item 17 permits. All
 * true. But the moment surfaces on the **home surface**, and the rule sharpened
 * this session says a number there *"may describe one book, never the
 * collection, never what is left"* — and a run of days describes neither one
 * book nor the collection. It describes a habit, which is one product decision
 * from a streak.
 *
 * The span carries the same fact and cannot be read as a target: *from the 1st
 * to the 3rd* is a closed thing that happened, and there is no number in it to
 * beat. So the count is dropped at the phrasing layer, which is exactly the
 * layer entry 23 says owns it, and the engine keeps the field for whoever draws
 * a reading-life page — where a count is permitted because you chose to go
 * there.
 */
import type { MomentDto } from '$lib/api/bindings';

/**
 * Where a moment lets you go next.
 *
 * `reflect` is the payload of the ceremony and carries the **reading**, not just
 * the book: `MomentDto.reading_id` exists because a card is minted per reading
 * and a reread has two, and dropping it here would put the reflection on
 * whichever read `open_anchored` guesses.
 */
export type MomentMove =
  | { move: 'reflect'; bookId: number; readingId: number | null; invitation: string }
  | { move: 'note'; bookId: number; noteId: number; invitation: string }
  | { move: 'life'; invitation: string };

export type MomentSentence = {
  /** What happened. Past tense, always, and with no count in it. */
  said: string;
  move: MomentMove;
};

/** A book's title, or `null` when it has not been read yet — never a guess. */
export type TitleOf = (bookId: number) => string | null;

/**
 * The sentence, and the move.
 *
 * A title that is not loaded yet reads as *a book* rather than as an empty gap
 * or a spinner: the moment is worth saying before its ornament arrives, and the
 * sentence is grammatical either way.
 */
export function momentSentence(m: MomentDto, titleOf: TitleOf): MomentSentence {
  const kind = m.kind;
  switch (kind.kind) {
    case 'reading_closed':
      return {
        said: `You finished ${named(m.book_id, titleOf)}.`,
        move: reflect(m, 'Write what you thought'),
      };
    case 'first_annotation':
      return {
        said: `You marked your first passage in ${named(m.book_id, titleOf)}.`,
        // The reflection is a note *kept as you go* — `open_anchored` opens or
        // mints one and the vault fixture's own body says "added to as I go" —
        // so inviting one this early is right rather than premature.
        move: reflect(m, 'Start a reflection'),
      };
    case 'reflection_reached':
      return {
        said: `Your reflection on ${named(m.book_id, titleOf)} reached ${named(
          kind.reached_book_id,
          titleOf,
        )}.`,
        move:
          m.book_id === null
            ? { move: 'life', invitation: 'Your reading life' }
            : {
                move: 'note',
                bookId: m.book_id,
                noteId: kind.note_id,
                invitation: 'Open what you wrote',
              },
      };
    case 'run_ended':
      return {
        // The span, not the count. See the module doc — this is the argued line.
        said: `You read every day from ${kind.from} to ${kind.to}.`,
        move: { move: 'life', invitation: 'Your reading life' },
      };
    default:
      // ts-rs drops `#[serde(other)]`, so this union is exhaustive over *today's*
      // kinds and the wire is not. A kind a newer engine grew degrades into a
      // sentence that is true of every moment rather than failing to render —
      // and it still ends somewhere, because nothing is a dead end.
      return {
        said: 'Something happened worth noticing.',
        move: { move: 'life', invitation: 'Your reading life' },
      };
  }
}

function named(bookId: number | null, titleOf: TitleOf): string {
  if (bookId === null) return 'a book';
  return titleOf(bookId) ?? 'a book';
}

/**
 * A moment about a book, ending in its reflection.
 *
 * `book_id` is nullable on the DTO — absent only for `run_ended`, which this is
 * never called for — so the fall-back exists to be *unreachable* rather than to
 * be useful, and it goes somewhere rather than nowhere.
 */
function reflect(m: MomentDto, invitation: string): MomentMove {
  if (m.book_id === null) return { move: 'life', invitation: 'Your reading life' };
  return { move: 'reflect', bookId: m.book_id, readingId: m.reading_id, invitation };
}
