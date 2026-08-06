/**
 * The shelf's layout seam — one registry, one prop contract, N arrangements.
 *
 * ## Why this exists at all
 *
 * Item 26 was specified as a WebGL spine shelf and shipped as a cover grid.
 * That is a **cosmetic** decision and it was taken as one: the spine shelf is
 * deferred, not abandoned, and `docs/decisions.md` entry 26 records it. The
 * cost of deferring it badly would be a grid welded into `+page.svelte`, so
 * that the day somebody builds spines they are not building a layout but
 * unpicking one.
 *
 * So the arrangement is a **value**, not a shape of the page. A layout is a
 * component plus a name; adding one is appending to [`LAYOUTS`] and writing the
 * component. Nothing else in the app learns it exists.
 *
 * ## Two implementations, on purpose
 *
 * There are two layouts and the second is not decoration. An abstraction with a
 * single implementation is a guess about what varies, and this repo has a
 * standing rule about guards that cannot fail — a seam nothing has ever been
 * swapped through is the same defect wearing a different hat. `Rows` exists so
 * that the contract below is known to be sufficient rather than believed to be:
 * it needs a different box per book, no aspect reservation, and it puts the
 * title where the grid puts a cover. The spine shelf, when it lands, is a third
 * entry here and should need no change to this file.
 *
 * ## What a layout is NOT allowed to do
 *
 * Decide which books it shows, or in what order. It receives a list and renders
 * it. Sorting, filtering and the currently-reading predicate are the engine's
 * (item 17) — a layout that sorted would put a fourth spelling of "recent" in
 * the repo, and the two frontends would have to be edited together for ever.
 */
import type { Component } from 'svelte';

import type { StoredBook } from '$lib/api/client';

import CoverGrid from './CoverGrid.svelte';
import Rows from './Rows.svelte';

/**
 * Everything a layout is given.
 *
 * Deliberately one field. Each addition here is a thing every future layout has
 * to accept, so the bar is that no layout can be written without it.
 */
export type ShelfLayoutProps = {
  books: StoredBook[];
};

export type ShelfLayoutId = 'grid' | 'rows';

export type ShelfLayout = {
  id: ShelfLayoutId;
  /** What the switch calls it. A noun, not an instruction. */
  label: string;
  component: Component<ShelfLayoutProps>;
};

/**
 * The registry. Order is the order the switch offers them; the first is the
 * default.
 *
 * Typed as a **non-empty** tuple rather than an array, so "there is always a
 * layout to fall back to" is a fact the compiler holds rather than a comment.
 * `tsconfig` runs `noUncheckedIndexedAccess`, so a plain `ShelfLayout[]` would
 * make every `LAYOUTS[0]` below an `| undefined` needing a runtime guard for a
 * case that cannot happen.
 */
export const LAYOUTS: [ShelfLayout, ...ShelfLayout[]] = [
  { id: 'grid', label: 'Covers', component: CoverGrid },
  { id: 'rows', label: 'List', component: Rows },
];

export const DEFAULT_LAYOUT: ShelfLayoutId = LAYOUTS[0].id;

/** The layout with this id, or the default. Never throws: see [`recallLayout`]. */
export function layoutById(id: string | null): ShelfLayout {
  return LAYOUTS.find((l) => l.id === id) ?? LAYOUTS[0];
}

const STORAGE_KEY = 'readingbuddy.shelf.layout';

/**
 * Which layout to open with.
 *
 * A view preference, so it lives in the frontend — this is the one class of
 * state `gui/CLAUDE.md`'s "no decisions in Svelte" rule does not reach, because
 * the TUI does not want the answer and nothing is derived from it. The terminal
 * sibling makes the same call in the other direction and persists its own sort
 * to `tui.toml`.
 *
 * **An unreadable store is not an error.** A private-mode webview throws on
 * `localStorage` access rather than returning null, and a shelf that failed to
 * render because it could not remember a preference would be the tail wagging
 * the dog. Both halves degrade to the default and say nothing.
 */
export function recallLayout(): ShelfLayoutId {
  try {
    return layoutById(localStorage.getItem(STORAGE_KEY)).id;
  } catch {
    return DEFAULT_LAYOUT;
  }
}

export function rememberLayout(id: ShelfLayoutId): void {
  try {
    localStorage.setItem(STORAGE_KEY, id);
  } catch {
    /* see `recallLayout` — a preference that cannot be saved is not a failure. */
  }
}
