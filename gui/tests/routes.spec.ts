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
  // No cover and no measurement — the hero's third state, the hatch. The plate
  // is on every other book row, so without this one the branch that says "never
  // measured" rather than "this jacket is grey" is rendered nowhere.
  { name: 'book-no-cover', path: '/book/5' },
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
 * The shelf's arrangements — every one of them, rendered.
 *
 * Item 26 shipped the cover grid and deferred the spine shelf, so the layout is
 * a **seam** rather than a shape (`src/lib/shelf/layouts.ts`). The failure that
 * seam invites is a second arrangement that compiles, is never looked at, and
 * rots — which is this repo's standing complaint about a guard that cannot
 * fail, one layer up. So the alternate layout is screenshotted like a route.
 *
 * It also drives the switch rather than seeding `localStorage`, which makes the
 * persistence real: pick, reload, and the shelf is still how you left it. State
 * persists and is visible is the axiom's first clause, and this is it asserted.
 */
test('the shelf renders in the arrangement you pick, and remembers it', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/');
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');

  const list = page.getByRole('button', { name: 'List' });
  await list.click();
  await expect(list).toHaveAttribute('aria-pressed', 'true');
  await expect(page).toHaveScreenshot('library-list.png', { fullPage: true });

  // The preference survives the window closing, which is the only thing that
  // makes it a preference rather than a toggle.
  await page.reload();
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');
  await expect(page.getByRole('button', { name: 'List' })).toHaveAttribute('aria-pressed', 'true');

  expect(problems, 'the console must be clean').toEqual([]);
});

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

/**
 * The book view's deeper states, driven rather than described (item 27).
 *
 * A route screenshot only ever sees a page's first frame, so the two things
 * this item is actually about — a note open in place, and its links pane
 * replacing it — would have been reviewed by nobody. The shelf's arrangement
 * test set the precedent: drive the thing, then take the picture.
 *
 * The row's accessible name carries its anchor, which is what tells the two
 * notes with *The Doorstop* in their titles apart without a test id.
 */
test('a note opens in place, and its links replace the note', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  await page.getByRole('button', { name: 'p. 212 On The Doorstop' }).click();

  // The body is the note's own markdown, not a rendering of it: the file in the
  // vault is the origin and Obsidian is the other thing editing it.
  // `toHaveValue`, not `toContainText`: a textarea's text node is its *initial*
  // markup and `bind:value` sets the property, so a content assertion here
  // passes on an empty box and fails on a full one.
  await expect(page.getByRole('textbox', { name: 'Note body' })).toHaveValue(/Two hundred pages/);

  // Citing is the gesture the mouse makes available, and it needs a note to
  // cite *into* — so the control exists now and did not before the click.
  await expect(page.getByRole('button', { name: /Cited in/ })).toHaveCount(1);
  await expect(page.getByRole('button', { name: /Cite into/ })).toHaveCount(2);
  await expect(page).toHaveScreenshot('book-note-open.png', { fullPage: true });

  // Cite a second passage. Round trip through the pane, not through the fake.
  await page.getByRole('button', { name: /Cite into/ }).first().click();
  await expect(page.getByRole('button', { name: /Cited in/ })).toHaveCount(2);

  await page.getByRole('button', { name: 'Links', exact: true }).click();
  // Counted per direction, both past tense. Nothing counts a link not written.
  await expect(page.locator('main')).toContainText('2 out · 1 in');
  // A wikilink naming a note nobody has written is kept as text, and says so.
  // It is a forward reference, and it resolves itself the day that note exists.
  await expect(page.locator('main')).toContainText('The Long Eighteenth Century');
  await expect(page.locator('main')).toContainText('no note yet');
  await expect(page).toHaveScreenshot('book-note-links.png', { fullPage: true });

  // Nothing is a dead end, at any of the three depths: the way back is in the
  // page rather than in a dialog that has to be dismissed.
  await page.getByRole('button', { name: '‹ Note' }).click();
  await page.getByRole('button', { name: '‹ Notes' }).click();
  await expect(page.getByRole('button', { name: 'Write a note' })).toBeVisible();

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * A passage carries two notes with two owners, and says which is which.
 *
 * `ko_note` is KOReader's and is rewritten toward the device on every import;
 * `annotation` is the reader's and no import has ever touched it. That is a
 * whole section of `docs/decisions.md` and nothing had ever drawn the pair —
 * unlabelled they are two grey paragraphs, and this is what stops them becoming
 * two grey paragraphs again.
 */
test('a passage says who wrote what against it', async ({ page }) => {
  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  const passage = page.locator('li').filter({ hasText: 'The thing about a place' });
  await expect(passage).toContainText('KOReader');
  await expect(passage).toContainText('You');
});

/**
 * Counts are allowed on a page you chose to open. Counting what is *undone* is
 * not, anywhere, ever — `docs/decisions.md` forbids it by name.
 */
test('the book view counts what you did and never what you have left', async ({ page }) => {
  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  const all = await page.locator('body').innerText();
  for (const banned of [
    /\bunread\b/i,
    /\bstreak\b/i,
    /\bgoal\b/i,
    /\bto[- ]read\b/i,
    /\bremaining\b/i,
    /\buncited\b/i,
    /\bnot yet cited\b/i,
  ]) {
    expect(all, `"${banned}" is task-completion framing`).not.toMatch(banned);
  }
});
