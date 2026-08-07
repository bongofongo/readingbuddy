/**
 * The search snippet, un-mixed (item 50).
 *
 * `Storage::search_marks` builds every snippet with sqlite's own
 * `snippet(…, -1, '>>', '<<', '…', 12)`, so what crosses the seam is **plain
 * text with two in-band markers**: `>>` opens a matched term, `<<` closes it,
 * `…` is the elision, twelve tokens of context either side. It is the reader's
 * own prose and it is **not escaped** — a passage containing `<script>` arrives
 * verbatim, which is why this returns segments for the markup to render and
 * never a string for `{@html}`.
 *
 * ## The markers are ambiguous, and that is recorded rather than papered over
 *
 * `>>` and `<<` are ordinary characters in real prose — nested mail quoting,
 * guillemets typed as ASCII — and a snippet cannot say which `>>` the engine
 * wrote. No frontend can resolve that; it is an engine item (a structured
 * snippet carrying offsets instead of delimiters), and until it lands the rule
 * here is **degrade, never guess**: an opener with no closer after it is text,
 * not an emphasis running to the end of the line. The worst case is then a
 * missed highlight inside a sentence the reader can still read, rather than
 * half a snippet rendered as a match.
 *
 * The CLI prints these markers raw (`crates/cli/src/commands/find.rs`), so this
 * is the first parser of them anywhere. It is deliberately the only one: the
 * second would be the divergence item 17 exists to prevent.
 */

/** One run of snippet text, and whether the engine marked it as a match. */
export type Segment = { text: string; match: boolean };

const OPEN = '>>';
const CLOSE = '<<';

/**
 * The snippet as runs of matched and unmatched text, in order.
 *
 * Concatenating every `text` reproduces the snippet with its markers removed —
 * which is the property the tests pin, because it is what makes this a
 * *rendering* of the engine's answer rather than an edit of it.
 */
export function snippetSegments(snippet: string): Segment[] {
  const out: Segment[] = [];
  let rest = snippet;

  while (rest.length > 0) {
    const open = rest.indexOf(OPEN);
    if (open === -1) break;
    const close = rest.indexOf(CLOSE, open + OPEN.length);
    // An opener with no closer is prose that happens to contain `>>`. Leave the
    // whole remainder as text rather than emphasising to the end of the string.
    if (close === -1) break;

    if (open > 0) out.push({ text: rest.slice(0, open), match: false });
    const inner = rest.slice(open + OPEN.length, close);
    // `>><<` is an empty match. It carries no text to draw and an empty `<mark>`
    // is a rectangle with nothing in it.
    if (inner.length > 0) out.push({ text: inner, match: true });
    rest = rest.slice(close + CLOSE.length);
  }

  if (rest.length > 0) out.push({ text: rest, match: false });
  return out;
}
