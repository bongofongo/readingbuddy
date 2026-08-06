/**
 * A library in memory, for layer 1 and layer 2.
 *
 * It implements the same [`LibraryClient`] the real one does, so a drifted or
 * renamed DTO field is a `tsc` error **here** — which is the whole reason
 * `docs/gui/testing.md` chose an injected fake over `mockIPC`, whose command-name
 * strings drift silently.
 *
 * ## Two fixtures, one purpose — and they must stay in step
 *
 * `corpus gen-devdb` builds the *app's* fixture: a real SQLite library the real
 * engine reads. This is the *frontend's*: the same hostile shapes, as plain data,
 * with no database and no Rust. Two fixtures is a real cost and it is paid on
 * purpose — layer 2 has to run in a bare browser with no IPC, so it cannot reach
 * the database one, and a `make shots` that needed a built binary would be the
 * minute-scale loop `testing.md` argues against.
 *
 * The cost is that they can diverge. So the shapes here are named after the
 * entries in `corpus/generated/devdb/manifest.json` and each carries the same
 * comment about what it is for. Adding a hostile case to one and not the other is
 * the drift to watch for; unifying them (a generator that emits both) is open
 * work, recorded in the session log rather than pretended away.
 */
import type {
  BookDto,
  BookFileDto,
  BookSortDto,
  BookTagDto,
  CreatedNoteDto,
  FieldSourceDto,
  HighlightDto,
  NewNoteDto,
  NoteDto,
  NoteKindDto,
  OutgoingLinkDto,
  PathsDto,
  RatingDto,
  RatingScaleDto,
  ReadingDto,
  TableOfContentsDto,
} from './bindings';
import type { LibraryClient, OpenReading, StoredBook } from './client';

/**
 * Every field, so a new DTO column shows up here as a type error.
 *
 * **The derived fields are literals, never computed here.** `progress`,
 * `series_label`, `reading_state` and `authors_display` are the engine's
 * answers (spec item 17), and a fake that re-derived them would be a second
 * implementation of the rules this app exists not to have twice — and one that
 * agreed with itself no matter how wrong it was. Spelling them out is the point:
 * each one below states what the engine *should* say about that row.
 */
/**
 * An invented jacket colour, and deliberately **not** a measurement.
 *
 * The engine's `cover_accent` is the median of a 2px border around a real PNG.
 * There is no PNG here, so this is fixture *data* rather than a re-derivation of
 * an engine rule — the thing this file must never do is recompute `progress` or
 * `series_label`, not invent a colour for a file that does not exist.
 *
 * It varies per id for `gen-devdb`'s reason: two hundred identical accents make
 * "every tile drew the same placeholder" and "the tiles are off by one"
 * invisible, which is exactly the class of bug a placeholder colour is for.
 */
function accent(id: number): { r: number; g: number; b: number } {
  return { r: (id * 37) % 256, g: (id * 89) % 256, b: (id * 151) % 256 };
}

function book(id: number, over: Partial<BookDto>): StoredBook {
  return {
    title: `Book ${id}`,
    sort_title: null,
    authors: ['Ada Ordinary'],
    translators: [],
    publisher: 'A Publisher',
    publish_year: 1988 + id,
    language: 'en',
    isbn_10: null,
    isbn_13: null,
    openlibrary_key: null,
    googlebooks_id: null,
    cover_url: null,
    // Cover-bearing by default, because twenty of the twenty-one declared cases
    // are — `A Book With No Cover At All` is the one that overrides these four
    // to null, and its value is that it is *alone*.
    //
    // `cover_shelf_path` equals `cover_path` here, and that is the **rule** and
    // not a coincidence: `make dev-db` writes 240×360 covers, both dimensions
    // under `images::THUMB_MAX` (400), so no shelf tier is written and
    // `Book::shelf_cover_path` falls back to the original. A fixture asserting a
    // thumb *exists* would be asserting itself rather than the rule.
    cover_path: `/fake/database/images/dev-${String(id).padStart(4, '0')}.png`,
    cover_shelf_path: `/fake/database/images/dev-${String(id).padStart(4, '0')}.png`,
    // 240/360 as an f32 widened to f64 — the same bytes `PAPERBACK_ASPECT`
    // carries, which is why `width_over_height` below does not move when this
    // goes from absent to present. Only `width_source` does.
    cover_aspect: 0.6666666865348816,
    cover_accent: accent(id),
    page_count: 300,
    description: null,
    first_sentence: null,
    // One provider subject, which is what `gen-devdb`'s own `base()` gives every
    // edge case. It was `[]` here until item 38 — the two fixtures disagreeing
    // about a column with nothing to say so.
    subjects: ['Fiction'],
    series: null,
    series_index: null,
    current_page: null,
    finished: false,
    reading_state: null,
    date_started: null,
    date_finished: null,
    progress: { progress: 'untouched' },
    series_label: null,
    authors_display: ['Ada Ordinary'],
    // Item 19's arithmetic over the fields above: 300 recorded pages and a
    // measured cover. Stated rather than computed, for this file's own reason —
    // a fixture that ran the rule would agree with the engine no matter how
    // wrong either of them was.
    //
    // `width_source` is `recorded` because `cover_aspect` is present, and those
    // two move together by `EditionShape`'s own definition: it reads
    // `Some(aspect) => Recorded`, `None => (PAPERBACK_ASPECT, Assumed)`. The
    // book that overrides `cover_aspect` to null owes an `assumed` here, and
    // `fake.test.ts` asserts the pair rather than trusting it.
    shape: {
      width_over_height: 0.6666666865348816,
      width_source: 'recorded',
      thickness_over_height: 0.10444444417953491,
      thickness_source: 'recorded',
    },
    created_at: 1735689600,
    last_modified: 1735689600 + id,
    ...over,
    // **After** the spread, and that is the point rather than a style choice.
    // `BookDto.id` is `number | null` while `StoredBook`'s is `number` — the
    // whole reason `StoredBook` exists — so an `over` naming an id could put a
    // null in the one field every route uses as a key. `as StoredBook` hid
    // exactly this until item 38 removed it; now the id this function was
    // *called* with always wins, and tsc proves the invariant instead of a cast
    // asserting it.
    id,
  };
}

/**
 * The hostile set, mirroring `devdb.rs`'s `edge_cases()`.
 *
 * Order matters for the screenshots: these are the first tiles in the grid, so a
 * reviewer sees the cases and not two hundred ordinary books.
 */
const BOOKS: StoredBook[] = [
  // page_count = 0 — item 17b's false denominator. Any percentage over this is a
  // divide by zero, so `of` is **absent** here and not zero: that is the whole
  // normalisation, stated as a fixture.
  book(1, {
    title: 'A Book Of Zero Pages',
    page_count: 0,
    reading_state: { state: 'reading' },
    current_page: 0,
    progress: { progress: 'started', page: 0, of: null, fraction: null, percent: null, source: null },
  }),
  // page_count NULL — absence, not zero. A progress bar has nothing to draw.
  book(2, {
    title: 'A Book Of Unknown Length',
    page_count: null,
    reading_state: { state: 'reading' },
    current_page: 40,
    progress: { progress: 'started', page: 40, of: null, fraction: null, percent: null, source: null },
  }),
  // The fat end of item 19's thickness scale, and the ordinary progress case.
  book(3, {
    title: 'The Doorstop',
    page_count: 1408,
    reading_state: { state: 'reading' },
    current_page: 500,
    progress: {
      progress: 'started',
      page: 500,
      of: 1408,
      fraction: 500 / 1408,
      // Integer division, not `Math.floor(fraction * 100)` — see `ProgressDto`.
      percent: 35,
      source: 'pages',
    },
  }),
  // And the thin end, where a spine has no room for a title.
  book(4, { title: 'A Pamphlet', page_count: 48 }),
  // No cover. Not a broken image and not an apology.
  //
  // All four cover fields go, not just the path: `cover_shelf_path` is derived
  // from a cover that is not there, `cover_aspect` is a measurement of it, and
  // `cover_accent` is its border. And `width_source` drops back to `assumed`,
  // which is the pair `EditionShape` defines — a null aspect with a `recorded`
  // width would be the fixture claiming a measurement of nothing.
  book(5, {
    title: 'A Book With No Cover At All',
    cover_path: null,
    cover_shelf_path: null,
    cover_aspect: null,
    cover_accent: null,
    shape: {
      width_over_height: 0.6666666865348816,
      width_source: 'assumed',
      thickness_over_height: 0.10444444417953491,
      thickness_source: 'recorded',
    },
  }),
  // 220 characters. Clipping, wrapping, and the whole title on the detail page.
  book(6, {
    title:
      'A Title Of Such Considerable And Frankly Self-Indulgent Length That It Cannot Possibly Fit In A Shelf Tile Or A Column Header, Being In The Manner Of The Long Eighteenth Century, Wherein The Title Was Also The Blurb',
  }),
  // `Surname, Given` — the calibre and Goodreads form. Item 17 moved the flip
  // into the engine, so the record keeps the origin's spelling and
  // `authors_display` carries the parse.
  book(7, {
    title: 'A Book Filed Under Surname',
    authors: ['Borges, Jorge Luis'],
    authors_display: ['Jorge Luis Borges'],
  }),
  // A mononym. Any name-splitting rule breaks here, and the answer is to leave
  // it alone.
  book(8, { title: 'A Book By One Name', authors: ['Colette'], authors_display: ['Colette'] }),
  // No author at all — a real state after a bare epub import, not an error.
  book(9, { title: 'A Book By Nobody', authors: [], authors_display: [] }),
  book(10, {
    title: 'A Book By Three People',
    authors: ['Ada Ordinary', 'Grace Second', 'Bea Third'],
    authors_display: ['Ada Ordinary', 'Grace Second', 'Bea Third'],
  }),
  // An abandoned reading. Never styled as failure — and note it is `finished:
  // false` with a `current_page`, identical to the open read above, which is the
  // whole reason the state had to cross. Its `progress` is `started` like any
  // other: putting a book down is not a bar that stopped short.
  book(11, {
    title: 'A Book I Put Down',
    reading_state: { state: 'abandoned' },
    current_page: 60,
    progress: { progress: 'started', page: 60, of: 300, fraction: 0.2, percent: 20, source: 'pages' },
  }),
  book(12, {
    title: 'A Book I Went Back To',
    reading_state: { state: 'reading' },
    current_page: 150,
    progress: { progress: 'started', page: 150, of: 300, fraction: 0.5, percent: 50, source: 'pages' },
  }),
  book(13, {
    title: 'A Book I Finished',
    reading_state: { state: 'finished' },
    finished: true,
    current_page: 300,
    isbn_13: '9780000000017',
    // Finished carries no page: every frontend that had this case dropped the
    // numbers for a word.
    progress: { progress: 'finished' },
  }),
  // CJK. Font fallback, and character-count clipping being wrong.
  book(14, { title: '北回帰線のあたりで', authors: ['村上 春樹'], authors_display: ['村上 春樹'] }),
  // RTL. Bidi layout, and a left-aligned column that should not be.
  book(15, {
    title: 'الكتاب الذي يقرأ من اليمين',
    authors: ['ابن خلدون'],
    authors_display: ['ابن خلدون'],
  }),
  book(16, { title: "Ærøskøbing: Ångström's Œuvre — a Naïve Façade" }),
  // An empty title — the schema's own default, reachable from a sidecar with no
  // doc_props. `titleLabel` is what stops this rendering as a blank line.
  //
  // The empty **string**, not null, and item 38 changed it: `books.title` is
  // `TEXT NOT NULL DEFAULT ''`, so no stored book's title is ever null however
  // `Option`-shaped the DTO is, and this fixture had been modelling a state the
  // database cannot produce. `titleLabel` handles both — but the engine's own
  // `Book::display_title` is `unwrap_or("(untitled)")`, which catches the
  // unreachable one and lets the reachable one through as a blank. That is an
  // engine item, recorded in `docs/decisions.md` rather than patched here.
  book(17, { title: '' }),
  // `series_index` is a REAL and must print as `#2`, never `#2.0`. The label is
  // the engine's since item 17; the frontend no longer builds it.
  book(18, {
    title: 'The Claw Of The Conciliator',
    series: 'The Book Of The New Sun',
    series_index: 2,
    series_label: 'The Book Of The New Sun #2',
    // Open, as it is in `gen-devdb` — a series book is the case where a shelf
    // shows a label *and* progress together, and this fixture had it unread
    // until item 38, so that pairing was never rendered anywhere.
    reading_state: { state: 'reading' },
    current_page: 100,
    // `percent` is integer division (100 * 100 / 300 = 33), not
    // `Math.floor(fraction * 100)` — see `ProgressDto`.
    progress: {
      progress: 'started',
      page: 100,
      of: 300,
      fraction: 100 / 300,
      percent: 33,
      source: 'pages',
    },
  }),
  // A series naming no index. The pair moves together or not at all, and nothing
  // is invented to fill the gap.
  book(19, {
    title: 'A Book In A Series With No Number',
    series: 'An Unnumbered Sequence',
    series_label: 'An Unnumbered Sequence',
  }),
  book(20, {
    title: 'A Book With Subjects And Shelves',
    subjects: ['Philosophy', 'Essays', 'Nineteenth century'],
    reading_state: { state: 'finished' },
    finished: true,
    progress: { progress: 'finished' },
    description:
      'Provider subjects beside minted shelves — two different things that look alike, and one of the separations migration 0013 rests on.',
  }),
  // A status this build does not know. The state is typed and still open: ts-rs
  // gives an exhaustive union over today's variants, and `other` is how a word
  // from a newer importer degrades instead of failing to parse.
  book(21, { title: 'A Book Some Other App Touched', reading_state: { state: 'other', raw: 'paused' } }),
];

/**
 * Every field, and **no `as` cast** — for `book()`'s reason.
 *
 * This function carried `as HighlightDto` until item 27, and it was hiding
 * exactly what the file header promises it catches: three fields that are not
 * on the DTO at all (`pos0`, `pos1`, `identity_hash`) sat here for a wave,
 * stated by the fixture and served to nothing. A cast makes an *added* field
 * silently absent and an *invented* one silently present, which are the two
 * halves of the same drift.
 */
function highlight(
  id: number,
  bookId: number,
  text: string,
  over: Partial<HighlightDto>,
): HighlightDto {
  return {
    id,
    book_id: bookId,
    reading_id: null,
    text,
    chapter: null,
    page: null,
    // The device's own clock, as the device wrote it. Kept because it is what
    // crosses; deliberately not rendered beside our UTC dates — see the book
    // view, which has the argument.
    ko_datetime: '2025-03-04 09:12:00',
    ko_note: null,
    annotation: null,
    source: 'koreader',
    created_at: 1735689600,
    ...over,
  };
}

const HIGHLIGHTS: Record<number, HighlightDto[]> = {
  3: [
    // **Both notes at once**, which is the case the ownership seam exists for
    // and which nothing had ever rendered: `ko_note` is KOReader's and is
    // rewritten toward the device on every pull, `annotation` is the reader's
    // and no import touches it. A screen showing them unlabelled, or showing
    // only one, has lost the distinction `docs/decisions.md` spends a section on.
    highlight(1, 3, 'The thing about a place is that it is still there when you are not.', {
      chapter: 'Chapter 4',
      page: 212,
      ko_note: 'What did they mean by this?',
      annotation: 'The whole book is arguing with this sentence.',
    }),
    highlight(2, 3, 'What survives is not what was meant to.', { chapter: 'Chapter 9', page: 640 }),
    // No chapter and no page: KOReader does produce this, and the "where" line
    // must then render as nothing rather than as a stray separator.
    highlight(3, 3, 'She counted the bells and then stopped counting.', {}),
  ],
  11: [highlight(4, 11, 'It is a mistake to read a map as a promise.', { chapter: 'Chapter 2', page: 31 })],
};

/** No cast here either. `NoteDto` has no `last_modified`; this file claimed one. */
function note(id: number, bookId: number | null, title: string, over: Partial<NoteDto>): NoteDto {
  return {
    id,
    book_id: bookId,
    reading_id: null,
    highlight_id: null,
    page: null,
    location: null,
    file_path: `the-doorstop/${String(id).padStart(4, '0')}.md`,
    title,
    kind: 'note',
    created_at: 1735776000 + id,
    ...over,
  };
}

const NOTES: NoteDto[] = [
  note(1, 3, 'On The Doorstop', { page: 212 }),
  // A reflection beside an ordinary note, in one list rather than in a tab of
  // its own — the TUI's ruling, and for its reason: a section is a *collection*
  // of things and there is exactly one reflection.
  note(2, 3, 'Reflection: The Doorstop', { kind: 'reflection' }),
  // Anchored to a passage rather than to a page. `↳` is the only thing that
  // says so on a row.
  note(3, 3, 'What survives', { highlight_id: 2 }),
  // A review, which is the one kind that carries a rating.
  note(4, 12, 'Review: A Book I Went Back To', { kind: 'review', reading_id: 2 }),
];

/**
 * The bodies, which are **not** on `NoteDto` — `notes` has no body column, and
 * that is why `notes_fts` cannot have triggers.
 */
const BODIES: Record<number, string> = {
  1: `Two hundred pages in and the argument has not started yet, which is either the point or the problem.

See [[Reflection: The Doorstop]], and one day [[The Long Eighteenth Century]].`,
  2: `What this book is doing to me, kept privately and added to as I go.

It rhymes with [[On The Doorstop]] more than I expected.`,
  3: 'A single sentence, hung off a single passage.',
  4: 'Written for other people. Worth the eleven hundred pages, on the second pass.',
};

/**
 * The graph, declared **once** and read in both directions.
 *
 * Stating outbound and inbound separately is how a fixture comes to claim an
 * edge one of its two notes denies writing — and the engine cannot be in that
 * state, because back-resolution keeps `to_note` complete (`docs/decisions.md`
 * entry 9). So there is one list of edges here and both `outgoingLinks` and
 * `backlinks` are views of it.
 *
 * The target resolves by **exact** title. The engine matches `COLLATE NOCASE`
 * and understands alias and heading syntax; this is a fixture lookup and
 * deliberately not a second implementation of that rule, so the titles below
 * are spelled exactly as the notes are.
 */
const EDGES: { from: number; target: string }[] = [
  { from: 1, target: 'Reflection: The Doorstop' },
  // A **forward reference**: no such note yet, and it is not an error. It
  // resolves itself the day that note is written, so a pane shows it as text.
  { from: 1, target: 'The Long Eighteenth Century' },
  { from: 2, target: 'On The Doorstop' },
];

/** Which passages a note cites. By reference, so a device refresh cannot break it. */
const CITATIONS: { note: number; highlight: number }[] = [{ note: 1, highlight: 2 }];

const TAGS: Record<number, BookTagDto[]> = {
  // `book_tags` are minted shelves and are **not** `BookDto.subjects`; the raw
  // string is kept beside the normalized one because the normalization is ours
  // and the shelf name is theirs.
  3: [
    { tag: 'science-fiction', source: 'calibre', raw: 'Science Fiction' },
    { tag: 'doorstop', source: 'goodreads', raw: null },
  ],
};

const FILES: Record<number, BookFileDto[]> = {
  3: [
    {
      sha256: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
      book_id: 3,
      format: 'epub',
      original_name: 'the-doorstop.epub',
      size: 4_718_592,
      added_at: 1735689600,
    },
  ],
};

const CONTENTS: Record<number, TableOfContentsDto> = {
  3: {
    sha256: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    entries: [
      { label: 'Part One', depth: 0, target: 'OEBPS/p1.xhtml', spine_index: 1 },
      { label: 'Chapter 4', depth: 1, target: 'OEBPS/ch4.xhtml', spine_index: 5 },
      // No spine index. Ordinary, and **not** a page number.
      { label: 'Notes on the text', depth: 0, target: 'OEBPS/notes.xhtml#top', spine_index: null },
    ],
  },
  // Book 6 owns a readable file that carries no TOC — `entries: []`, which is a
  // different answer from `null` and must not render as the same sentence.
  6: { sha256: 'a'.repeat(64), entries: [] },
};

const PROVENANCE: Record<number, FieldSourceDto[]> = {
  3: [
    { field: 'title', source: 'open_library', fetched_at: 1735689600 },
    { field: 'cover_url', source: 'google_books', fetched_at: 1735689600 },
    // A field the reader answered back on. `user` outranks every provider.
    { field: 'page_count', source: 'user', fetched_at: 1738368000 },
  ],
};

/**
 * The scale a bare rating would be unreadable against.
 *
 * Half steps on purpose: `1 + 0.5 * 5` is `3.5000000000000004` in binary, so a
 * control enumerating the points has arithmetic to get right, and a fixture
 * with integer steps would never say so.
 */
const SCALE: RatingScaleDto = { id: 1, name: 'stars', min: 1, max: 5, step: 0.5 };

/** Which review notes carry a rating. */
const RATINGS: Record<number, number> = { 4: 4.5 };

/**
 * No `as` cast: `ReadingDto` gained `progress` in item 22 and this fixture had
 * never stated it, so every per-reading progress line in the app would have
 * rendered `undefined` and no test could have said so.
 */
function reading(id: number, bookId: number, over: Partial<ReadingDto>): ReadingDto {
  return {
    id,
    book_id: bookId,
    started_at: 1735689600,
    finished_at: null,
    status: { state: 'reading' },
    source: 'koreader',
    current_page: null,
    ko_status: null,
    ko_percent: null,
    ko_rating: null,
    created_at: 1735689600,
    last_modified: 1735689600,
    progress: { progress: 'untouched' },
    ...over,
  };
}

const READINGS: Record<number, ReadingDto[]> = {
  // Two readings, one closed and one open. Item 28's card is per reading, so
  // this book has two of them — and **this** reading's progress, not the
  // book's: the closed read finished, and printing today's page under it is
  // exactly what `Progress::of_book` warns about.
  12: [
    reading(1, 12, {
      status: { state: 'finished' },
      started_at: 1704067200,
      finished_at: 1706745600,
      current_page: 300,
      progress: { progress: 'finished' },
    }),
    reading(2, 12, {
      status: { state: 'reading' },
      current_page: 150,
      progress: { progress: 'started', page: 150, of: 300, fraction: 0.5, percent: 50, source: 'pages' },
    }),
  ],
};

/**
 * A library in memory.
 *
 * The book set above is **module state and read-only** — it is the declared
 * hostile set, and a test that mutated it would change what the next test sees.
 * Everything item 27 writes to (bodies, annotations, citations, the note list)
 * is **instance** state, copied on construction, so `new FakeClient()` is a
 * fresh library and one page session accumulates its own edits. That is what
 * makes an edit in the running app visible without a database.
 */
export class FakeClient implements LibraryClient {
  #notes: NoteDto[] = NOTES.map((n) => ({ ...n }));
  #bodies: Record<number, string> = { ...BODIES };
  #edges: { from: number; target: string }[] = EDGES.map((e) => ({ ...e }));
  #citations: { note: number; highlight: number }[] = CITATIONS.map((c) => ({ ...c }));
  #annotations: Record<number, string | null> = {};
  #ratings: Record<number, number> = { ...RATINGS };
  #nextNoteId = 100;

  async paths(): Promise<PathsDto> {
    return {
      db_url: 'sqlite://fake/app.db',
      images_dir: '/fake/database/images',
      vault_dir: '/fake/vault',
      files_dir: '/fake/database/files',
      log_dir: '/fake/logs',
    };
  }

  async listBooks(limit = 200, _sort: BookSortDto = 'last_modified'): Promise<StoredBook[]> {
    // The sort is deliberately ignored rather than reimplemented. There is no
    // client-side sorting in this app: a SQL `LIMIT` makes the sort key decide
    // membership and not just order, so sorting belongs in the engine (item 17a)
    // — and a fake that sorted would be a place that rule could be broken and
    // still look tested.
    return BOOKS.slice(0, limit);
  }

  /**
   * Every book whose state is `reading`, paired with the reading `listReadings`
   * would hand back for it — so the two answers cannot disagree here the way
   * two independent fixtures would.
   *
   * Book 12 is the case worth having: it holds a *closed* reading beside an
   * open one, and only the open one may appear. A fake that returned the first
   * reading of each book would pass every test written against the other
   * twenty and be wrong about the one reread in the set.
   */
  async currentlyReading(limit = 12): Promise<OpenReading[]> {
    const open: OpenReading[] = [];
    for (const book of BOOKS) {
      if (book.reading_state?.state !== 'reading') continue;
      const r = (await this.listReadings(book.id)).find((x) => x.status.state === 'reading');
      if (r) open.push({ book, reading: r });
    }
    return open.slice(0, limit);
  }

  async getBook(id: number): Promise<StoredBook | null> {
    return BOOKS.find((b) => b.id === id) ?? null;
  }

  async listHighlights(bookId: number): Promise<HighlightDto[]> {
    // The reader's own annotation is instance state, so an edit made in the app
    // is there when the list is asked for again.
    return (HIGHLIGHTS[bookId] ?? []).map((h) =>
      h.id in this.#annotations ? { ...h, annotation: this.#annotations[h.id] ?? null } : h,
    );
  }

  async listNotes(bookId: number | null): Promise<NoteDto[]> {
    if (bookId === null) return [...this.#notes];
    return this.#notes.filter((n) => n.book_id === bookId);
  }

  async listReadings(bookId: number): Promise<ReadingDto[]> {
    const own = READINGS[bookId];
    if (own) return own;
    const b = BOOKS.find((x) => x.id === bookId);
    if (!b || b.reading_state === null) return [];
    // One reading, so **this** reading's progress is the book's — stated, not
    // recomputed. `Progress::of_book` reads the current reading, so a book with
    // a single read is the one case where the two are the same value, and the
    // reread above is where they are not.
    return [
      reading(100 + bookId, bookId, {
        status: b.reading_state,
        current_page: b.current_page,
        progress: b.progress,
      }),
    ];
  }

  // ---- what is behind one book (item 27) ---------------------------------

  async bookTags(bookId: number): Promise<BookTagDto[]> {
    return TAGS[bookId] ?? [];
  }

  async bookFiles(bookId: number): Promise<BookFileDto[]> {
    return FILES[bookId] ?? [];
  }

  async fieldProvenance(bookId: number): Promise<FieldSourceDto[]> {
    // Absence is the ordinary answer for every book predating migration `0012`.
    return PROVENANCE[bookId] ?? [];
  }

  async tableOfContents(bookId: number): Promise<TableOfContentsDto | null> {
    // `null` for "no file here we can read", and book 6's empty `entries` for
    // "this file carries no TOC". Two answers, both reachable from this fixture.
    return CONTENTS[bookId] ?? null;
  }

  async setAnnotation(highlightId: number, annotation: string | null): Promise<void> {
    this.#annotations[highlightId] = annotation;
  }

  // ---- notes --------------------------------------------------------------

  async getNote(id: number): Promise<NoteDto | null> {
    return this.#notes.find((n) => n.id === id) ?? null;
  }

  async noteBody(noteId: number): Promise<string> {
    return this.#bodies[noteId] ?? '';
  }

  async updateNoteBody(noteId: number, body: string): Promise<void> {
    this.#bodies[noteId] = body;
    // The real one reindexes the FTS row **and** rewrites the wikilink edges,
    // so a fixture that only stored the text would make an edit that adds a
    // `[[link]]` look like it did nothing. The extraction is one regex here
    // against the engine's full alias/heading syntax — enough to keep the pane
    // honest, and not a claim to be the same parser.
    this.#edges = this.#edges.filter((e) => e.from !== noteId);
    for (const m of body.matchAll(/\[\[([^\]|#]+)/g)) {
      const target = (m[1] ?? '').trim();
      if (target) this.#edges.push({ from: noteId, target });
    }
  }

  async createNote(input: NewNoteDto): Promise<CreatedNoteDto> {
    const id = this.#nextNoteId++;
    // `title: null` is the engine deriving one from the body's first six words.
    // Stated here rather than left blank, because that rule is what the screen's
    // placeholder promises and a fixture that ignored it would let the promise rot.
    const title = input.title ?? deriveTitle(input.body);
    const created: NoteDto = {
      id,
      book_id: input.book_id,
      reading_id: input.reading_id,
      highlight_id: input.highlight_id,
      page: input.page,
      location: input.location,
      file_path: `unsorted/${id}.md`,
      title,
      kind: input.kind,
      created_at: 1738368000 + id,
    };
    this.#notes.push(created);
    await this.updateNoteBody(id, input.body);
    return { id, title, file: `/fake/vault/${created.file_path}`, links: [] };
  }

  async deleteNote(noteId: number): Promise<void> {
    this.#notes = this.#notes.filter((n) => n.id !== noteId);
    delete this.#bodies[noteId];
    this.#edges = this.#edges.filter((e) => e.from !== noteId);
    this.#citations = this.#citations.filter((c) => c.note !== noteId);
  }

  async openReflection(bookId: number): Promise<CreatedNoteDto> {
    return this.#openAnchored(bookId, 'reflection');
  }

  async openReview(bookId: number): Promise<CreatedNoteDto> {
    return this.#openAnchored(bookId, 'review');
  }

  /** Open **or mint** — the engine's `open_anchored`, which is why it is one call. */
  async #openAnchored(bookId: number, kind: NoteKindDto): Promise<CreatedNoteDto> {
    const existing = this.#notes.find((n) => n.book_id === bookId && n.kind === kind);
    if (existing) {
      return {
        id: existing.id,
        title: existing.title,
        file: `/fake/vault/${existing.file_path}`,
        links: [],
      };
    }
    const book = BOOKS.find((b) => b.id === bookId);
    const label = kind === 'reflection' ? 'Reflection' : 'Review';
    return this.createNote({
      book_id: bookId,
      reading_id: null,
      highlight_id: null,
      page: null,
      location: null,
      kind,
      title: `${label}: ${book?.title || 'Untitled'}`,
      body: '',
    });
  }

  async noteForReading(readingId: number, kind: NoteKindDto): Promise<NoteDto | null> {
    return this.#notes.find((n) => n.reading_id === readingId && n.kind === kind) ?? null;
  }

  // ---- the graph, both directions off one declaration ---------------------

  async outgoingLinks(noteId: number): Promise<OutgoingLinkDto[]> {
    return this.#edges
      .filter((e) => e.from === noteId)
      .map((e) => ({
        target_title: e.target,
        note: this.#notes.find((n) => n.title === e.target) ?? null,
      }));
  }

  async backlinks(noteId: number): Promise<NoteDto[]> {
    const me = this.#notes.find((n) => n.id === noteId);
    if (!me) return [];
    return this.#edges
      .filter((e) => e.target === me.title)
      .map((e) => this.#notes.find((n) => n.id === e.from))
      .filter((n): n is NoteDto => n !== undefined);
  }

  // ---- citations ----------------------------------------------------------

  async cite(noteId: number, highlightId: number): Promise<void> {
    if (!this.#citations.some((c) => c.note === noteId && c.highlight === highlightId)) {
      this.#citations.push({ note: noteId, highlight: highlightId });
    }
  }

  async uncite(noteId: number, highlightId: number): Promise<boolean> {
    const before = this.#citations.length;
    this.#citations = this.#citations.filter(
      (c) => !(c.note === noteId && c.highlight === highlightId),
    );
    return this.#citations.length !== before;
  }

  async citationsFor(noteId: number): Promise<HighlightDto[]> {
    const ids = this.#citations.filter((c) => c.note === noteId).map((c) => c.highlight);
    const all = Object.values(HIGHLIGHTS).flat();
    return ids
      .map((id) => all.find((h) => h.id === id))
      .filter((h): h is HighlightDto => h !== undefined);
  }

  // ---- the rating on a review --------------------------------------------

  async activeRatingScale(): Promise<RatingScaleDto | null> {
    return SCALE;
  }

  async reviewRating(noteId: number): Promise<RatingDto | null> {
    const value = this.#ratings[noteId];
    // The scale travels **with** the value, never without it — the Goodreads map
    // is user-editable, so a bare number is not re-derivable into anything.
    return value === undefined ? null : { scale: SCALE, value };
  }

  async setRating(noteId: number, value: number): Promise<void> {
    this.#ratings[noteId] = value;
  }

  async clearReviewRating(noteId: number): Promise<boolean> {
    const had = noteId in this.#ratings;
    delete this.#ratings[noteId];
    return had;
  }

  /**
   * No cover **bytes** in the fake, ever. Layer 2 runs in a bare browser with no
   * asset protocol, so any URL returned here would render as a broken image.
   *
   * That was the whole story until item 38, and it hid something: the *shape* a
   * cover-bearing row carries — `cover_shelf_path`, `cover_aspect`,
   * `cover_accent` — was missing from this file entirely, so every tile
   * exercised the no-cover branch and the branch item 26 is about to build on
   * was exercised nowhere. `as StoredBook` is why nothing said so: a cast makes
   * an *added* DTO field silently absent, which is precisely the drift this
   * file's header claims `tsc` catches.
   *
   * So the fields are stated now and only the bytes are withheld. A component
   * asking "what box does this tile reserve" gets a real answer here; a
   * component asking for pixels gets `null` and has to have a designed empty
   * state, which is still the case worth forcing.
   */
  coverSrc(): string | null {
    return null;
  }

  /** The hero shot's bytes are withheld for `coverSrc`'s reason. */
  heroSrc(): string | null {
    return null;
  }
}

/**
 * The engine's `derive_title`: the body's first six words, trimmed of trailing
 * punctuation, or *Untitled*.
 *
 * Restated rather than imported — there is no wire request that asks the engine
 * what it would title a body, so a fake that skipped this would let the note
 * composer promise something nothing checks. It is a **fixture** of the rule,
 * not a second implementation anything in the app calls.
 */
function deriveTitle(body: string): string {
  const words = body.split(/\s+/).filter(Boolean).slice(0, 6);
  if (words.length === 0) return 'Untitled';
  return words.join(' ').replace(/[,.;:]+$/, '');
}
