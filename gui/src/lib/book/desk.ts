/**
 * What the book page's work surface is showing.
 *
 * ## It used to be a centre column between two rails
 *
 * The book page was three columns — *where you navigate*, *what you are doing*,
 * *what it connects to* — and this was the middle one's state. Twelve controls
 * were resident at all times: three write verbs, four destinations, a note list,
 * a search box, a link search and a citation list, none of which the reader had
 * asked for at the moment they were drawn.
 *
 * The minimal pass cut it to **one work surface and one row of controls**. The
 * argument that held the rails up was that a permanent rail is what keeps the
 * page from being modal — and that is true of a rail and not only of a rail.
 * A surface with a URL, a visible selector naming every alternative, and no
 * dismissal gesture is not a mode; `/reading` (entry 54) has made exactly that
 * case since it shipped, with four panels and one at a time. What a rail buys
 * over a selector is that the *contents* of the other places are legible without
 * going there, and that is worth much less than it costs: nobody reads a note
 * list while writing into a different note.
 *
 * So this type still names the destinations and still is not a shape of the
 * page. What changed is that they are now six rather than five, because *the
 * note list* became one of them instead of living in the rail.
 *
 * ## Six values, four places
 *
 * `note` and `compose` are both **inside** `notes` — the list is the place, the
 * editor and the composer are what you do there — which is what [`place`] says
 * and what keeps the selector to four members while the state has six. Lighting
 * *Notes* while a note is open is the honest answer to *where am I*, and it is
 * also the way back out: the same control the reader is looking at returns them
 * to the list.
 */
export type Centre = 'passages' | 'notes' | 'note' | 'compose' | 'reads' | 'about';

/** The four the selector offers, in the order it offers them. */
export type Place = 'passages' | 'notes' | 'reads' | 'about';

/**
 * Which of the four places a given state is in.
 *
 * The mapping is only interesting for `note` and `compose`, and it exists as a
 * function rather than as a ternary in the markup because it is a decision about
 * what this app considers *a place*, and there is exactly one right answer that
 * every caller has to agree on.
 */
export function place(centre: Centre): Place {
  return centre === 'note' || centre === 'compose' ? 'notes' : centre;
}
