/**
 * The axiom, over this app's **own** strings (item 52).
 *
 * `docs/decisions.md` bans task-completion framing and `gui/CLAUDE.md` sharpens
 * it to one sentence — *the app tells you what you did, it never tells you what
 * you have left*. The word **yet** is the smallest way to break it: it turns an
 * absence into something outstanding, so *nothing on the shelf yet* says the
 * shelf is short of where it should be, while *nothing on the shelf* is a fact
 * about a library.
 *
 * ## Why this is a source scan and not another route assertion
 *
The route suite that once forbade the completion words in a rendered
 * body **could not have caught any of these**, which is why this scan exists and
 * why it outlived that suite. Six of the seven lived in *empty states*, and
 * `fake.ts` is deliberately a library with books, notes, highlights and months in
 * it — so the markup that says them was never on screen for a renderer to read. A
 * guard that can only fire on a branch the fixture never takes is the guard this
 * repo keeps writing down as worse than none. Scanning the source has no such
 * blind spot, and it is now the only check of this rule there is.
 *
 * The scan is over **markup only**: the `<script>` and `<style>` blocks and the
 * HTML comments come out first, because a comment saying *no "yet" here* is the
 * rule being taught and must not be the rule being broken. Two of the routes
 * carry exactly that comment today.
 */
import { describe, expect, it } from 'vitest';

/**
 * Every component and route, as text.
 *
 * Vite's own glob rather than `node:fs`: this project ships no `@types/node`,
 * and adding a dependency so a test can walk a directory would be the tail
 * wagging the lockfile. It is also the same resolution the app builds with, so
 * a file this cannot see is a file that does not ship.
 */
const SOURCES = import.meta.glob('/src/**/*.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** What a reader can actually read: no script, no style, no comments. */
function markup(source: string): string {
  return source
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '');
}

/**
 * One word per row, and each row carries why.
 *
 * Deliberately narrow. The other completion words — *unread*, *streak*, *goal*,
 * *remaining* — are asserted against the rendered library surface, where they
 * would appear if they appeared at all; **yet** is the one that hides in the
 * branch a full fixture never renders.
 */
const BANNED: [RegExp, string][] = [[/\byet\b/i, 'turns an absence into something outstanding']];

describe('this app never says what you have left', () => {
  const files = Object.entries(SOURCES);

  it('finds the components to check at all', () => {
    // A glob that resolved to nothing would make every assertion below pass
    // without reading a line — the failure mode `dialect.test.ts` exists for.
    expect(files.length).toBeGreaterThan(10);
  });

  for (const [banned, why] of BANNED) {
    it(`never writes "${banned.source}" — it ${why}`, () => {
      const offenders = files
        .filter(([, source]) => banned.test(markup(source)))
        .map(([path]) => path);
      expect(offenders, `${offenders.join(', ')}: ${why}`).toEqual([]);
    });
  }

  it('reads the markup and not the comment teaching the rule', () => {
    // The guard has to survive its own explanation: `Card.svelte` and both new
    // routes carry a comment forbidding the word by name, and a scan that fired
    // on those would be deleted within a wave.
    const text = markup(`<script>const yet = 1;</script>
      <!-- **No "yet"**: that one word turns an absence into an omission. -->
      <p>Nothing on the shelf.</p>
      <style>.yet { color: red; }</style>`);
    expect(text).not.toMatch(/\byet\b/i);
    expect(text).toContain('Nothing on the shelf.');
  });
});
