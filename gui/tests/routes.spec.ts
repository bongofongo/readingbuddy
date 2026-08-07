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
  { name: 'life', path: '/life' },
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
      /Reading the shelf…|Opening…|Reading the log…/,
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

  await page.getByRole('button', { name: 'p. 212 On The Doorstop' }).click();
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
  await page.getByRole('link', { name: 'Reading life' }).click();
  await expect(page.locator('main')).not.toContainText('Reading the log…');
  await page.getByRole('link', { name: '← Library' }).click();
  await expect(page.locator('main')).not.toContainText('Reading the shelf…');

  await expect(page.locator('section').filter({ hasText: 'You finished' })).toHaveCount(0);
  // The next one, not the same one — and still exactly one at a time.
  await expect(page.locator('main')).toContainText('You marked your first passage in The Doorstop.');
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
 * 2023 is in the fixture's activity log with no read closed in it, which is a
 * real shape (the log is filled by highlights and notes as well as by closed
 * reads) and is the one state the empty wall has to word without apologising.
 */
test('the wall filters by year, and an empty year is not a failure', async ({ page }) => {
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

  await page.getByRole('button', { name: '2023', exact: true }).click();
  await expect(page.locator('main')).toContainText('No cards from 2023');
  const empty = await page.locator('main').innerText();
  // Not an apology, not a task, and **no "yet"** — that one word turns an
  // absence into something outstanding.
  for (const banned of [/\byet\b/i, /\bfail/i, /\bmissing\b/i, /\bsorry\b/i, /\bno data\b/i]) {
    expect(empty, `"${banned}" frames an ordinary year as a shortfall`).not.toMatch(banned);
  }
  await expect(page).toHaveScreenshot('cards-wall-empty-year.png', { fullPage: true });

  expect(problems, 'the console must be clean').toEqual([]);
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
  await expect(nov).toContainText('no device data');
  await expect(nov).not.toContainText('0 min');

  // February 2025 measured a very little, which is not the same thing at all.
  const feb = page.locator('li').filter({ hasText: 'February 2025' });
  await expect(feb).toContainText('0 min');
  await expect(feb).not.toContainText('no device data');

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
