import { describe, expect, it } from 'vitest';

import { snippetSegments } from './snippet';

/** The text back, markers gone — the property every case below also has to hold. */
const flat = (s: string) =>
  snippetSegments(s)
    .map((x) => x.text)
    .join('');

describe('the search snippet', () => {
  it('splits a marked term out of the prose around it', () => {
    expect(snippetSegments('what >>survives<< is not what was meant to')).toEqual([
      { text: 'what ', match: false },
      { text: 'survives', match: true },
      { text: ' is not what was meant to', match: false },
    ]);
  });

  it('carries the elision through as ordinary text', () => {
    // `…` is sqlite's, and it means *there was more*. It is not a match and it
    // is not ours to re-word.
    expect(snippetSegments('…and then >>grief<…').map((s) => s.match)).toEqual([false]);
    expect(snippetSegments('…the >>bells<< and then…')).toEqual([
      { text: '…the ', match: false },
      { text: 'bells', match: true },
      { text: ' and then…', match: false },
    ]);
  });

  it('marks every term rather than the first', () => {
    const segs = snippetSegments('>>a<< place is still >>there<<');
    expect(segs.filter((s) => s.match).map((s) => s.text)).toEqual(['a', 'there']);
  });

  it('leaves an opener with no closer as text', () => {
    // Real prose contains `>>`. Emphasising to the end of the string on the
    // strength of one marker is the failure mode this degrades away from —
    // `docs/decisions.md` records the structured snippet as the engine fix.
    const s = 'she wrote >> in the margin and never closed it';
    expect(snippetSegments(s)).toEqual([{ text: s, match: false }]);
    expect(flat(s)).toBe(s);
  });

  it('draws no empty mark', () => {
    expect(snippetSegments('before>><<after')).toEqual([
      { text: 'before', match: false },
      { text: 'after', match: false },
    ]);
  });

  it('is empty for an empty snippet, and never a mark of nothing', () => {
    expect(snippetSegments('')).toEqual([]);
  });

  it('never edits the reader’s text — only the delimiters go', () => {
    // The hostile set: markup that is not markup, comparison operators, an
    // unpaired marker, and a snippet that is nothing but a match. In every one,
    // the text a reader sees is the text the engine sent.
    const cases: [string, string][] = [
      ['a <script>alert(1)</script> b', 'a <script>alert(1)</script> b'],
      ['1 < 2 and 3 > 2', '1 < 2 and 3 > 2'],
      ['quoting >> like mail does', 'quoting >> like mail does'],
      ['>>only<<', 'only'],
      ['no markers at all', 'no markers at all'],
    ];
    for (const [snippet, seen] of cases) {
      expect(flat(snippet), snippet).toBe(seen);
    }
  });
});
