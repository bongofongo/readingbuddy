/**
 * The links pane's rows: out first, in after, each side counted.
 *
 * ## Why this is the frontend's and not an engine gap
 *
 * `Backlinks` and `OutgoingLinks` are two requests answering two different
 * questions, and the engine is right to keep them apart — one is a `WHERE
 * to_note = ?` and the other reads what a note *wrote*. Putting them one after
 * the other in a pane, and saying how many are on each side, is arrangement:
 * the TUI's `LinksPane::out_count` does exactly this, in Rust, above the same
 * two calls. So this is item 17's line falling on the frontend side, and a
 * `LinksDto` would be the engine deciding what a pane looks like.
 *
 * ## The two things the shape has to preserve
 *
 * **A dangling target is text, not an error.** `OutgoingLinkDto.note` is `null`
 * for a `[[wikilink]]` naming a note nobody has written, and that edge resolves
 * itself the day they do. Dropping it would delete the one feature the vault
 * has that a folder of files does not.
 *
 * **The direction is carried in the data, not in a colour.** The TUI's pane
 * puts an arrow in the text for exactly this reason — a styled-only distinction
 * is invisible to the eye that most needs it, and to the test.
 */
import type { NoteDto, OutgoingLinkDto } from '$lib/api/bindings';

/** One edge, in the direction it was found. */
export type LinkRow =
  | {
      dir: 'out';
      /** The `[[wikilink]]` as written — which is all there is when it dangles. */
      title: string;
      /** The note it resolves to, or `null` for a live forward reference. */
      note: NoteDto | null;
    }
  | { dir: 'in'; title: string; note: NoteDto };

export type LinkPane = {
  rows: LinkRow[];
  /** Past tense both: edges that exist, never edges that are missing. */
  outgoing: number;
  incoming: number;
};

/**
 * Both directions, as one list.
 *
 * Outbound first because that is the order the note itself is in: what it says,
 * then what says it. Neither side is re-sorted — the engine returned them in
 * its own order and a second ordering here would be a relevance claim nobody
 * made.
 */
export function linkPane(out: OutgoingLinkDto[], back: NoteDto[]): LinkPane {
  const rows: LinkRow[] = [
    ...out.map((l): LinkRow => ({ dir: 'out', title: l.target_title, note: l.note })),
    ...back.map((n): LinkRow => ({ dir: 'in', title: n.title, note: n })),
  ];
  return { rows, outgoing: out.length, incoming: back.length };
}

/**
 * Whether an outbound row is still only text.
 *
 * A predicate rather than `row.note === null` spelled at three call sites: the
 * inbound direction cannot dangle at all (a backlink is a note by construction),
 * so the check is not symmetric and reads wrong when written out inline.
 */
export function isForwardReference(row: LinkRow): boolean {
  return row.dir === 'out' && row.note === null;
}
