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
  // Item 49. Words already taken off a passage — two off one, which is the only
  // way the plural renders, plus a card anchored to nothing that must appear
  // against no passage at all. Book 3 carries the other half: a passage that is
  // both quoted and captured from, which is where the two marks would collide
  // if they had been drawn as one thing.
  { name: 'book-captured', path: '/book/20' },
  // Item 28. The card is per reading, so book 12 is the one that matters: two
  // cards side by side is the comparison the whole object exists for, and it is
  // also the only width at which the wall's grid has to do anything.
  { name: 'cards-reread', path: '/book/12/cards' },
  // One reading, one card — the ordinary case, and the one where a wall laid out
  // for two would leave a card looking like it lost its sibling.
  { name: 'cards-one-read', path: '/book/3/cards' },
  // A read whose marks are all unattributed: no card passage, drawn as an
  // absence rather than as an empty box or an error.
  { name: 'cards-no-passage', path: '/book/11/cards' },
  // A book nobody has read. Idle is not blank: the empty state names the moves.
  { name: 'cards-none', path: '/book/4/cards' },
  // Item 47. The wall across the library — every card, a page at a time, with
  // the year picker and the three orders above it. It is also the only screen
  // where a card with a read ordinal sits beside one without.
  { name: 'cards-wall', path: '/cards' },
  // The vault as a place. Its first frame is *Recently written* plus an empty
  // preview column, which is the state a reader lands on and the one the two
  // panes' widths have to look right in.
  { name: 'notes', path: '/notes' },
  { name: 'life', path: '/life' },
  // Item 54. The book you are reading, with the window to itself — the first
  // route outside `(shell)/`, so it is also the only screenshot in the suite
  // that proves the header is genuinely absent rather than merely unstyled.
  { name: 'reading', path: '/reading' },
  // A `?book=` naming a book that is not open. It must land on an open read
  // rather than on an error, and this is the only place that branch renders.
  { name: 'reading-stale-book', path: '/reading?book=9999' },
  // Item 55. The fifth place, and the fixture behind it is four readers chosen
  // for four different failures: an ordinary one, a name long enough to break a
  // card laid out for `Kindle`, a volume with nothing of ours on it (the
  // install flow, whose destination path must be on screen before it writes),
  // and one readingbuddy refuses to write to because a file of ours was edited
  // on the device. Plus a reader in a bag with no name at all.
  { name: 'devices', path: '/devices' },
];

for (const route of ROUTES) {
  test(`${route.name} renders`, async ({ page }) => {
    const problems: string[] = [];
    page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
    page.on('pageerror', (e) => problems.push(e.message));

    await page.goto(route.path);
    // The screens fetch in an `$effect`, so "rendered" means the placeholder is
    // gone — not that the document loaded.
    await expect(page.locator('main')).not.toContainText(
      /Reading the shelf…|Opening…|Reading the log…|Looking for readers…/,
    );

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

  // The wall opens grouped by the year a reading closed, which is the default
  // and the one arrangement that shows a book with no reading at all.
  await expect(page.getByRole('button', { name: 'Year' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('main')).toContainText('No reading recorded');

  const byAuthor = page.getByRole('button', { name: 'Author' });
  await byAuthor.click();
  await expect(byAuthor).toHaveAttribute('aria-pressed', 'true');
  // The finding this arrangement carries: a book with no reading has no answer
  // to "whose work have I read", so it is not on the wall under Author — and
  // its absence is what stops the wall becoming a mixed field of read and
  // unread, which is the backlog rendering arriving with no label changing.
  await expect(page.locator('main')).not.toContainText('No reading recorded');
  await expect(page).toHaveScreenshot('library-by-author.png', { fullPage: true });

  // The preference survives the window closing, which is the only thing that
  // makes it a preference rather than a toggle.
  await page.reload();
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');
  await expect(page.getByRole('button', { name: 'Author' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * The band above the wall promotes four books and says nothing about the rest.
 *
 * A "continue reading" shelf is populated by starting and drained only by
 * finishing, so its steady state is a queue of things abandoned — and an
 * uncapped one puts that arithmetic on the highest-salience region of the home
 * surface, with no number anywhere for a test to catch. This is the cap, and the
 * silence around it, asserted.
 */
test('the reading band promotes four books and counts none of them', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');

  const band = page.locator('section').filter({ hasText: 'Reading now' }).first();
  const previews = band.locator('article');
  expect(await previews.count(), 'four is the cap, and it is silent').toBeLessThanOrEqual(4);

  // No "and 3 others", no total, nothing counting the books it did not promote.
  const text = await band.innerText();
  for (const banned of [/\band \d+ (more|others)\b/i, /\b\d+ books?\b/i, /\bin progress\b/i]) {
    expect(text, `"${banned}" would be a count of what was left out`).not.toMatch(banned);
  }
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

  // `textContent`, not `innerText`: the library's heading is in the document for
  // the outline and for a screen reader and is not drawn — the shell's nav is
  // what says where you are — and `innerText` reads what is rendered.
  const heading = (await page.locator('main h1').textContent()) ?? '';
  expect(heading, 'a count beside the heading is the framing decisions.md bans').not.toMatch(/\d/);

  // And none of the words that carry completion framing, wherever they appear.
  //
  // `yet` joined the list with item 52, and it is the one word here that this
  // assertion **cannot** be trusted to find: six of the seven it was added for
  // lived in empty states, and this fixture is a library with books in it, so
  // the branch that says them never renders. `src/lib/axiom.test.ts` scans the
  // markup for exactly that reason. This row guards the surfaces that do draw.
  const all = await page.locator('body').innerText();
  for (const banned of [
    /\bunread\b/i,
    /\bstreak\b/i,
    /\bgoal\b/i,
    /\bto[- ]read\b/i,
    /\bremaining\b/i,
    /\byet\b/i,
  ]) {
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
test('a note opens on the work surface, with its links beside it', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  // The note list is the left rail and it is there while you are anywhere on
  // this page — including while writing, which is when it used to disappear.
  // `exact`, because the cite control on a passage quotes the note's title.
  await page.getByRole('button', { name: 'On The Doorstop', exact: true }).click();

  // The body is the note's own markdown, not a rendering of it: the file in the
  // vault is the origin and Obsidian is the other thing editing it.
  // `toHaveValue`, not `toContainText`: a textarea's text node is its *initial*
  // markup and `bind:value` sets the property, so a content assertion here
  // passes on an empty box and fails on a full one.
  await expect(page.getByRole('textbox', { name: 'Note body' })).toHaveValue(/Two hundred pages/);

  // The save bar names the file. Naming it is the app saying it did not capture
  // your writing — the vault is markdown on disk and this is where that shows.
  await expect(page.locator('main')).toContainText('in your vault as');

  // **The links are a region, not a depth.** They used to be behind a button
  // that replaced the note; with a third column there is nothing to trade off,
  // and a graph you can see while writing into it is a different tool from one
  // you have to go and look at. Counted per direction, both past tense.
  await expect(page.locator('main')).toContainText('2 out · 1 in');
  // A wikilink naming a note nobody has written is kept as text, and says so.
  // It is a forward reference, and it resolves itself the day that note exists.
  await expect(page.locator('main')).toContainText('The Long Eighteenth Century');
  // *no note*, not *no note yet* — item 52 cut the word that turns a note
  // nobody has written into a note somebody owes.
  await expect(page.locator('main')).toContainText('no note');

  await expect(page).toHaveScreenshot('book-note-open.png', { fullPage: true });

  // **The note stays open while the passages are shown**, which is what makes
  // citing possible at all: `Cite` needs a note to cite *into* and a passage to
  // cite *from*, and only one of them can be the work surface. The rail keeps the
  // note marked as current; the centre goes back to the list.
  await page.getByRole('button', { name: 'Passages', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'On The Doorstop', exact: true }),
  ).toHaveAttribute('aria-current', 'true');
  await expect(page.getByRole('button', { name: /Cited in/ })).toHaveCount(1);
  await expect(page.getByRole('button', { name: /Cite into/ })).toHaveCount(2);
  await page
    .getByRole('button', { name: /Cite into/ })
    .first()
    .click();
  await expect(page.getByRole('button', { name: /Cited in/ })).toHaveCount(2);

  // Nothing is a dead end and nothing is modal: every other destination is on
  // screen while a note is open, so leaving one is a move rather than a
  // dismissal — and going back to it is one click on a row that never left.
  await expect(page.getByRole('textbox', { name: 'Note body' })).toHaveCount(0);
  await page.getByRole('button', { name: 'On The Doorstop', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Note body' })).toHaveCount(1);

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * The passage list is one tab stop, not one per control.
 *
 * `opacity: 0` does not remove anything from the tab order, so a hover-revealed
 * control on forty passages would be a hundred and twenty stops on invisible
 * buttons — SC 2.4.7 failing in substance. The fix is the list: it contributes
 * one stop, arrow keys move within it, and only the active passage's own
 * controls are tabbable. **Tab-stop count is the real ergonomic metric of a
 * keyboard interface and almost nobody measures it**, so this measures it.
 */
test('the passage list is a composite widget, not a hundred tab stops', async ({ page }) => {
  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  const passages = page.locator('.passages > li');
  await expect(passages).toHaveCount(3);

  // Exactly one row is in the document's tab sequence at a time.
  await expect(page.locator('.passages > li[tabindex="0"]')).toHaveCount(1);
  await expect(page.locator('.passages > li[tabindex="-1"]')).toHaveCount(2);

  // And the controls on the rows that are *not* active are out of it too, which
  // is the half that stops the hidden buttons being stops of their own.
  const dormant = passages.nth(1).locator('button');
  expect(await dormant.count(), 'the fixture must have a control to check').toBeGreaterThan(0);
  for (const b of await dormant.all()) {
    await expect(b).toHaveAttribute('tabindex', '-1');
  }

  // Arrow keys move between passages; the active row follows focus.
  await passages.first().focus();
  await page.keyboard.press('ArrowDown');
  await expect(passages.nth(1)).toBeFocused();
  await page.keyboard.press('End');
  await expect(passages.nth(2)).toBeFocused();
  await page.keyboard.press('Home');
  await expect(passages.first()).toBeFocused();
});

/**
 * The search over one book's marks, driven (item 50).
 *
 * Typed rather than described, for the reason the note-pane test above exists:
 * every state worth reviewing here is behind an input, so a route screenshot
 * would have shown an empty box and nothing else. What it pins is the three
 * claims the item makes — one list holding both kinds, a hit that is a move
 * into the page rather than a fourth place to read something, and an absence
 * stated in words with no number anywhere near it.
 */
test('a search over one book answers with notes and passages together', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  const box = page.getByRole('searchbox', { name: 'Search this book' });
  await box.fill('the');

  // Both kinds, in one list. The counts are the fixture's; what matters is that
  // neither is zero, because a list that could only ever hold one kind is the
  // notes-only search item 34 deleted.
  const hits = page.locator('.search li button');
  await expect(hits.filter({ hasText: 'Passage' }).first()).toBeVisible();
  await expect(hits.filter({ hasText: 'Note' }).first()).toBeVisible();
  await expect(page).toHaveScreenshot('book-search.png', { fullPage: true });

  // A passage hit takes the reader to the passage in the centre column and marks
  // it — it does not filter the list down to one row, which would throw away
  // where the passage sits.
  await hits.filter({ hasText: 'Passage' }).first().click();
  await expect(page.locator('li.found')).toHaveCount(1);
  await expect(page.locator('.passages > li')).toHaveCount(3);

  // Nothing matched is a sentence, not a zero. No digit may appear in it.
  await box.fill('zzzznothing');
  const none = page.locator('.search p');
  await expect(none).toContainText('Nothing here matches');
  expect(await none.innerText()).not.toMatch(/\d/);

  // And clearing the box puts the page back rather than leaving the last answer
  // under an empty question.
  await box.fill('');
  await expect(page.locator('.search li')).toHaveCount(0);
  await expect(page.locator('.search p')).toHaveCount(0);

  // A note hit opens the note on the work surface. It is asserted last because
  // the box lives in the right rail, and the right rail is an **inspector**:
  // with a note open it shows that note's connections instead, which is the
  // whole reason the column can be permanent.
  await box.fill('the');
  await hits.filter({ hasText: 'Note' }).first().click();
  await expect(page.getByRole('textbox', { name: 'Note body' })).toHaveCount(1);

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

// ---------------------------------------------------------------------------
// Items 48 and 49 — the cited mark, and the capture.
// ---------------------------------------------------------------------------

/**
 * *A note quotes this* and *I am citing this into the note I have open* are two
 * different facts, and the item is judged on their being drawn apart.
 *
 * The mark comes from one `CitationsForNotes` over the page of notes the route
 * already loaded; the toggle comes from `CitationsFor` for the one open note.
 * Collapsing them into one visual is the failure `docs/prompts/48-49` names,
 * and it is the kind that passes every other assertion here — the screen still
 * renders, the console is still clean, and the reader can no longer tell which
 * of two things they are looking at.
 */
test('a quoted passage says so, and that is not the cite toggle', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/book/3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  // Three passages, three states. `byThisNote` is quoted by note 1, which is
  // the one about to be opened; `byAnother` is quoted by note 3, which is not;
  // `plain` is quoted by nobody.
  const byThisNote = page.locator('li').filter({ hasText: 'What survives is not what' });
  const byAnother = page.locator('li').filter({ hasText: 'She counted the bells' });
  const plain = page.locator('li').filter({ hasText: 'The thing about a place' });
  await expect(byThisNote).toContainText('Quoted in a note');
  await expect(byAnother).toContainText('Quoted in a note');
  await expect(plain).not.toContainText('Quoted in a note');

  // With no note open there is no toggle at all, so the mark is the only thing
  // on screen saying anything about citations — which is the state that proves
  // it is not the toggle wearing a different colour.
  await expect(page.getByRole('button', { name: /Cite into|Cited in/ })).toHaveCount(0);

  await page.getByRole('button', { name: 'On The Doorstop', exact: true }).click();
  await page.getByRole('button', { name: 'Passages', exact: true }).click();
  await expect(page.getByRole('button', { name: /Cited in/ })).toHaveCount(1);
  // Both facts on one passage at once — the case where they could be mistaken
  // for one thing…
  await expect(byThisNote.getByRole('button', { name: /Cited in/ })).toHaveCount(1);
  await expect(byThisNote).toContainText('Quoted in a note');
  // …and the case that pays for the mark existing: quoted, by a note this one
  // is not, so the button offers to cite it and the mark still says somebody
  // has. A screen where these two never appear apart cannot show the
  // difference, which is why `fake.ts` states a second citing note.
  await expect(byAnother.getByRole('button', { name: /Cite into/ })).toHaveCount(1);
  await expect(byAnother).toContainText('Quoted in a note');

  // And the mark moves with the toggle. Citing the unquoted passage marks it;
  // unciting the one only note 1 quotes unmarks it. That is the batch's row
  // being corrected from the single-note reply rather than the page re-asking
  // for the whole page of notes on every click.
  await plain.getByRole('button', { name: /Cite into/ }).click();
  await expect(plain).toContainText('Quoted in a note');
  await byThisNote.getByRole('button', { name: /Cited in/ }).click();
  await expect(byThisNote).not.toContainText('Quoted in a note');
  // Unciting one note does not unmark what another quotes.
  await expect(byAnother).toContainText('Quoted in a note');

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * A card is made from a passage, and the two answers are told apart.
 *
 * `CreateFlashcard` answers a bool where `true` is *created* and `false` is
 * *you already had this*, and `UNIQUE(book_id, word)` leaves the existing card
 * exactly as it was. A frontend rendering both as "saved" throws away the only
 * thing the write answers — and rendering the second as an error would make
 * having already done something a failure, which the axiom forbids by name.
 *
 * The selection is driven rather than described: the box arrives holding what
 * the reader selected inside *that* passage, and arrives empty when there is no
 * selection. Both paths end in the same write.
 */
test('a word can be taken off a passage, and the second time it says so', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/book/20');
  await expect(page.locator('main')).not.toContainText('Opening…');

  // What was already taken, past tense, and pluralised by how many there are.
  const shelf = page.locator('li').filter({ hasText: 'A shelf is an argument' });
  await expect(shelf).toContainText('You kept “argument” and “intends”');
  // The card anchored to nothing belongs to no passage and is drawn against
  // none — a band that guessed one would hang it off whichever came first. The
  // whole set of record lines is asserted rather than one absence, because that
  // is the direction a stray card would arrive from.
  const records = page.locator('p').filter({ hasText: /^You kept / });
  expect(await records.allInnerTexts()).toEqual(['You kept “argument” and “intends”']);

  const filing = page.locator('li').filter({ hasText: 'Everything else is filing' });
  await expect(filing).not.toContainText('You kept');

  // Nothing selected: an empty box, focused. The fallback is a real path and
  // not a degradation — a reader who wants a word that is not on the page
  // types it.
  await filing.getByRole('button', { name: 'Make a card' }).click();
  const word = filing.getByRole('textbox');
  await expect(word).toHaveValue('');
  await expect(word).toBeFocused();
  await expect(page).toHaveScreenshot('book-capture-open.png', { fullPage: true });
  await filing.getByRole('button', { name: 'Cancel' }).click();

  // A selection inside the passage fills the box.
  //
  // A **drag**, not a double-click: headless WebKit does not do word-selection
  // on `dblclick` under Playwright at all (it leaves the selection collapsed
  // with one range), so a double-click here would be asserting the driver
  // rather than the app. A drag is also the gesture this control was put on the
  // passage for.
  //
  // The distance decides which words are caught, so the assertion is that the
  // box holds *the passage's own text* rather than a word the layout picks.
  const line = await filing.locator('blockquote').boundingBox();
  if (line === null) throw new Error('the passage has no box to drag across');
  await page.mouse.move(line.x + 2, line.y + line.height / 2);
  await page.mouse.down();
  await page.mouse.move(line.x + 70, line.y + line.height / 2, { steps: 10 });
  await page.mouse.up();

  await filing.getByRole('button', { name: 'Make a card' }).click();
  const picked = await word.inputValue();
  // `preventDefault` on the button's **mousedown** is what makes this non-empty:
  // pressing a button outside the selection collapses it before `click` fires,
  // so without that line the box would arrive empty every time and the prefill
  // would be dead code nothing noticed.
  expect(picked.length, 'the selection fills the box').toBeGreaterThan(0);
  expect('Everything else is filing.').toContain(picked);

  await word.fill('filing');
  await filing.getByRole('button', { name: 'Keep' }).click();
  await expect(filing).toContainText('Kept.');
  // The record is re-read from the library rather than synthesized from what
  // was sent — `CreateFlashcard` answers a bool and no card.
  await expect(filing).toContainText('You kept “filing”');

  // The same word again. Not an error, not styled as one, and the card that is
  // there is unchanged.
  await filing.getByRole('button', { name: 'Make a card' }).click();
  await word.fill('filing');
  await filing.getByRole('button', { name: 'Keep' }).click();
  await expect(filing).toContainText('You already had that one');
  await expect(filing).toContainText('You kept “filing”');
  const text = await filing.innerText();
  for (const banned of [/\bfail/i, /\berror\b/i, /\balready exists\b/i]) {
    expect(text, 'having already done something is not a failure').not.toMatch(banned);
  }

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * Neither control counts anything, and neither offers a dialog.
 *
 * The passages band is where two new controls could quietly reintroduce both
 * things the axiom forbids: a tally of the passages you have *not* captured
 * from, and a modal to capture in. `role=dialog` is the assertion for the
 * second because it is what a modal is, whatever it looks like.
 */
test('the passage controls count nothing and open nothing', async ({ page }) => {
  await page.goto('/book/20');
  await expect(page.locator('main')).not.toContainText('Opening…');

  await page.getByRole('button', { name: 'Make a card' }).first().click();
  await expect(page.getByRole('dialog')).toHaveCount(0);

  const all = await page.locator('body').innerText();
  for (const banned of [
    /\buncaptured\b/i,
    /\bnot yet\b/i,
    /\bremaining\b/i,
    // A literal space, **not** `\s`: `\s` crosses the newline between a
    // passage's `p. 44` and its `Cards:` line, so the broad spelling of this
    // banned a page number sitting above a record of what you did. The thing
    // being forbidden is "3 cards" on one line, and that is what it says now.
    /\d+ cards?\b/i,
    /\bcards? left\b/i,
  ]) {
    expect(all, `"${banned}" is task-completion framing`).not.toMatch(banned);
  }
});

// ---------------------------------------------------------------------------
// Item 28 — the chain, and the reading-life page.
// ---------------------------------------------------------------------------

/**
 * The moment ends with a cursor in the reflection, not in a dismissable dialog.
 *
 * `gui-vision.md:121` is the whole assertion: *"a moment that ended in a
 * dismissable dialog would be a task-completion popup wearing a costume. A
 * moment that ends with a cursor in an empty reflection is the app doing what it
 * exists for."* A screenshot of the shelf shows the band exists; only driving it
 * shows where it goes.
 */
test('a moment ends by opening the reflection, and offers no way to dismiss it', async ({
  page,
}) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/');
  const moment = page.getByRole('button', { name: 'Write what you thought' });
  await expect(moment).toBeVisible();

  // No close, no dismiss, no "later", no × — the moment ends by being acted on
  // or by being read and left alone. A dismiss control is the popup this refuses.
  const band = page.locator('section').filter({ hasText: 'You finished' });
  for (const banned of ['Dismiss', 'Close', 'Later', 'Not now', 'Got it', '×']) {
    await expect(band.getByRole('button', { name: banned })).toHaveCount(0);
  }

  await moment.click();

  // It lands on the book with the reflection **open in the note pane** — item
  // 27's editor, reused, rather than a second one built for the ceremony.
  await expect(page).toHaveURL(/\/book\/12\?note=\d+/);
  await expect(page.getByRole('textbox', { name: 'Note body' })).toBeVisible();
  await expect(page.locator('main')).toContainText('Reflection');

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * The moment fires once, and nothing anywhere counts how many are waiting.
 *
 * `surfaced_at` means *shown*, so it is written when the band renders. There is
 * no count on the wire — `the_wire_states_no_number_of_moments` asserts that
 * absence in the engine deliberately — and a `3` beside a ceremony here would
 * put the badge back one layer up.
 */
test('a moment is shown once and is never counted', async ({ page }) => {
  await page.goto('/');
  const band = page.locator('section').filter({ hasText: 'You finished' });
  await expect(band).toBeVisible();
  const first = await band.innerText();
  expect(first).not.toMatch(/\d+\s*(more|others|waiting|pending|new)\b/i);
  expect(first).not.toMatch(/\b(pending|waiting|unseen|inbox)\b/i);

  // Leave and come back **within the app**, which is both halves of the design:
  // the shelf remounts and polls again, so a write made elsewhere is caught up
  // with — and the moment already surfaced does not come back, because
  // acknowledging happens when it is shown.
  //
  // A browser reload would rebuild `FakeClient` and prove nothing here; the real
  // client's acknowledgement is a row in SQLite, and layer 2 has no database.
  // Through the shell's nav, which is now the way back from every place — the
  // per-page "← Library" was a second door to the same room and the pages that
  // are *in* the nav do not need one. A leaf still has its own: the book page
  // is not a place the nav names.
  await page.getByRole('link', { name: 'Reading life' }).click();
  await expect(page.locator('main')).not.toContainText('Reading the log…');
  await page.getByRole('link', { name: 'Library' }).click();
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');

  await expect(page.locator('section').filter({ hasText: 'You finished' })).toHaveCount(0);
  // The next one, not the same one — and still exactly one at a time.
  await expect(page.locator('main')).toContainText(
    'You marked your first passage in The Doorstop.',
  );
});

/**
 * The card shows the passage the **engine** chose, not `highlights[0]`.
 *
 * Item 44 put the choice below the seam because which passage a card carries is
 * a selection predicate, and the day the TUI grows a card the two apps would
 * otherwise show a different sentence for the same reading with neither looking
 * wrong. The fixture states the engine's answer and makes it differ from the
 * first mark of each read, so a component reaching for `[0]` renders visibly
 * different text — which is what this pins.
 */
test('two readings of one book carry two different passages', async ({ page }) => {
  await page.goto('/book/12/cards');
  await expect(page.locator('main')).not.toContainText('Opening…');

  const cards = page.locator('article');
  await expect(cards).toHaveCount(2);

  // The engine's choice for each read — neither of them that read's first mark.
  await expect(page.locator('main')).toContainText('it is a description of a marriage');
  await expect(page.locator('main')).toContainText('the house is not a metaphor at all');
  // The first mark of each read, which `highlights[0]` would have shown.
  await expect(page.locator('main')).not.toContainText('She said it plainly');
  await expect(page.locator('main')).not.toContainText('A door is a decision');
  // And the longest mark on the *book*, which belongs to neither read. A
  // selection over `book_id` would have put it on both cards.
  await expect(page.locator('main')).not.toContainText('on some pass or other');

  // The comparison the card exists for: two ratings, one per read.
  await expect(page.locator('main')).toContainText('3 / 5');
  await expect(page.locator('main')).toContainText('4.5 / 5');
});

/**
 * A card names its read by the **engine's** ordinal, and only when there are two.
 *
 * This test used to forbid an ordinal outright, and the prohibition was right
 * while it lasted: `readings.indexOf(id) + 1` re-implements a domain rule *and*
 * silently re-acquires a dependency on `list_readings`' undocumented ordering,
 * with nothing on the screen looking wrong. Item 41 put `read_number` and
 * `of_reads` on the row, so the rule is now read rather than reinvented — and a
 * stale prohibition is worse than none, because the next thread obeys it.
 *
 * What is pinned instead is the half that can still go wrong: the ordinal
 * appears exactly where `of_reads > 1`, which is the same test the TUI's gutter
 * makes. A frontend counting off a list would caption the single-read page too.
 */
test('a card names its read by the engine’s number, and only on a reread', async ({ page }) => {
  await page.goto('/book/12/cards');
  await expect(page.locator('main')).not.toContainText('Opening…');
  const reread = await page.locator('main').innerText();
  expect(reread).toContain('Your first read');
  expect(reread).toContain('Your second read');
  // Oldest first: the order a reading life happened in, and the order the
  // side-by-side comparison reads in. It comes from `read_number` itself.
  expect(reread.indexOf('Your first read')).toBeLessThan(reread.indexOf('Your second read'));

  // One read is one read. `ReadCount::ordinal` is "a lone read has no number",
  // so this page must caption nothing at all.
  await page.goto('/book/3/cards');
  await expect(page.locator('main')).not.toContainText('Opening…');
  const once = await page.locator('main').innerText();
  expect(once, 'a book read once gets no ordinal').not.toMatch(/\byour \w+ read\b/i);
  // And never a raw one, whatever the source. `Read #2` is the shape a frontend
  // reaches for when it has an index rather than a fact.
  expect(once).not.toMatch(/read #\d/i);
});

// ---------------------------------------------------------------------------
// Item 47 — the wall of cards.
// ---------------------------------------------------------------------------

/**
 * The wall filters by year, and a year it has nothing for is not a failure.
 *
 * The year picker is the item's own subject and a route screenshot only sees a
 * page's first frame, so the two states that matter — a year selected, and a
 * year that matched nothing — would have been reviewed by nobody. The shelf's
 * arrangement test set the precedent: drive the thing, then take the picture.
 *
 * **2023 is the assertion item 51 turned around.** It is in the fixture's
 * activity log with no read closed in it, and under the old `activityByMonth`
 * proxy the picker offered it — this test used to click it and check that the
 * empty wall did not apologise. The years now come from `readings.finished_at`,
 * so a year with no closed read is not offered at all, and the *absence of the
 * pill* is what proves the proxy is gone.
 */
test('the wall filters by year, and offers no year it has nothing for', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/cards');
  await expect(page.locator('main')).not.toContainText('Reading the wall…');

  const y2024 = page.getByRole('button', { name: '2024', exact: true });
  await y2024.click();
  await expect(y2024).toHaveAttribute('aria-pressed', 'true');
  // The reread's first read closed in 2024, and its ordinal is a fact about the
  // book rather than about the page — item 43's correction, on a screen: this
  // filter hides the second read and the first is still the first.
  await expect(page.locator('main')).toContainText('Your first read');
  await expect(page.locator('main')).not.toContainText('Your second read');
  await expect(page).toHaveScreenshot('cards-wall-year.png', { fullPage: true });

  // The ghost year. It has notes and highlights in the activity log and no read
  // that ended, so it is not a year of cards and is not offered as one.
  await expect(page.getByRole('button', { name: '2023', exact: true })).toHaveCount(0);

  // Every year that *is* offered has cards behind it — the picker and the wall
  // agree by construction now, and this is that claim on a screen.
  const pills = page.getByRole('navigation', { name: 'Which cards' }).getByRole('button');
  for (const label of await pills.allInnerTexts()) {
    if (!/^\d{4}$/.test(label)) continue;
    await page.getByRole('button', { name: label, exact: true }).click();
    await expect(page.locator('main'), `${label} was offered and holds nothing`).not.toContainText(
      `No cards from ${label}`,
    );
  }

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * The read you are in the middle of is reachable, and is not a year (item 51).
 *
 * An open reading has no `finished_at`, so it belongs to no year — and before
 * this chip existed those cards were reachable from *All* and from nowhere
 * else, so a reader visiting every year in turn never saw the book they were
 * reading. `ReadingYearsDto.open` is what says the chip exists, and it is a
 * bool: nothing here may say how many.
 */
test('the wall reaches the read you have not finished, without counting it', async ({ page }) => {
  await page.goto('/cards');
  await expect(page.locator('main')).not.toContainText('Reading the wall…');

  const chip = page.getByRole('button', { name: 'Still reading' });
  await expect(chip).toHaveCount(1);
  // The control names a state, not a number and not a shortfall.
  const controls = await page.getByRole('navigation', { name: 'Which cards' }).innerText();
  expect(controls, 'a figure on the chip is the badge the axiom bans').not.toMatch(
    /still reading\s*\(?\d/i,
  );

  await chip.click();
  await expect(chip).toHaveAttribute('aria-pressed', 'true');
  // Every card under it is a read that has not ended.
  const cards = page.locator('article');
  await expect(cards.first()).toBeVisible();
  await expect(page).toHaveScreenshot('cards-wall-open.png', { fullPage: true });
});

/**
 * The wall may count, and the shelf may not.
 *
 * `/cards` is a page you chose to open, like `/life`, so a count of the readings
 * a filter matched is legitimate here. Two conditions, and both are asserted:
 * it is phrased as a **total** rather than as a portion of one — *showing 24 of
 * 400* is a progress bar through your own library — and it does not leak onto
 * the home surface.
 */
test('the wall counts what you read and never what is left of it', async ({ page }) => {
  await page.goto('/cards');
  await expect(page.locator('main')).not.toContainText('Reading the wall…');

  await expect(page.locator('main')).toContainText(/\d+ cards\b/);
  const all = await page.locator('body').innerText();
  for (const banned of [
    /\bshowing \d+ of \d+/i,
    /\bstreak\b/i,
    /\bgoal\b/i,
    /\btarget\b/i,
    /\bunread\b/i,
    /\bto[- ]read\b/i,
    /\bremaining\b/i,
    /\bleft\b(?! behind)/i,
  ]) {
    expect(all, `"${banned}" is a portion or a target, not a thing you did`).not.toMatch(banned);
  }

  // And the door to it is a link, never a figure — the header is checked for
  // digits by `the library surface greets you with no numbers` besides.
  await page.goto('/');
  const chrome = await page.locator('header').innerText();
  expect(chrome).toContain('Cards');
  expect(chrome).not.toMatch(/\d/);
});

/**
 * A month with no device data says so. It does not show a zero.
 *
 * The most important line on the reading-life page, and the reason item 42
 * exists at all: bucketing `ActivityByDay` into months above the seam collapses
 * `null` to `0` on the first `reduce`, so a month the device never measured
 * renders as *you read for zero minutes*.
 */
test('the reading life renders an absence as an absence', async ({ page }) => {
  await page.goto('/life');
  await expect(page.locator('main')).not.toContainText('Reading the log…');

  // November 2024 has activity days and no device behind them.
  const nov = page.locator('li').filter({ hasText: 'November 2024' });
  // Case-insensitive: the same absence is a chip in one place and the first
  // words of a sentence in another, and which it is is phrasing.
  await expect(nov).toContainText(/no device data/i);
  await expect(nov).not.toContainText('0 min');

  // February 2025 measured a very little, which is not the same thing at all.
  const feb = page.locator('li').filter({ hasText: 'February 2025' });
  await expect(feb).toContainText('0 min');
  await expect(feb).not.toContainText(/no device data/i);

  // April 2025 has pages and no minutes: the two are independent `Option`s.
  const apr = page.locator('li').filter({ hasText: 'April 2025' });
  await expect(apr).toContainText('minutes not measured');
  await expect(apr).toContainText('120 pages');
});

/**
 * Counts are allowed here, because it is a place you chose to go — and
 * `activity_days` must never become a streak.
 *
 * It is a count of days inside a range you asked for: past tense, bounded, and
 * not consecutive. A "current streak" rendered from it would be a threshold
 * announced in advance in a costume, and it is the nearest wrong turn on this
 * screen.
 */
test('the reading life counts what you did and sets no target', async ({ page }) => {
  await page.goto('/life');
  await expect(page.locator('main')).not.toContainText('Reading the log…');

  // It is allowed to carry numbers. That is the whole distinction.
  await expect(page.locator('main')).toContainText('books finished');

  const all = await page.locator('body').innerText();
  for (const banned of [
    /\bstreak\b/i,
    /\bgoal\b/i,
    /\btarget\b/i,
    /\bin a row\b/i,
    /\bunread\b/i,
    /\bto[- ]read\b/i,
    /\bremaining\b/i,
    /\bon track\b/i,
    /\bkeep it up\b/i,
    /\bbehind\b/i,
    /\bpace\b/i,
  ]) {
    expect(all, `"${banned}" is a target, not a thing you did`).not.toMatch(banned);
  }
});

/**
 * The home surface still greets you with no numbers, **with a moment on it**.
 *
 * The rule was qualified this session, not relaxed: a number on the home surface
 * may describe one book and never the collection or what is left. A moment is
 * the newest thing on that surface and the likeliest to break it — which is why
 * `run_ended` is spoken as its span rather than as `4 days`.
 */
test('a moment puts no number on the home surface', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');

  const band = page.locator('section').filter({ hasText: 'You finished' });
  await expect(band).toBeVisible();
  const said = await band.locator('p').innerText();
  expect(said, 'a moment describing the collection with a number is a badge').not.toMatch(/\d/);

  const chrome = await page.locator('header').innerText();
  expect(chrome).not.toMatch(/\d/);
});

/**
 * Reading mode at rest — the whole claim of the route, asserted (item 54).
 *
 * The claim is that the surface shows the book and the four things you can do to
 * it, and nothing else. Three of these four assertions are about what is *not*
 * there, which is the half a screenshot review reads past: an extra region, a
 * count on a verb and a stray shell header all look plausible in a PNG.
 */
test('reading mode shows the book, four verbs, and both ways out', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/reading');
  await expect(page.locator('main')).not.toContainText('Opening…');

  // The shell's header is a route group away, and this is the assertion that
  // says so. `(shell)/+layout.svelte` draws a nav labelled *Places*; if the
  // group ever collapsed back into one layout, every other test here would pass.
  await expect(page.getByRole('navigation', { name: 'Places' })).toHaveCount(0);

  // Four verbs, and each carries the letter that reaches it — a shortcut nobody
  // can discover is a shortcut nobody uses.
  const verbs = page.getByRole('group', { name: 'What you can do' }).getByRole('button');
  await expect(verbs).toHaveCount(4);
  const KEYS: [string, string][] = [
    ['Note', 'n'],
    ['Page', 'p'],
    ['Passages', 's'],
    ['Books', 'b'],
  ];
  for (const [label, key] of KEYS) {
    const verb = verbs.filter({ hasText: label });
    await expect(verb).toHaveAttribute('aria-keyshortcuts', key);
  }

  // Nothing is up until it is asked for.
  for (const name of ['Note', 'Page', 'Passages', 'Books']) {
    await expect(page.getByRole('button', { name, exact: false })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  }

  // Both exits, at rest.
  await expect(page.getByRole('link', { name: 'The book' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'The library' })).toBeVisible();

  expect(problems, 'the console must be clean').toEqual([]);
});

/**
 * One panel at a time, and the way out survives it.
 *
 * The second half is the one that matters. *Nothing is a dead end* is satisfied
 * trivially at rest — the exits are right there — and the state where it is
 * actually at risk is with a panel covering the surface.
 */
test('one panel is open at a time, and the exits stay on screen', async ({ page }) => {
  await page.goto('/reading');
  await expect(page.locator('main')).not.toContainText('Opening…');

  const note = page.getByRole('button', { name: /^Note/ });
  const books = page.getByRole('button', { name: /^Books/ });

  await note.click();
  await expect(note).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('textbox', { name: 'Note' })).toBeVisible();

  await books.click();
  await expect(books).toHaveAttribute('aria-pressed', 'true');
  // Opening one closes the last — the single-slot rule, which is the design and
  // not an incidental consequence of how the markup happens to be nested.
  await expect(note).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByRole('textbox', { name: 'Note' })).toHaveCount(0);

  // With a panel open, both ways out are still there.
  await expect(page.getByRole('link', { name: 'The book' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'The library' })).toBeVisible();

  // And pressing the lit verb again puts the surface back to the book.
  await books.click();
  await expect(books).toHaveAttribute('aria-pressed', 'false');
});

/**
 * The page a reader types, and the answer that comes back from below the seam.
 *
 * Book 3 is the doorstop: 1408 pages, so it is the one open read with an honest
 * denominator. The assertion is deliberately on `of 1408` and on a percentage
 * the box was never given — a panel that echoed its own input would show `p.
 * 214` and pass every other check on this page.
 */
test('a page can be said, and what comes back is the engine’s arithmetic', async ({ page }) => {
  await page.goto('/reading?book=3');
  await expect(page.locator('main')).not.toContainText('Opening…');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('The Doorstop');
  await expect(page.locator('.where')).toHaveText('p. 500 of 1408 · 35%');

  await page.getByRole('button', { name: /^Page/ }).click();
  const box = page.getByRole('textbox', { name: 'Page you are on' });
  // The box starts at the record, so a reader correcting a page does not retype
  // it from nothing.
  await expect(box).toHaveValue('500');

  await box.fill('704');
  await page.getByRole('button', { name: 'Say so' }).click();

  // 704 * 100 / 1408 = 50. The frontend sent a page and nothing else.
  await expect(page.locator('.where')).toHaveText('p. 704 of 1408 · 50%');
  // Past tense, about a thing just done.
  await expect(page.locator('.said')).toContainText('p. 704 of 1408 · 50%');
  // The panel closes on a successful write: the surface goes back to the book.
  await expect(page.getByRole('button', { name: /^Page/ })).toHaveAttribute(
    'aria-pressed',
    'false',
  );
});

/**
 * A page nobody meant is refused, and the refusal says what would work.
 *
 * The engine takes an `i64` and would store `0` without complaint, so this is
 * the frontend's own refusal and it has to be tested here — there is no engine
 * test that would fail if this went away.
 */
test('a page that is not a page is refused with the move that would work', async ({ page }) => {
  await page.goto('/reading?book=3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  await page.getByRole('button', { name: /^Page/ }).click();
  await page.getByRole('textbox', { name: 'Page you are on' }).fill('nowhere');
  await page.getByRole('button', { name: 'Say so' }).click();

  await expect(page.getByRole('alert')).toContainText('whole number');
  // Nothing was written: the record still says where it said.
  await expect(page.locator('.where')).toHaveText('p. 500 of 1408 · 35%');
});

/**
 * Reading mode counts nothing.
 *
 * The surface a reader leaves open while reading is the last place in this app
 * that may put a number on them. `mode.ts` asserts the verb labels carry no
 * digit; this asserts the rendered surface carries none of the completion
 * vocabulary either — including with the passages panel open, which is the one
 * place a `.length` would be one keystroke away.
 */
test('reading mode says what you did and never what is left of it', async ({ page }) => {
  await page.goto('/reading?book=3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  await page.getByRole('button', { name: /^Passages/ }).click();
  await expect(page.getByRole('heading', { name: 'Come across' })).toBeVisible();

  const body = (await page.locator('main').innerText()).toLowerCase();
  for (const word of ['unread', 'remaining', 'left', 'goal', 'streak', 'target', 'yet']) {
    expect(body, `"${word}" is completion framing`).not.toMatch(new RegExp(`\\b${word}\\b`));
  }
});

/**
 * The books you have open, as links — and the one you are on is still in the
 * list.
 *
 * A panel that removed the current entry would change length as the reader moved
 * through it, which is the shape of a list that cannot be scanned.
 */
test('reading mode switches book by URL, and marks the one you are on', async ({ page }) => {
  await page.goto('/reading?book=3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  await page.getByRole('button', { name: /^Books/ }).click();
  const here = page.locator('a[aria-current="page"]');
  await expect(here).toHaveCount(1);
  await expect(here).toContainText('The Doorstop');

  await page.getByRole('link', { name: /A Book I Went Back To/ }).click();
  await expect(page).toHaveURL(/\/reading\?book=12$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('A Book I Went Back To');
  // Picking closes the panel: you asked to read something, not to keep browsing.
  await expect(page.getByRole('button', { name: /^Books/ })).toHaveAttribute(
    'aria-pressed',
    'false',
  );
});

/**
 * The keyboard, which is most of the reason this surface is worth leaving open.
 *
 * The two rejections are the interesting half: a modified keystroke belongs to
 * the platform, and a letter typed into the note box is text — or the note box
 * could not contain the word *note*.
 */
test('the verbs have keys, and a key typed into a field is text', async ({ page }) => {
  await page.goto('/reading?book=3');
  await expect(page.locator('main')).not.toContainText('Opening…');

  await page.keyboard.press('n');
  const box = page.getByRole('textbox', { name: 'Note' });
  await expect(box).toBeVisible();

  await box.fill('');
  await box.pressSequentially('note on page b');
  // Every one of those letters is a verb key. None of them fired.
  await expect(box).toHaveValue('note on page b');
  await expect(page.getByRole('button', { name: /^Note/ })).toHaveAttribute('aria-pressed', 'true');

  // Escape is exempt from that rule, or a panel would be a trap for a keyboard.
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: /^Note/ })).toHaveAttribute('aria-pressed', 'false');
});

/**
 * The other half of reading mode, rendered — every panel, looked at.
 *
 * The resting state is in `ROUTES` and the working state was in nothing, which
 * is this repo's standing complaint in a new costume: the surface that swaps its
 * whole layout had one of its two layouts covered by a screenshot. A panel that
 * overflows the window, or a book row that collapses on a 220-character title,
 * looks exactly like a passing run from here.
 *
 * Book 3 rather than the default, because it is the open read that has
 * passages, notes and an honest page count — the three panels are empty on the
 * book the route lands on by default, and an empty panel is not the case the
 * layout has to survive.
 */
const PANELS = [
  { verb: 'Page', name: 'reading-page' },
  { verb: 'Note', name: 'reading-note' },
  { verb: 'Passages', name: 'reading-passages' },
  { verb: 'Books', name: 'reading-books' },
];

for (const panel of PANELS) {
  test(`${panel.name} renders`, async ({ page }) => {
    const problems: string[] = [];
    page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
    page.on('pageerror', (e) => problems.push(e.message));

    await page.goto('/reading?book=3');
    await expect(page.locator('main')).not.toContainText('Opening…');
    await page.getByRole('button', { name: new RegExp(`^${panel.verb}`) }).click();

    // The book is still on screen with a panel up — it is the thing the panel is
    // about, and a surface that hid it would be a dialog with better manners.
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('The Doorstop');
    // And the surface is still not a dead end.
    expect(await page.locator('a').count()).toBeGreaterThan(0);

    expect(problems, 'the console must be clean').toEqual([]);
    await expect(page).toHaveScreenshot(`${panel.name}.png`, { fullPage: true });
  });
}

/**
 * The devices page's working state — item 55.
 *
 * The resting state is in `ROUTES`, and it is **half the design**: the
 * install's destination path and the rename box are only reachable by pressing
 * something, and item 54 shipped exactly that half rendered in no test. So the
 * two states a write goes through are screenshotted like a route.
 *
 * The assertion that matters is not the picture. `docs/decisions.md` requires
 * the destination be shown *before* an install, and a path shown afterwards is
 * not the same promise — so this presses the verb, checks the path is on
 * screen, and checks **nothing has been written**, which is the half a
 * screenshot cannot make a claim about.
 */
test('the install shows where it will write before it writes', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  page.on('pageerror', (e) => problems.push(e.message));

  await page.goto('/devices');
  await expect(page.locator('main')).not.toContainText('Looking for readers…');

  // The one volume with nothing of ours on it.
  const card = page.locator('article').filter({ hasText: 'PB632' });
  await expect(card).toContainText('readingbuddy is not on this reader');
  await card.getByRole('button', { name: 'Connect this reader' }).click();

  // The exact path, before anything happens.
  await expect(card).toContainText('/applications/koreader/plugins/readingbuddy.koplugin');
  // And it says what it will *not* touch, which is the other half of the promise.
  await expect(card).toContainText('nothing else on this reader');
  // Still not installed: the first step wrote nothing.
  await expect(card).toContainText('readingbuddy is not on this reader');

  // The other write on the card, opened at the same time, so one screenshot
  // carries both states this page can be in.
  await page
    .locator('article')
    .filter({ hasText: 'Kindle' })
    .getByRole('button', { name: 'Give it a name' })
    .click();

  expect(problems, 'the console must be clean').toEqual([]);
  await expect(page).toHaveScreenshot('devices-working.png', { fullPage: true });
});

/**
 * A reader readingbuddy will not write to offers **no way to write to it**.
 *
 * A disabled button is a dead end with a tooltip. The refusal is a sentence
 * that names the file and the move, and the control is simply absent — which is
 * a claim about what is *not* in the markup, and therefore not one a screenshot
 * can make.
 */
test('a reader we are leaving alone has no install control at all', async ({ page }) => {
  await page.goto('/devices');
  await expect(page.locator('main')).not.toContainText('Looking for readers…');

  const card = page.locator('article').filter({ hasText: 'the one I lent Sam' });
  await expect(card).toContainText('readingbuddy is leaving this reader alone');
  await expect(card).toContainText('main.lua');
  await expect(card).toContainText('move it aside');

  // Not disabled — absent. Every verb that would write the plugin.
  await expect(card.getByRole('button', { name: /Connect this reader/ })).toHaveCount(0);
  await expect(card.getByRole('button', { name: /Update the plugin/ })).toHaveCount(0);
  await expect(card.getByRole('button', { name: /Put the plugin on again/ })).toHaveCount(0);
  // Reading from it is still offered: the refusal is about writing.
  await expect(card.getByRole('button', { name: 'Bring everything across' })).toHaveCount(1);
});

/**
 * Forgetting is our side only, and the page has to say so.
 *
 * `ForgetDevice` drops our row and cannot reach the device — the plugin and the
 * token stay where they are. Copy that said *removed* without saying *from
 * here* would leave somebody believing a reader they lent out had been cleaned,
 * which is the one sentence on this page that could mislead about a secret.
 */
test('forgetting a reader says the plugin stays on it', async ({ page }) => {
  await page.goto('/devices');
  await expect(page.locator('main')).not.toContainText('Looking for readers…');

  // The reader in a bag — the only one that offers it, because with the volume
  // in front of you the exact move is taking the plugin off.
  const away = page.locator('article').filter({ hasText: 'Forget this reader' });
  await expect(away).toHaveCount(1);
  await expect(away).toContainText('only on this computer');
  await expect(away).toContainText('the plugin stays on the reader');

  // The chip **exactly**, and not `hasText: 'plugged in'`: the away card's own
  // *Last plugged in at* contains that substring, and `hasText` is a
  // case-insensitive substring match — so the loose selector picked up the one
  // card the assertion is about and passed while checking nothing.
  const here = page
    .locator('article')
    .filter({ has: page.getByText('Plugged in', { exact: true }) });
  await expect(here).toHaveCount(4);
  await expect(here.getByRole('button', { name: 'Forget this reader' })).toHaveCount(0);
});

/**
 * The axiom, on the surface most likely to break it.
 *
 * A device page is where an inbox grows: *3 books waiting*, a badge in the nav,
 * a total across readers. Numbers about **one reader's own contents** are
 * allowed here for `/life`'s reason — a page you chose to open, past tense —
 * and the three things that are not allowed are asserted rather than reviewed.
 */
test('the devices page counts no work you have left', async ({ page }) => {
  await page.goto('/devices');
  await expect(page.locator('main')).not.toContainText('Looking for readers…');

  // No number in the chrome. The nav is the place a badge would appear.
  await expect(page.locator('header nav')).not.toContainText(/\d/);
  // None of the completion vocabulary, anywhere on the page.
  await expect(page.locator('main')).not.toContainText(
    /pending|remaining|to do|overdue|unsynced|out of date/i,
  );
  // And no total across readers: every figure names one reader's own contents.
  await expect(page.locator('main')).not.toContainText(/across (all|your) (readers|devices)/i);
});
