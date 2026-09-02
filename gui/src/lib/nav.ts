/**
 * Where a link goes: the two routing decisions this app makes about a book, and
 * what *back* means on a surface that can be reached from more than one place.
 *
 * ## Why routing is a module and not an `href` in a component
 *
 * `gui/CLAUDE.md`'s rule is that a component is presentation and event dispatch,
 * and a destination that depends on the *state of the row* is not presentation —
 * it is a decision, taken in three places (the wall's tile, the "Reading now"
 * preview, and anything later that draws a book), and three copies of it drift.
 * It is not the engine's either: the engine owns the *fact* that a reading is
 * open (`reading_state`), and this file owns what a frontend does about it,
 * which is the same line `phrasing.ts` draws for words.
 *
 * ## The two decisions
 *
 * **A book you are in the middle of opens into reading mode.** Everywhere. A
 * book you are not opens its page. That is the whole rule, and it follows the
 * brief: the thing you do with a book you are reading is *read it*, and the
 * thing you do with one you are not is *look at it*.
 *
 * `reading_state` is enough to decide it even though it cannot name *which*
 * reading is open on a reread (`client.ts` says so where `currentlyReading`
 * is declared) — `/reading` resolves the book against the engine's own open set
 * and falls back rather than refusing, so a stale `reading` state costs a
 * redirect and never a dead end.
 *
 * ## And what *back* is
 *
 * `docs/decisions.md`'s axiom: nothing is a dead end. A leaf surface therefore
 * carries the way out — and once a leaf is reachable from two places, a *fixed*
 * way out sends half its visitors somewhere they were not. So the back link
 * names the page you actually came from, and falls back to the entrance when
 * there is no such page: a fresh window, a reload, a URL somebody pasted.
 *
 * **It is a link and not `history.back()`.** A real `href` can be middle-clicked
 * and read by a screen reader, it works on the reload where the history stack
 * has nothing in it, and the label can say where it goes — which is the part
 * that makes it not a mystery button.
 */
import type { StoredBook } from '$lib/api/client';

/** The entrance. `/` is the books you have open (was the wall, until it was not). */
export const HOME = '/';

/** The whole collection. */
export const LIBRARY = '/library';

/**
 * Where clicking this book goes.
 *
 * A book with no `id` has no page at all — that is a row the engine has not
 * stored, which `StoredBook`'s nullable id admits — and it links to the library
 * rather than to `/book/null`.
 */
export function bookHref(book: Pick<StoredBook, 'id' | 'reading_state'>): string {
  if (book.id === null) return LIBRARY;
  return book.reading_state?.state === 'reading' ? readingHref(book.id) : `/book/${book.id}`;
}

/** Reading mode, for one book. The query string is the subject of that route. */
export function readingHref(bookId: number): string {
  return `/reading?book=${bookId}`;
}

/** A way out: where it goes, and the word for it. */
export type Back = { href: string; label: string };

/**
 * What each place is called when a back link names it.
 *
 * The nav's own words, so the link a reader follows says the same thing as the
 * entry it lands on. `/book/:id` is not in the nav and is *The book* — the
 * title would be better and is not available here: this module is given a URL,
 * not a library.
 */
const PLACES: [RegExp, string][] = [
  [/^\/$/, 'Reading now'],
  [/^\/library$/, 'Library'],
  [/^\/notes$/, 'Notes'],
  [/^\/cards$/, 'Cards'],
  // `/cards/history` before `/cards` would be redundant — the patterns are
  // anchored, so order is not doing any work here — but the entry is required:
  // the wall moved there in the minimal pass, and without a row of its own a
  // book reached from the wall would offer *Back* rather than the place it came
  // from. It is worded as the door that leads to it.
  [/^\/cards\/history$/, 'Every card'],
  [/^\/life$/, 'Reading life'],
  [/^\/devices$/, 'Devices'],
  [/^\/book\/\d+\/cards$/, 'The cards'],
  [/^\/book\/\d+$/, 'The book'],
  [/^\/reading$/, 'Reading'],
];

/** The entrance, which is where back goes when there is nowhere to go back to. */
const ENTRANCE: Back = { href: HOME, label: 'Reading now' };

/**
 * The way back from `here`, given the page navigated from.
 *
 * Three cases collapse to the entrance, and each is a real one:
 *
 * - **no previous page** — a reload, a pasted URL, the window opening here.
 * - **the previous page is this one** — reading mode switching book, or the
 *   book page opening a note, both of which write to their own query string. A
 *   back link to the surface you are standing on is the dead end this exists to
 *   prevent.
 * - **an off-site URL** — nothing outside the app is a place this can name.
 *
 * The search string is kept, so backing out of a book returns to the same
 * `/reading?book=3` rather than to whichever book happens to be first.
 */
export function backTarget(here: URL, from: URL | null): Back {
  if (from === null) return ENTRANCE;
  if (from.origin !== here.origin) return ENTRANCE;
  if (from.pathname === here.pathname) return ENTRANCE;
  const label = PLACES.find(([at]) => at.test(from.pathname))?.[1] ?? 'Back';
  return { href: `${from.pathname}${from.search}`, label };
}
