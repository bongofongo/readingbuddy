/**
 * The two surfaces of the reading-life page, and the seam that lets there be a
 * third.
 *
 * ## Why the page has tabs at all
 *
 * `/life` answers two questions that want opposite treatments. *What did my
 * reading look like* is a recognition question — a run of months and the covers
 * that closed in them, read by scrolling, no controls. *What can this data
 * actually tell me* is an interrogation, and it wants density, rankings and
 * comparisons that the first question is actively harmed by.
 *
 * One surface answering both made the first a preamble to the second. Entry 57's
 * first corollary — **one question per surface** — says that is two surfaces,
 * and the tab is how a reader chooses which one they are on.
 *
 * ## The order is the argument
 *
 * `timeline` is first and is what the page opens on, and that is the whole
 * design: the reader is **not confronted** with rankings and comparisons, they
 * go and get them. `docs/decisions.md` entry 58 is the settled account of why
 * that ordering is what makes the second tab permissible at all — the same
 * material, drawn on arrival, would be the app telling you how you are doing
 * before you asked.
 *
 * ## Shaped like `$lib/book/desk.ts`, and deliberately separate from it
 *
 * A closed union plus an ordered registry, so a third view is one member and one
 * `{#if}` arm rather than a refactor. It has no collapse function because,
 * unlike `Centre`, no state here is nested inside another — if a future view
 * grows sub-states, that is when it earns a `place()` of its own.
 *
 * **No count on a label, ever.** `decisions.md` entry 51 rules that a figure on
 * a control is one decision from the badge the axiom bans, and entry 58 does not
 * touch that: *Everything* never becomes *Everything (11)*.
 */

/** Which surface of the page is up. */
export type View = 'timeline' | 'everything';

/**
 * The views the selector offers, **in the order it offers them**.
 *
 * The order is load-bearing (see the module doc) and the first member is the
 * default. The labels live here rather than in the route because they are the
 * names of the two questions, not a rendering detail — the route's job is to
 * draw them, not to decide what they are called.
 */
export const VIEWS: [View, string][] = [
  ['timeline', 'Timeline'],
  ['everything', 'Everything'],
];
