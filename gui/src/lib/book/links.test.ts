/**
 * The links pane's arrangement, and the one property that must survive it.
 *
 * A dangling `[[wikilink]]` is the vault's best trick — the edge completes the
 * day the other note is written — and a pane that dropped it, or rendered it as
 * an error, would delete the feature. So the shape is asserted rather than
 * looked at: a screenshot cannot tell a missing row from a row that was never
 * there.
 */
import { describe, expect, it } from 'vitest';

import type { NoteDto, OutgoingLinkDto } from '$lib/api/bindings';
import { isForwardReference, linkPane } from './links';

function note(id: number, title: string): NoteDto {
  return {
    id,
    book_id: 3,
    reading_id: null,
    highlight_id: null,
    page: null,
    location: null,
    file_path: `${id}.md`,
    title,
    kind: 'note',
    created_at: 1735689600,
  };
}

const RESOLVED: OutgoingLinkDto = { target_title: 'On The Doorstop', note: note(1, 'On The Doorstop') };
const DANGLING: OutgoingLinkDto = { target_title: 'A Note Nobody Wrote', note: null };

describe('the links pane', () => {
  it('puts what the note says before what says it', () => {
    const pane = linkPane([RESOLVED], [note(2, 'Reflection')]);
    expect(pane.rows.map((r) => r.dir)).toEqual(['out', 'in']);
  });

  it('counts each direction separately', () => {
    const pane = linkPane([RESOLVED, DANGLING], [note(2, 'Reflection')]);
    expect(pane.outgoing).toBe(2);
    expect(pane.incoming).toBe(1);
  });

  it('keeps a dangling target as a row, carrying the text that was written', () => {
    // The whole point. It is not an error and not a missing note — it is a
    // forward reference, and it resolves itself later.
    const pane = linkPane([DANGLING], []);
    expect(pane.rows).toHaveLength(1);
    expect(pane.rows[0]).toMatchObject({ dir: 'out', title: 'A Note Nobody Wrote', note: null });
    expect(isForwardReference(pane.rows[0]!)).toBe(true);
  });

  it('never calls an inbound row a forward reference', () => {
    // A backlink is a note by construction — `backlinks` is a `WHERE to_note =
    // ?` and there is no dangling half of it. The predicate is asymmetric on
    // purpose, and `row.note === null` spelled inline would read as if it
    // were not.
    const pane = linkPane([], [note(2, 'Reflection')]);
    expect(pane.rows.every((r) => !isForwardReference(r))).toBe(true);
  });

  it('reorders neither side', () => {
    // Each list arrives in the engine's own order and nothing above the seam
    // re-sorts it — a second ordering here would be a relevance claim no
    // request made.
    const out = [DANGLING, RESOLVED];
    const back = [note(9, 'Zeta'), note(4, 'Alpha')];
    const pane = linkPane(out, back);
    expect(pane.rows.map((r) => r.title)).toEqual([
      'A Note Nobody Wrote',
      'On The Doorstop',
      'Zeta',
      'Alpha',
    ]);
  });

  it('is empty rather than absent when a note is in no edges at all', () => {
    const pane = linkPane([], []);
    expect(pane.rows).toEqual([]);
    expect(pane.outgoing).toBe(0);
    expect(pane.incoming).toBe(0);
  });
});
