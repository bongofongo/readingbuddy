/**
 * The fake's per-book search (item 50).
 *
 * What is asserted here is the **arithmetic and the scope** — the parts a
 * caller can get wrong and the parts the real engine is contractual about. The
 * *ranking* is deliberately not asserted: this fixture matches substrings and
 * the engine ranks with bm25 over two indexes, so a test pinning an order here
 * would pin this file's opinion, which exists nowhere below the seam.
 */
import { describe, expect, it } from 'vitest';

import { snippetSegments } from '../book/snippet';
import { FakeClient } from './fake';

const api = () => new FakeClient();

describe('searching one book', () => {
  it('answers with both kinds in one list', async () => {
    // "survives" is a passage in book 3; "The Doorstop" is in two of its note
    // titles. One query that reaches both is the shape of the method.
    const hits = await api().searchMarks('the', 3, 50);
    expect(hits.some((h) => h.kind === 'note')).toBe(true);
    expect(hits.some((h) => h.kind === 'highlight')).toBe(true);
  });

  it('never returns another book’s marks', async () => {
    // The scope is the engine's predicate, not a filter over the answer — and
    // this is the assertion that would fail if a caller reintroduced the second
    // spelling. Book 11's passage says "map"; book 3's say nothing of the kind.
    const hits = await api().searchMarks('map', 3, 50);
    expect(hits).toEqual([]);
    const elsewhere = await api().searchMarks('map', 11, 50);
    expect(elsewhere.map((h) => h.kind)).toContain('highlight');
  });

  it('searches a note’s body and not only its title', async () => {
    // `notes` has no body column and `notes_fts` indexes title *and* body, so a
    // fixture that searched titles alone would make the commonest kind of hit
    // untestable.
    const [hit] = await api().searchMarks('hung off a single passage', 3, 50);
    expect(hit?.kind === 'note' && hit.note.id).toBe(3);
  });

  it('searches what the reader wrote against a passage, and what the device did', async () => {
    // `snippet(…, -1, …)` picks whichever column matched, so a hit's snippet is
    // not necessarily its passage text. Both of these prove the point.
    const ours = await api().searchMarks('arguing with this sentence', 3, 50);
    expect(ours.map((h) => h.kind)).toEqual(['highlight']);
    const theirs = await api().searchMarks('what did they mean', 3, 50);
    expect(theirs.map((h) => h.kind)).toEqual(['highlight']);
  });

  it('marks the terms it matched, and the text survives un-edited', async () => {
    // Found by kind rather than by position — "survives" is also a note title
    // here, and taking `[0]` would be this test asserting an order it has just
    // said nothing above the seam may depend on.
    const hits = await api().searchMarks('survives', 3, 50);
    const hit = hits.find((h) => h.kind === 'highlight');
    expect(hit).toBeDefined();
    const segs = snippetSegments(hit!.snippet);
    expect(segs.filter((s) => s.match).map((s) => s.text)).toEqual(['survives']);
    expect(segs.map((s) => s.text).join('')).toContain('is not what was meant to');
  });

  it('treats an empty or blank query as not asking', async () => {
    // The engine issues no statement for one and answers `Ok(vec![])`, which is
    // what lets a box send every keystroke without guarding for blankness.
    expect(await api().searchMarks('', 3, 50)).toEqual([]);
    expect(await api().searchMarks('   ', 3, 50)).toEqual([]);
  });

  it('takes a limit of zero as an empty list, never as no limit', async () => {
    // `listBooks` reads a negative limit as *no limit* and `listNotes` reads an
    // absent one as *every note*. Three neighbours, three meanings — this is
    // the one a `?? 0` would silently break.
    expect(await api().searchMarks('the', 3, 0)).toEqual([]);
    expect(await api().searchMarks('the', 3, -1)).toEqual([]);
    expect((await api().searchMarks('the', 3, 1)).length).toBe(1);
  });

  it('does not let one kind crowd the other out of a small limit', async () => {
    // The merge is by within-source position in the engine and here, which is
    // what stops a book with thirty passages and one note answering a two-row
    // limit with two passages.
    const hits = await api().searchMarks('the', 3, 2);
    expect(new Set(hits.map((h) => h.kind)).size).toBe(2);
  });

  it('takes punctuation as text rather than as syntax', async () => {
    // Below the seam every token is quoted into a phrase, so `don't` and `C++`
    // are searches rather than the fts5 errors the old `search_notes` produced.
    // Nothing here may throw.
    await expect(api().searchMarks("don't", 3, 50)).resolves.toEqual([]);
    await expect(api().searchMarks('C++', 3, 50)).resolves.toEqual([]);
    await expect(api().searchMarks('*', 3, 50)).resolves.toBeInstanceOf(Array);
  });
});
