/**
 * Reading mode's state — what is open, what a key does, and what a typed page
 * has to be before it is sent.
 *
 * ## What this module is defending
 *
 * Reading mode shows the book and nothing else. Everything you can do to it —
 * write a note, say where you are, look at what came off the device, read
 * something else instead — is behind one of four verbs, and **at most one of
 * them is open at a time**. That single-slot rule is the whole design and it is
 * the thing a later screen will be tempted to relax, so it lives in a type here
 * rather than in four booleans in a component.
 *
 * ## Why this is not the book page's `Centre`, which it looks exactly like
 *
 * `$lib/book/desk.ts` has a value of the same shape and the two must not be
 * merged. `Centre` swaps a **work surface** on a page where every other
 * destination is on screen beside it — the rail says so in as many words, and
 * that is what makes it not modal. This one covers the book, which *is* modal
 * inside the route, and it is legitimate for a different reason: reading mode is
 * a **place** rather than a state. It has a URL, it survives a reload, the book
 * it is about is the engine's own open reading, and both exits — the book's own
 * page and the library — are on screen the whole time.
 *
 * Sharing a type between them would make that difference invisible on the day
 * somebody gives one of the two a second open panel.
 *
 * ## And the one thing that is deliberately not here
 *
 * No ordering, no progress arithmetic, no decision about which passages belong
 * to a read. `highlightsForReading` is the engine's answer in the engine's
 * order and reading mode renders it as given (item 17). What this file holds is
 * input parsing and a keyboard map, both of which are the frontend's.
 */
import type { OpenReading } from '$lib/api/client';

/**
 * Which panel is up. `none` is the resting state and is where the route starts —
 * the reader opened this to read, not to operate it.
 */
export type Panel = 'none' | 'note' | 'page' | 'passages' | 'books';

/** A verb, its word, and the key that reaches it. */
export type Verb = { panel: Exclude<Panel, 'none'>; label: string; key: string };

/**
 * The four, in the order they sit on screen.
 *
 * Ordered by how often a reader reaches for them rather than alphabetically or
 * by how they were built: a note and a page are what you do *while* reading, the
 * passages are what you look at when you stop, and changing book is the rarest
 * and sits furthest from the hand.
 *
 * The words are verbs in the reader's voice — *Note*, *Page*, *Passages*,
 * *Books* — and none of them takes a number. A count of passages on this surface
 * would be the library counting itself at exactly the moment it is supposed to
 * be out of the way.
 */
export const VERBS: Verb[] = [
  { panel: 'note', label: 'Note', key: 'n' },
  { panel: 'page', label: 'Page', key: 'p' },
  { panel: 'passages', label: 'Passages', key: 's' },
  { panel: 'books', label: 'Books', key: 'b' },
];

/**
 * What a keystroke means, or `null` for *nothing here wants it*.
 *
 * **Lowercased, and modifiers are the caller's to reject**: a `Cmd-P` is the
 * platform's print and must not open the page box, so the component checks that
 * before it asks. Deliberately no arrow keys and no numbers — this is a reading
 * surface, and a key that does something surprising while a reader is not
 * looking at the app is worse than a key that does nothing.
 */
export function panelForKey(key: string): Panel | null {
  const k = key.toLowerCase();
  if (k === 'escape') return 'none';
  return VERBS.find((v) => v.key === k)?.panel ?? null;
}

/**
 * A page number, as typed, or the refusal to show instead.
 *
 * The engine takes an `i64` and will store whatever it is given, so this is not
 * validation standing in for the engine's — it is the frontend refusing to send
 * a value the reader plainly did not mean. Three cases, and each says what would
 * work rather than what went wrong, which is this app's shape for a refusal.
 *
 * `0` is refused along with negatives. A book has no page zero, and a reader who
 * typed it meant to clear the page — which no request does, so saying so is
 * more honest than writing a page nobody is on.
 */
export type PageInput = { page: number } | { refusal: string };

export function parsePage(raw: string): PageInput {
  const trimmed = raw.trim();
  if (trimmed === '') return { refusal: 'Type the page you are on.' };
  if (!/^\d+$/.test(trimmed)) return { refusal: 'A page is a whole number — 214, say.' };
  const page = Number(trimmed);
  if (page < 1) return { refusal: 'Pages start at 1.' };
  if (!Number.isSafeInteger(page)) return { refusal: 'That is more pages than a book has.' };
  return { page };
}

/**
 * Which of the open reads this route is about.
 *
 * **The set is the engine's** — `currentlyReading` is a selection predicate and
 * lives below the seam (item 17) — and all this does is resolve `?book=` against
 * it. Falling back to the first is right rather than lazy: the request returns
 * the reads in the engine's order, so *the first one* is an answer it already
 * gave, and a route that refused to open without a valid query string would be a
 * dead end reachable by editing a URL.
 *
 * A `?book=` naming something that is not open resolves to the same fallback and
 * **not** to an error. Closing a book in another window is the ordinary way to
 * get there, and it is not a mistake anybody made.
 */
export function chooseReading(open: OpenReading[], wanted: number | null): OpenReading | null {
  if (open.length === 0) return null;
  return open.find((o) => o.book.id === wanted) ?? open[0]!;
}

/**
 * `?book=`, parsed. `null` for absent, unparseable, or out of range — all three
 * mean *you did not pick one*, and [`chooseReading`] treats them identically.
 */
export function paramBook(params: URLSearchParams): number | null {
  const raw = params.get('book');
  if (raw === null) return null;
  const n = Number(raw);
  return Number.isInteger(n) && n > 0 ? n : null;
}
