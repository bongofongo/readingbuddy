import { expect, test } from '@playwright/test';

/**
 * Every route renders, at every size — and the axiom holds on the surface you
 * land on.
 *
 * The direct descendant of the TUI's `every_screen_draws_at_every_size`, and it
 * exists for the same reason: *"a layout panic wrecks the user's tmux pane."* Here
 * it is a white screen with a console error nobody is reading, which is worse
 * because nothing crashes.
 *
 * There is no Tauri IPC on the dev server, so these run against `FakeClient` —
 * whose books are the hostile set from `corpus gen-devdb`: a null title, a
 * 220-character one, an RTL one, CJK, no author, `page_count` of zero, an
 * abandoned reading. Those are the inputs, on purpose. A suite that rendered
 * twenty ordinary books would pass on the day the long title broke the grid.
 */

/** The routes this app has. Add one here when you add one there. */
const ROUTES = [
  { name: 'library', path: '/' },
  // Ids from `FakeClient`'s hostile set, each chosen for a different failure.
  { name: 'book-doorstop', path: '/book/3' }, // highlights, notes, a huge page count
  { name: 'book-long-title', path: '/book/6' }, // a title with no room anywhere
  { name: 'book-rtl', path: '/book/15' }, // bidi, in a left-aligned grid
  { name: 'book-untitled', path: '/book/17' }, // title is null
  { name: 'book-abandoned', path: '/book/11' }, // must not read as failure
  { name: 'book-reread', path: '/book/12' }, // two readings
  { name: 'book-missing', path: '/book/9999' }, // no such book: a page, not a dead end
];

for (const route of ROUTES) {
  test(`${route.name} renders`, async ({ page }) => {
    const problems: string[] = [];
    page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
    page.on('pageerror', (e) => problems.push(e.message));

    await page.goto(route.path);
    // The screens fetch in an `$effect`, so "rendered" means the placeholder is
    // gone — not that the document loaded.
    await expect(page.locator('main')).not.toContainText(/Reading the shelf…|Opening…/);

    // Nothing is a dead end: every screen shows a next move. On the library that
    // is a book or the empty state's commands; everywhere else it is the way back.
    const links = await page.locator('a').count();
    expect(links, 'a screen with no links out is a dead end').toBeGreaterThan(0);

    expect(problems, 'the console must be clean').toEqual([]);
    await expect(page).toHaveScreenshot(`${route.name}.png`, { fullPage: true });
  });
}

/**
 * The axiom, asserted rather than reviewed.
 *
 * `gui/CLAUDE.md`: *"No number on a home surface. Ever."* The TUI asserts this
 * against its own drawn buffer (`the_home_screen_greets_you_with_no_numbers`);
 * until now the GUI had only the `screenshot-reviewer` agent, which is judgement
 * rather than a gate. This is the gate.
 */
test('the library surface greets you with no numbers', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');

  // Book titles legitimately contain digits, so this looks at the chrome — the
  // header and everything in `main` that is not a tile.
  const chrome = await page.locator('header').innerText();
  expect(chrome).not.toMatch(/\d/);

  const heading = await page.locator('main h1').innerText();
  expect(heading, 'a count beside the heading is the framing decisions.md bans').not.toMatch(/\d/);

  // And none of the words that carry completion framing, wherever they appear.
  const all = await page.locator('body').innerText();
  for (const banned of [/\bunread\b/i, /\bstreak\b/i, /\bgoal\b/i, /\bto[- ]read\b/i, /\bremaining\b/i]) {
    expect(all, `"${banned}" is task-completion framing`).not.toMatch(banned);
  }
});

/**
 * Putting a book down is not failure, and is not styled as one.
 *
 * This is the rule `reading_status` had to cross the API for: before it,
 * *abandoned* and *reading* were both `finished: false` with a `current_page` and
 * no frontend could tell them apart to honour this at all.
 */
test('an abandoned book is not styled as a failure', async ({ page }) => {
  await page.goto('/book/11');
  const text = await page.locator('main').innerText();
  expect(text).toContain('Put down');
  for (const banned of [/\bfail/i, /\bdid not finish\b/i, /\bdnf\b/i, /\bgave up\b/i]) {
    expect(text).not.toMatch(banned);
  }
});
