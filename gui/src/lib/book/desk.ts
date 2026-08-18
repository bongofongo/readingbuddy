/**
 * What the desk's centre column is showing.
 *
 * The book page is three columns — *where you navigate*, *what you are doing*,
 * *what it connects to* — and this is the middle one's state. It is a value
 * rather than a shape of the page for the same reason the shelf's arrangement
 * is: the rail swaps the centre, and nothing else in the app learns what states
 * exist.
 *
 * **Nothing here is modal.** Every other destination is on screen while you are
 * in any one of them, which is what the left rail buys and what makes swapping
 * the centre legitimate rather than a dialog with better manners.
 */
export type Centre = 'passages' | 'note' | 'compose' | 'reads' | 'about';

/**
 * Whether the right rail is showing a note's connections or the book's.
 *
 * The right rail is an **inspector**: its contents depend on what the centre is
 * doing, because "find the note to link to" is an instrument acting on the
 * editor rather than reference material sitting beside it. Writing a note and
 * finding the note to link to is one operation, which is what justifies the
 * permanent column.
 */
export function inspects(centre: Centre): 'note' | 'book' {
  return centre === 'note' || centre === 'compose' ? 'note' : 'book';
}
