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
 * the database one — vitest runs in node with no IPC and no engine at all.
 *
 * The cost is that they can diverge. So the shapes here are named after the
 * entries in `corpus/generated/devdb/manifest.json` and each carries the same
 * comment about what it is for. Adding a hostile case to one and not the other is
 * the drift to watch for; unifying them (a generator that emits both) is open
 * work, recorded in the session log rather than pretended away.
 */
import type {
  ActivitySummaryDto,
  BookDto,
  BookFileDto,
  BookSortDto,
  BookTagDto,
  CalibreBookDto,
  CalibreReportDto,
  CalibreStatusDto,
  CreatedNoteDto,
  DayActivityDto,
  DeviceScanDto,
  FieldSourceDto,
  FlashcardDto,
  HighlightDto,
  InstallReportDto,
  MomentDto,
  MonthActivityDto,
  MountSyncDto,
  NewNoteDto,
  NoteCitationsDto,
  NoteDto,
  NoteKindDto,
  OutgoingLinkDto,
  PairedDeviceDto,
  PathsDto,
  PluginStatusDto,
  RatingDto,
  RatingScaleDto,
  ReadingDto,
  ReadingFilterDto,
  ReadingSortDto,
  ReadingYearsDto,
  SearchHitDto,
  SearchSourceDto,
  StatsImportReportDto,
  TableOfContentsDto,
  UninstallReportDto,
} from './bindings';
import type {
  LibraryClient,
  NewFlashcard,
  OpenReading,
  ReadingPage,
  ReadingRow,
  StoredBook,
} from './client';

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
    progress: {
      progress: 'started',
      page: 0,
      of: null,
      fraction: null,
      percent: null,
      source: null,
    },
  }),
  // page_count NULL — absence, not zero. A progress bar has nothing to draw.
  book(2, {
    title: 'A Book Of Unknown Length',
    page_count: null,
    reading_state: { state: 'reading' },
    current_page: 40,
    progress: {
      progress: 'started',
      page: 40,
      of: null,
      fraction: null,
      percent: null,
      source: null,
    },
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
    progress: {
      progress: 'started',
      page: 60,
      of: 300,
      fraction: 0.2,
      percent: 20,
      source: 'pages',
    },
  }),
  book(12, {
    title: 'A Book I Went Back To',
    reading_state: { state: 'reading' },
    current_page: 150,
    progress: {
      progress: 'started',
      page: 150,
      of: 300,
      fraction: 0.5,
      percent: 50,
      source: 'pages',
    },
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
    // **Dated, since item 47.** These two projections were `null` under a
    // `finished: true` — a state a real `finish_reading` cannot leave behind,
    // since it stamps `finished_at` — and the cost of the inconsistency was
    // that the whole fixture held exactly *one* dated finish, so a year filter
    // over `finished_at` had a single row to select in the entire library and
    // the wall's own subject was untestable. 2025-02-20 to 2025-03-14.
    date_started: 1740009600,
    date_finished: 1741910400,
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
    // A finish in the *other* year, so the wall's year filter partitions rather
    // than merely matching: 2024 holds this and the reread's first read,
    // 2025 holds book 13 alone. 2024-11-10 to 2024-12-05.
    date_started: 1731196800,
    date_finished: 1733356800,
    description:
      'Provider subjects beside minted shelves — two different things that look alike, and one of the separations migration 0013 rests on.',
  }),
  // A status this build does not know. The state is typed and still open: ts-rs
  // gives an exhaustive union over today's variants, and `other` is how a word
  // from a newer importer degrades instead of failing to parse.
  book(21, {
    title: 'A Book Some Other App Touched',
    reading_state: { state: 'other', raw: 'paused' },
  }),
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
    //
    // Two of these three carry `reading_id: 103` — the id `listReadings`
    // synthesises for a book with a single read (`100 + bookId`, below). The
    // third does not, because that is the ordinary shape of a real library:
    // entry 44 calls an unattributed mark *"an ordinary, well-understood set of
    // marks"*, so a fixture where every highlight belonged to a read would make
    // the commonest case the one nothing renders.
    highlight(1, 3, 'The thing about a place is that it is still there when you are not.', {
      chapter: 'Chapter 4',
      page: 212,
      ko_note: 'What did they mean by this?',
      annotation: 'The whole book is arguing with this sentence.',
      reading_id: 103,
    }),
    highlight(2, 3, 'What survives is not what was meant to.', { chapter: 'Chapter 9', page: 640 }),
    // No chapter and no page: KOReader does produce this, and the "where" line
    // must then render as nothing rather than as a stray separator.
    highlight(3, 3, 'She counted the bells and then stopped counting.', { reading_id: 103 }),
  ],
  11: [
    highlight(4, 11, 'It is a mistake to read a map as a promise.', {
      chapter: 'Chapter 2',
      page: 31,
    }),
  ],
  /**
   * The two dated finishes, so the wall is a wall of *cards* and not a wall of
   * absences.
   *
   * Both belong to the reading `listReadings` synthesises for a single-read
   * book (`100 + bookId`), and in both the chosen passage is **not** the first
   * mark — the same trap book 12's reread sets, restated here because these are
   * the rows a year filter selects and therefore the ones a reviewer looks at.
   */
  13: [
    highlight(11, 13, 'It began, as these things do, with a timetable.', {
      chapter: 'I',
      page: 12,
      reading_id: 113,
    }),
    highlight(
      12,
      13,
      'What I had taken for an ending turned out to be the middle of something much larger and much slower, and I had been reading it at the wrong speed the whole way through.',
      { chapter: 'XIV', page: 288, reading_id: 113, annotation: 'The line I will remember it by.' },
    ),
  ],
  20: [
    highlight(13, 20, 'A shelf is an argument about what a person intends to have been.', {
      chapter: 'On Shelving',
      page: 44,
      reading_id: 120,
    }),
    highlight(14, 20, 'Everything else is filing.', {
      chapter: 'On Shelving',
      page: 45,
      reading_id: 120,
    }),
  ],
  /**
   * The reread, and the case item 44's `CardPassage` exists for.
   *
   * Book 12 has two readings, so it has two cards — and the three shapes below
   * are what stop a card being written wrong:
   *
   * - Each read has **its own** longest mark, so a card scoped to `book_id`
   *   would hand both cards the same sentence and the side-by-side comparison
   *   the card exists for would show two identical passages.
   * - The longest mark on the *book* (id 10) belongs to **neither** read, so a
   *   selection over the book would pick a passage no card should carry.
   * - Neither read's chosen passage is its **first**, so `highlights[0]` — the
   *   rule a frontend invents — renders visibly different text.
   */
  12: [
    highlight(5, 12, 'She said it plainly.', { chapter: 'One', page: 11, reading_id: 1 }),
    highlight(
      6,
      12,
      'The first time through, I took this for a description of a house; it is a description of a marriage, and every room in it was already named.',
      { chapter: 'Four', page: 88, reading_id: 1, annotation: 'I had this completely backwards.' },
    ),
    highlight(7, 12, 'Nobody was coming.', { chapter: 'Nine', page: 204, reading_id: 1 }),
    highlight(8, 12, 'A door is a decision.', { chapter: 'Two', page: 34, reading_id: 2 }),
    highlight(
      9,
      12,
      'Second time: the house is not a metaphor at all, and the marriage is the thing being described in terms of it. The book is funnier than I remembered and much less kind.',
      { chapter: 'Eleven', page: 231, reading_id: 2, ko_note: 'cf. the opening' },
    ),
    // Longer than either chosen passage, and attributed to **no** read: a
    // highlight the dates could not place belongs to neither card. `null` here
    // is an ordinary answer, not a missing one.
    highlight(
      10,
      12,
      'Marked once, on some pass or other, and the dates can no longer say which — a mark between two readings belongs to neither of them, and the device cannot tell us otherwise.',
      { chapter: 'Six', page: 140 },
    ),
  ],
};

/**
 * Which passage each reading's card carries — **stated, never computed**.
 *
 * The rule is *longest, ties to the lowest id*, and it lives in SQL (item 44)
 * because which passage a card shows is a **selection predicate** and item 17
 * puts those in the engine. So this fixture states the engine's answer the way
 * `book()` states `progress` and `series_label`: a fake that ran the rule would
 * agree with the engine no matter how wrong either of them was, and this file's
 * whole job is to be a second opinion rather than a second implementation.
 *
 * Reading 111 (book 11) is deliberately **absent**: its one mark is
 * unattributed, so it has no card passage at all, and `cardPassage` returns
 * `null` there exactly as `highlightsForReading` returns `[]`.
 */
const CARD_PASSAGE: Record<number, number> = { 1: 6, 2: 9, 103: 1, 113: 12, 120: 13 };

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
  /**
   * The first read's own review and reflection, which is what makes the two
   * cards a **comparison** rather than a duplicate.
   *
   * `gui-vision.md:114`: *"reading Piranesi twice mints two cards, and the two
   * sit side by side showing what changed. What you rated it at 22 and at 31."*
   * The ratings below are 3 then 4.5, so that sentence is a thing this fixture
   * can actually render.
   *
   * Note that every one of these carries a `reading_id`. `NoteScope::Reading` is
   * literally `WHERE reading_id = ?` with **no** fall-back to the book's
   * unanchored notes, so a note created without one appears on no card — which
   * is a real property of a real vault and is why the plain notes (1–3)
   * are left without one here.
   */
  // Titled by its **date**, not by "first read": a read ordinal is item 41's
  // and nothing in this app may spell one, so a fixture that spelled one in a
  // title would defeat the assertion that nothing does.
  note(5, 12, 'Review: A Book I Went Back To (January 2024)', { kind: 'review', reading_id: 1 }),
  note(6, 12, 'Reflection: A Book I Went Back To', { kind: 'reflection', reading_id: 1 }),
  note(7, 12, 'The house, again', { kind: 'note', reading_id: 2, page: 231 }),
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
  5: 'Written the first time through, when I thought I had it.',
  6: `What this book was doing to me in January, and I was wrong about most of it.

It argues with [[On The Doorstop]].`,
  7: 'The same passage, read the other way round.',
};

/** Characters that are terms in prose and syntax in a regex — `C++` is both. */
function escapeRe(t: string): string {
  return t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * A snippet in sqlite's own shape: `>>`/`<<` around the terms, `…` where text
 * was cut, and a window of context rather than the whole field.
 *
 * The engine counts **tokens** (`snippet(…, '…', 12)`); this counts characters,
 * because reproducing fts5's tokenizer here would be a second implementation of
 * the one thing this fake exists not to have an opinion about. What matters to
 * a caller — that a long passage arrives elided, and that the markers can
 * therefore sit anywhere in the string — is the same either way.
 */
const CONTEXT = 60;

function mark(field: string, terms: string[]): string {
  const lower = field.toLowerCase();
  const first = Math.min(...terms.map((t) => lower.indexOf(t)).filter((i) => i >= 0));
  const start = Math.max(0, first - CONTEXT);
  const end = Math.min(field.length, first + CONTEXT * 2);
  const marked = field
    .slice(start, end)
    .replace(new RegExp(`(${terms.map(escapeRe).join('|')})`, 'gi'), '>>$1<<');
  return `${start > 0 ? '…' : ''}${marked}${end < field.length ? '…' : ''}`;
}

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
/**
 * Who quotes what — and **two citing notes, not one**, which is the fixture the
 * item 48 mark is judged on.
 *
 * With only `note 1 → highlight 2` the mark was never rendered apart from the
 * Cite toggle: open note 1 and the one marked passage is also the one whose
 * button is filled, so the two facts appear together and *only* together, and a
 * screenshot review could not tell whether the mark was doing any work. The
 * second row is a passage quoted by a note that is **not** open, which is the
 * case the mark exists for and the only one where it says something the button
 * cannot.
 *
 * Note 3 is *What survives*, which is itself anchored to highlight 2 — so a
 * note anchored to one passage citing another is also stated here, and that is
 * an ordinary vault rather than an edge: an anchor is where a note was written
 * and a citation is what it quotes.
 */
const CITATIONS: { note: number; highlight: number }[] = [
  { note: 1, highlight: 2 },
  { note: 3, highlight: 3 },
];

/** No cast here either — `FlashcardDto` gained two columns in item 45. */
function card(
  id: number,
  bookId: number,
  bookTitle: string,
  word: string,
  over: Partial<FlashcardDto>,
): FlashcardDto {
  return {
    id,
    book_id: bookId,
    highlight_id: null,
    word,
    context: null,
    book_title: bookTitle,
    exported: false,
    ...over,
  };
}

/**
 * The cards already captured, and the three shapes item 49's band has to draw.
 *
 * - **A card on a passage that is also quoted** (book 3, highlight 2). The two
 *   marks are different facts — *a note quotes this* and *you took a word from
 *   this* — and this is the row that stops them being drawn as one thing.
 * - **Two cards on one passage** (book 20, highlight 13), which is the only way
 *   the plural ever renders. A reader taking two words out of one sentence is
 *   ordinary, not an edge case.
 * - **A card anchored to nothing** (book 20, `highlight_id: null`). `flashcards`
 *   has carried the column since `0001_init.sql` and `list_flashcards` never
 *   selected it until item 45, so every card minted before then is this shape.
 *   The passages band must show it against **no** passage rather than guessing
 *   one, and nothing on that screen may render it at all.
 *
 * `exported` is stated and deliberately drawn nowhere: it is `rb cards export`'s
 * bookkeeping, and a tick beside a passage saying a card left for Anki would be
 * this screen reporting on a pipeline the reader did not open it to see.
 */
const FLASHCARDS: FlashcardDto[] = [
  card(1, 3, 'The Doorstop', 'survives', {
    highlight_id: 2,
    context: 'What survives is not what was meant to.',
  }),
  card(2, 20, 'A Book With Subjects And Shelves', 'argument', {
    highlight_id: 13,
    context: 'A shelf is an argument about what a person intends to have been.',
  }),
  card(3, 20, 'A Book With Subjects And Shelves', 'intends', {
    highlight_id: 13,
    context: 'A shelf is an argument about what a person intends to have been.',
  }),
  card(4, 20, 'A Book With Subjects And Shelves', 'shelving', { exported: true }),
];

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

/**
 * Which review notes carry a rating.
 *
 * Two readings of book 12, two reviews, two different numbers — the comparison
 * the card is for. A card whose read has no review carries no rating at all,
 * which is most of them.
 */
const RATINGS: Record<number, number> = { 4: 4.5, 5: 3 };

/**
 * What is worth noticing and has not been shown (item 23).
 *
 * **Newest first by `occurred_at`**, which is the order the engine returns and
 * the only order `limit` makes sense against — it takes from the newest end.
 *
 * All four kinds are here because each ends in a *different move*, and a
 * fixture carrying only the obvious one would leave three arms of
 * `momentSentence` rendered nowhere. `id` is opaque on the wire and opaque
 * here: these are strings a client hands back, never strings it parses.
 */
const MOMENTS: MomentDto[] = [
  // A reading closed — the kind the whole chain is drawn around, and the only
  // kind whose `reading_id` the engine guarantees. It is what mints a card.
  {
    id: 'reading_closed:1',
    kind: { kind: 'reading_closed' },
    book_id: 12,
    reading_id: 1,
    day: '2025-01-31',
    occurred_at: 1738368000,
  },
  {
    id: 'first_annotation:3',
    kind: { kind: 'first_annotation' },
    book_id: 3,
    // Absent where the evidence does not settle on one read, which is ordinary.
    reading_id: null,
    day: '2025-01-20',
    occurred_at: 1737331200,
  },
  {
    id: 'reflection_reached:2:12',
    kind: { kind: 'reflection_reached', note_id: 2, reached_book_id: 12 },
    book_id: 3,
    reading_id: null,
    day: '2025-01-14',
    occurred_at: 1736812800,
  },
  // The one that had to be argued. `days` is on the wire and this frontend
  // deliberately does not draw it — see `$lib/moments/sentence.ts`.
  {
    id: 'run_ended:2025-01-05:2025-01-08',
    kind: { kind: 'run_ended', from: '2025-01-05', to: '2025-01-08', days: 4 },
    book_id: null,
    reading_id: null,
    day: '2025-01-08',
    occurred_at: 1736294400,
  },
];

/**
 * The reading life, as fixture data (items 21, 31, 42).
 *
 * **Stated at both grains, never folded from one to the other**, and that is
 * the whole reason item 42 exists rather than a nicety of this file. A fake
 * that summed its own months into a year would be the client-side bucketing
 * this page was built to refuse, sitting inside the thing that is supposed to
 * catch it — and it would agree with itself no matter how wrong it was.
 *
 * The shapes below are chosen so every branch of the rendering is reachable:
 *
 * - **A whole year with no device data** (2024). A Goodreads CSV or a calibre
 *   library reads exactly like this, and it is the case `minutes: null` exists
 *   for. It must never render as *0 min*.
 * - **A measured zero** (2025-02). Item 31: a twenty-second session records
 *   `Some(0)`, not `None` — the device *is* saying something, and collapsing it
 *   into the absence throws away the distinction the column is nullable to keep.
 * - **Minutes absent beside pages present** (2025-04). The two are independent
 *   `Option`s and one page treating them as a pair would be wrong here.
 * - **Gaps** (no 2025-06 onward, nothing before 2024-11). Only months carrying
 *   an event come back; the empty ones are the client's to draw or to leave out.
 */
/**
 * The days behind some of the months above — the fixture the run panel needs.
 *
 * Only days carrying an event exist, which is the request's own rule, so the
 * gaps here are the absences and the runs are what is left between them. Three
 * shapes are stated on purpose:
 *
 * - **A run of five** (2025-03-04 … 03-08), the longest, and the one a reader
 *   should see named.
 * - **A run of two broken by one missing day** (2025-03-10, 03-11, then 03-13),
 *   so a derivation that forgot to check adjacency reports six instead of five.
 * - **A lone day** (2024-11-02). A run of one is not a run and must not tie.
 *
 * Every date here is far in the past, which matters: a run touching *today* is
 * still running and `longestRunOf` refuses it. A fixture anchored near now would
 * make that rule untestable and would change behaviour as the clock moved.
 */
const DAYS: DayActivityDto[] = [
  { day: '2024-11-02', books: 1, minutes: null, pages: null },
  { day: '2025-03-04', books: 1, minutes: 90, pages: 40 },
  { day: '2025-03-05', books: 1, minutes: 60, pages: 30 },
  { day: '2025-03-06', books: 2, minutes: 120, pages: 55 },
  { day: '2025-03-07', books: 1, minutes: 30, pages: 12 },
  { day: '2025-03-08', books: 1, minutes: 45, pages: 20 },
  { day: '2025-03-10', books: 1, minutes: 25, pages: 10 },
  { day: '2025-03-11', books: 1, minutes: 35, pages: 14 },
  { day: '2025-03-13', books: 1, minutes: 20, pages: 8 },
];

const MONTHS: MonthActivityDto[] = [
  // **A year you read in and finished nothing** (item 47). It is a real shape —
  // `activity_by_month` reads `reading_events`, which a highlight or a note
  // fills as readily as a closed read — and it is the fixture for the one state
  // the wall's year filter cannot otherwise reach: a year the picker offers and
  // no card belongs to. `SUMMARIES['2023']` states `books_finished: 0` so the
  // reading-life page and the wall agree about it rather than contradicting
  // each other on adjacent screens.
  { month: '2023-06', books: 1, activity_days: 4, minutes: null, pages: null },
  { month: '2024-11', books: 2, activity_days: 9, minutes: null, pages: null },
  { month: '2024-12', books: 3, activity_days: 14, minutes: null, pages: null },
  { month: '2025-01', books: 2, activity_days: 12, minutes: 620, pages: 410 },
  { month: '2025-02', books: 1, activity_days: 3, minutes: 0, pages: 0 },
  { month: '2025-03', books: 4, activity_days: 18, minutes: 900, pages: 590 },
  { month: '2025-04', books: 1, activity_days: 6, minutes: null, pages: 120 },
  { month: '2025-05', books: 2, activity_days: 12, minutes: 460, pages: 170 },
];

/**
 * What a period held.
 *
 * `books_finished`, `activity_days`, `notes_created` and `links_created` are
 * counts the engine **originates**, so a zero in one is knowable and is not an
 * absence. `minutes` and `pages` are a device's and are not — which is the whole
 * distinction the reading-life page is built to render.
 */
type PeriodFigures = Omit<ActivitySummaryDto, 'range'>;

/**
 * Everything recorded, kept out of the year map on purpose.
 *
 * `tsconfig` sets `noUncheckedIndexedAccess`, so a `Record` lookup is
 * `T | undefined` **including** for the one key you know is there — and a
 * `?? SUMMARIES.all` fallback would not have discharged it. A separate binding
 * makes the fallback genuinely total rather than asserting that it is, which is
 * this repo's own line about a guard that cannot fail.
 */
const WHOLE_LIFE: PeriodFigures = {
  books_finished: 7,
  activity_days: 74,
  notes_created: 12,
  links_created: 5,
  minutes: 1980,
  pages: 1290,
};

/**
 * One summary per year the fixture has.
 *
 * **2024's minutes are `null` while its `activity_days` is 23**, and that pair
 * is the point: a reader with no device still has a reading life, recorded from
 * highlights, notes and reading endpoints — which is exactly what item 21's
 * three device-free fillers are for, and what a page showing *0 min* for the
 * year would deny.
 */
const SUMMARIES: Record<string, PeriodFigures> = {
  // Days with something on them and **nothing closed** — `books_finished: 0`,
  // which is knowable rather than absent (the engine originates that count) and
  // is what makes `0 books finished` a sentence this fixture can render. It is
  // also the year the wall's picker offers with no card behind it.
  '2023': {
    books_finished: 0,
    activity_days: 4,
    notes_created: 1,
    links_created: 0,
    minutes: null,
    pages: null,
  },
  '2024': {
    books_finished: 3,
    activity_days: 23,
    notes_created: 4,
    links_created: 1,
    minutes: null,
    pages: null,
  },
  '2025': {
    books_finished: 4,
    activity_days: 51,
    notes_created: 8,
    links_created: 4,
    minutes: 1980,
    pages: 1290,
  },
};

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
      progress: {
        progress: 'started',
        page: 150,
        of: 300,
        fraction: 0.5,
        percent: 50,
        source: 'pages',
      },
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
// ---------------------------------------------------------------------------
// Item 55's readers. Not part of `crates/corpus/edge-cases.json` and that is
// not an oversight: `gen-devdb` builds a real library out of a real engine and
// **cannot mint a paired device**, because pairing writes to a mount and there
// is no mount on a build machine. So this half of the fixture has no database
// counterpart to drift from — and the consequence, worth knowing before you
// wonder where your readers went, is that a real `make dev-db` run shows this
// page's *empty* state.
//
// Four readers, each the shape a different branch would render wrong.
// ---------------------------------------------------------------------------

const KINDLE = '/run/media/oliver/Kindle';
const KOBO = '/run/media/oliver/KOBOeReader';
const POCKETBOOK = '/run/media/oliver/PB632';
const BORROWED = '/media/oliver/Reader';

/**
 * Volumes that hold a KOReader install. The engine filters this below the seam;
 * here it is a list, because the *filtering* is not what a screen renders.
 */
const MOUNTS = [KINDLE, KOBO, POCKETBOOK, BORROWED];

const PAIRED: PairedDeviceDto[] = [
  // 1. The ordinary reader: named, plugged in, up to date, everything across.
  //    The case every other one is a departure from.
  {
    device_id: 'd1f0c2ae9b7645bc',
    label: 'Kindle',
    plugin_version: 1,
    installed_at: 1_745_000_000,
    last_mount_path: KINDLE,
    last_seen_at: 1_756_200_000,
    last_synced_at: 1_756_199_000,
  },
  // 2. A name the user typed, and a long one. The label is the only field on
  //    this page a person controls, so it is the only one that can be any
  //    length at all — and a card laid out for `Kindle` breaks here.
  {
    device_id: 'b7c3e1049fa2d8e6',
    label: 'the Kobo I keep by the bed and lend to absolutely everybody',
    plugin_version: 1,
    installed_at: 1_700_000_000,
    last_mount_path: KOBO,
    last_seen_at: 1_756_100_000,
    last_synced_at: null,
  },
  // 3. **No label at all**, so the name falls back to the id — and `last_seen`
  //    is old, because this is the reader in a bag. `last_synced_at: null` here
  //    is the case the copy is most likely to get wrong: it means *not since
  //    readingbuddy started recording*, never *never*.
  {
    device_id: '3a9d5f7e2c1b4088',
    label: null,
    plugin_version: 1,
    installed_at: 1_690_000_000,
    last_mount_path: '/run/media/oliver/KOBOeReader',
    last_seen_at: 1_692_000_000,
    last_synced_at: null,
  },
  // 4. Plugged in, and we will not write to it: a file of ours was edited on
  //    the device. The refusal has to name the file and the next move.
  {
    device_id: 'ff02aa4411bb99cc',
    label: 'the one I lent Sam',
    plugin_version: 1,
    installed_at: 1_752_000_000,
    last_mount_path: BORROWED,
    last_seen_at: 1_756_000_000,
    last_synced_at: 1_752_100_000,
  },
];

/** What each mount's plugin says. Keyed by mount, because that is what is asked. */
const PLUGINS: Record<string, PluginStatusDto> = {
  [KINDLE]: {
    mount: KINDLE,
    plugin_dir: `${KINDLE}/koreader/plugins/readingbuddy.koplugin`,
    installed: true,
    installed_version: 1,
    our_version: 1,
    paired: true,
    device_id: 'd1f0c2ae9b7645bc',
    modified: [],
    unrecognised: [],
    condition: 'current',
  },
  [KOBO]: {
    mount: KOBO,
    plugin_dir: `${KOBO}/.adds/koreader/plugins/readingbuddy.koplugin`,
    installed: true,
    // An older plugin than this build carries. The upgrade is the one write
    // that is safe to offer prominently, since it is our own directory.
    installed_version: 1,
    our_version: 2,
    paired: true,
    device_id: 'b7c3e1049fa2d8e6',
    modified: [],
    unrecognised: [],
    condition: 'upgradable',
  },
  // A KOReader volume with nothing of ours on it — the *connect a new reader*
  // case, and the one whose flow must show `plugin_dir` before it writes.
  [POCKETBOOK]: {
    mount: POCKETBOOK,
    plugin_dir: `${POCKETBOOK}/applications/koreader/plugins/readingbuddy.koplugin`,
    installed: false,
    installed_version: null,
    our_version: 2,
    paired: false,
    device_id: null,
    modified: [],
    unrecognised: [],
    condition: 'absent',
  },
  [BORROWED]: {
    mount: BORROWED,
    plugin_dir: `${BORROWED}/koreader/plugins/readingbuddy.koplugin`,
    installed: true,
    installed_version: 1,
    our_version: 2,
    paired: true,
    device_id: 'ff02aa4411bb99cc',
    modified: ['main.lua'],
    unrecognised: ['notes.txt'],
    condition: 'obstructed',
  },
};

/**
 * What a scan of each mount finds.
 *
 * The Kindle has something to bring across and the Kobo does not, which is the
 * pair that matters: *nothing new* and *nothing here* must not render as the
 * same sentence, and a device with `books: []` is the second.
 */
const SCANS: Record<string, DeviceScanDto> = {
  [KINDLE]: {
    root: KINDLE,
    books: [
      {
        path: `${KINDLE}/documents/Piranesi.sdr/metadata.epub.lua`,
        title: 'Piranesi',
        authors: 'Susanna Clarke',
        partial_md5: 'aa11',
        book_id: 3,
        matched_by: 'md5',
        state: { state: 'updated', new_highlights: 4, refreshed: 1 },
        ko_percent: 0.41,
        ko_status: { status: 'reading' },
      },
      {
        path: `${KINDLE}/documents/Solaris.sdr/metadata.epub.lua`,
        title: 'Solaris',
        authors: 'Stanisław Lem',
        partial_md5: 'bb22',
        book_id: null,
        matched_by: null,
        state: { state: 'new', candidates: [] },
        ko_percent: 0.02,
        ko_status: null,
      },
      {
        path: `${KINDLE}/documents/Dune.sdr/metadata.epub.lua`,
        title: 'Dune',
        authors: 'Frank Herbert',
        partial_md5: 'cc33',
        book_id: 12,
        matched_by: 'md5',
        state: { state: 'unchanged' },
        ko_percent: 1,
        ko_status: { status: 'complete' },
      },
    ],
    warnings: [],
    parsed: 3,
    cached: 0,
  },
  // Nothing on it at all. Not an error, and not the same picture as *up to
  // date* — a reader you have never read on is a reader you have never read on.
  [KOBO]: { root: KOBO, books: [], warnings: [], parsed: 0, cached: 0 },
  [POCKETBOOK]: {
    root: POCKETBOOK,
    books: [
      {
        path: `${POCKETBOOK}/Books/Ubik.sdr/metadata.epub.lua`,
        title: 'Ubik',
        authors: 'Philip K. Dick',
        partial_md5: 'dd44',
        book_id: null,
        matched_by: null,
        state: { state: 'new', candidates: [] },
        ko_percent: 0.6,
        ko_status: { status: 'reading' },
      },
    ],
    warnings: [],
    parsed: 1,
    cached: 0,
  },
  [BORROWED]: {
    root: BORROWED,
    books: [
      {
        path: `${BORROWED}/koreader/help/Quickstart.sdr/metadata.epub.lua`,
        title: null,
        authors: null,
        partial_md5: 'ee55',
        book_id: null,
        matched_by: null,
        // One unreadable sidecar must not cost the view of the rest, and it is
        // the only place a `DiagnosticDto` renders on this page.
        state: {
          state: 'unreadable',
          // `display` is the engine's own `Display`, carried rather than
          // re-derived — three clients formatting the same warning is three
          // chances to disagree with the CLI about it. A screen renders that.
          diagnostic: {
            kind: 'sidecar_unparsable',
            path: 'koreader/help/Quickstart.sdr/metadata.epub.lua',
            severity: 'warning',
            detail: 'attempt to call a nil value',
            display: 'could not parse koreader/help/Quickstart.sdr/metadata.epub.lua',
          },
        },
        ko_percent: null,
        ko_status: null,
      },
    ],
    warnings: [],
    parsed: 1,
    cached: 0,
  },
};

export class FakeClient implements LibraryClient {
  #notes: NoteDto[] = NOTES.map((n) => ({ ...n }));
  #bodies: Record<number, string> = { ...BODIES };
  #edges: { from: number; target: string }[] = EDGES.map((e) => ({ ...e }));
  #citations: { note: number; highlight: number }[] = CITATIONS.map((c) => ({ ...c }));
  #flashcards: FlashcardDto[] = FLASHCARDS.map((c) => ({ ...c }));
  #annotations: Record<number, string | null> = {};
  /**
   * Pages written by [`updateProgress`], overlaid on the book — item 54.
   *
   * An overlay for `#annotations`' reason: `BOOKS` is module state shared by
   * every instance, and a test that turned a page would otherwise change the
   * fixture for the whole suite.
   */
  #pages: Record<number, number> = {};
  #ratings: Record<number, number> = { ...RATINGS };
  #nextNoteId = 100;
  #nextCardId = 100;
  /** Moments already handed back. Acknowledging is idempotent, so this is a set. */
  #surfaced = new Set<string>();
  #device: boolean;
  /** Item 55's readers, and the three overlays that make the page's writes real. */
  #paired = PAIRED;
  #forgotten = new Set<string>();
  #labels: Record<string, string | null> = {};
  #installed: Record<string, Partial<PluginStatusDto>> = {};
  #synced = new Set<string>();
  #plugged: boolean;
  #calibre: boolean;

  /**
   * `device: false` is **a reader, not an edge case** — the required fixture.
   *
   * A library built from a Goodreads CSV, or from calibre, or from epub imports
   * has no `statistics.sqlite3` behind it and therefore no minutes and no pages
   * anywhere, at any grain, for ever. That reader still has a reading life —
   * item 21's three device-free fillers record highlight days, vault days and
   * reading endpoints — and the reading-life page owes them a page that says so
   * rather than a calendar of zeros.
   *
   * It is a constructor argument rather than a twenty-second fixture book
   * because it is a property of the whole library and not of one row.
   */
  constructor(options: { device?: boolean; plugged?: boolean; calibre?: boolean } = {}) {
    this.#device = options.device ?? true;
    // Both default to *present*, so the route suite renders the populated page
    // — the one with four readers and every branch in it. The empty states are
    // reached from vitest, which is where they are asserted; layer 2 picks its
    // fake by URL alone and has no way to construct one.
    this.#plugged = options.plugged ?? true;
    this.#calibre = options.calibre ?? true;
  }

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
      if (r) open.push({ book: this.#turned(book), reading: r });
    }
    return open.slice(0, limit);
  }

  async getBook(id: number): Promise<StoredBook | null> {
    const b = BOOKS.find((x) => x.id === id);
    return b === undefined ? null : this.#turned(b);
  }

  /**
   * A book with whatever page this instance has been told about.
   *
   * **The arithmetic is reproduced here, and that is the point of it.** The real
   * `update_progress` answers with the book re-read, so `percent` comes back off
   * the engine's own division — which is why [`LibraryClient.updateProgress`]
   * returns a book at all instead of the caller keeping the number it sent. A
   * fake that echoed the page without recomputing the percentage would let a
   * screen render its own input and pass.
   *
   * It follows `ProgressDto`'s two rules rather than inventing softer ones: a
   * `page_count` of zero is a false denominator and is **absence** by the time
   * it reaches a DTO, and a page with no length has no percentage — so no track
   * is drawn over one. Book 5 in this fixture is that case on purpose.
   */
  #turned(b: StoredBook): StoredBook {
    const page = this.#pages[b.id];
    if (page === undefined) return b;
    const of = b.page_count !== null && b.page_count > 0 ? b.page_count : null;
    const fraction = of === null ? null : Math.min(1, page / of);
    return {
      ...b,
      current_page: page,
      progress: {
        progress: 'started',
        page,
        of,
        fraction,
        // Integer division over the pages, not `Math.floor(fraction * 100)` —
        // `ProgressDto` says so and the fixture rows above already follow it.
        // The two agree on almost every input, which is exactly what makes the
        // wrong one survive.
        percent: of === null ? null : Math.floor((page * 100) / of),
        source: fraction === null ? null : 'pages',
      },
    };
  }

  /**
   * Writes the page and hands the book back re-read, like the engine.
   *
   * `finished` is accepted and **not implemented**, and that is stated rather
   * than silently true: nothing in this app closes a read yet, so a fake arm for
   * it would be a claim no screen exercises — the shape this repo keeps writing
   * down as worse than an absence. It is on the wire and on the interface
   * because the request has it; the day a screen closes a read, this is the line
   * that has to grow.
   */
  async updateProgress(
    bookId: number,
    page: number | null = null,
    _finished: boolean | null = null,
  ): Promise<StoredBook | null> {
    if (page !== null) this.#pages[bookId] = page;
    return this.getBook(bookId);
  }

  async listHighlights(bookId: number): Promise<HighlightDto[]> {
    // The reader's own annotation is instance state, so an edit made in the app
    // is there when the list is asked for again.
    return (HIGHLIGHTS[bookId] ?? []).map((h) =>
      h.id in this.#annotations ? { ...h, annotation: this.#annotations[h.id] ?? null } : h,
    );
  }

  async listNotes(bookId: number | null, limit: number | null = null): Promise<NoteDto[]> {
    const own = bookId === null ? [...this.#notes] : this.#notes.filter((n) => n.book_id === bookId);
    // Newest first, and the cut is applied *after* the sort — which is the half
    // of `list_notes` a caller can get wrong. The engine orders
    // `created_at DESC, id DESC`; a null `created_at` sorts last here rather
    // than as epoch zero, because the column is nullable and a note with no date
    // is not a note from 1970.
    own.sort((a, b) => (b.created_at ?? -Infinity) - (a.created_at ?? -Infinity) || b.id - a.id);
    return limit === null ? own : own.slice(0, Math.max(0, limit));
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
        // The book's four reading projections **are** the current reading's, so
        // a synthesised reading takes its dates from them rather than carrying
        // a second opinion. `date_started` falls back to the default epoch
        // because most of the set states no start; `date_finished` does not,
        // because `finished_at: null` is what *open* means and inventing one
        // would close every read in the fixture.
        started_at: b.date_started ?? 1735689600,
        finished_at: b.date_finished,
      }),
    ];
  }

  // ---- the wall of cards (items 43, 41, 47) -------------------------------

  /**
   * Every reading in the library as a row, **before any filter** — which is the
   * whole reason this is a separate step.
   *
   * `read_number` and `of_reads` are counted here, over each book's complete
   * list, and never over what a filter left. That is item 43's correction
   * restated as a fixture: the engine had to refuse `ROW_NUMBER() OVER
   * (PARTITION BY book_id …)` because a window function is computed over the
   * rows that survived the `WHERE`, so a wall filtered to 2025 would hold the
   * reread's second read without its first and call it the first. A fake that
   * numbered after filtering would agree with the wrong version.
   *
   * The passage is [`CARD_PASSAGE`]'s stated answer, so the wall and
   * [`FakeClient.cardPassage`] cannot disagree here either.
   */
  async #readingRows(): Promise<ReadingRow[]> {
    const rows: ReadingRow[] = [];
    for (const b of BOOKS) {
      const own = await this.listReadings(b.id);
      own.forEach((r, i) => {
        const chosen = CARD_PASSAGE[r.id];
        rows.push({
          book: b,
          reading: r,
          read_number: i + 1,
          of_reads: own.length,
          passage:
            chosen === undefined
              ? null
              : (Object.values(HIGHLIGHTS)
                  .flat()
                  .find((h) => h.id === chosen) ?? null),
        });
      });
    }
    return rows;
  }

  async listReadingRows({
    limit,
    sort = 'finished',
    offset = 0,
    filter = null,
  }: ReadingPage): Promise<ReadingRow[]> {
    refuseInvertedSpan(filter);
    const matched = (await this.#readingRows()).filter((r) => matchesReadingFilter(r, filter));
    matched.sort(readingOrder(sort));
    const from = Math.max(0, offset);
    // **Negative is no limit and `0` is a page of nothing** — SQLite's own
    // reading of `LIMIT -1`, and `slice(from, from)` is the second half of it.
    return limit < 0 ? matched.slice(from) : matched.slice(from, from + limit);
  }

  /**
   * The same predicate, over the same rows — so the count and the page cannot
   * disagree here any more than they can in the engine, where both are built
   * from one `ReadingFilter::predicate`.
   */
  async countReadings(filter: ReadingFilterDto | null): Promise<number> {
    refuseInvertedSpan(filter);
    return (await this.#readingRows()).filter((r) => matchesReadingFilter(r, filter)).length;
  }

  /**
   * The years, off the same rows the wall draws (item 51).
   *
   * Built from `#readingRows` through the **same** `matchesReadingFilter` the
   * page and the count use, which is the fixture's copy of the engine's own
   * guarantee: the picker offers a year exactly when the wall has rows in it.
   * A fake that answered from a second list would make the one property this
   * request exists for untestable at layer 1.
   *
   * The year is taken from the UTC day string, never from `getFullYear` — a
   * local-time year files a New Year's Eve read under the other year for every
   * reader west of Greenwich, and `finished_in` is UTC.
   */
  async readingYears(filter: ReadingFilterDto | null): Promise<ReadingYearsDto> {
    refuseInvertedSpan(filter);
    const rows = (await this.#readingRows()).filter((r) => matchesReadingFilter(r, filter));
    const years = new Set<number>();
    let open = false;
    for (const row of rows) {
      const finished = row.reading.finished_at;
      // A reading with no `finished_at` is in no year — that is what open means,
      // and it is why this answer is not a bare list.
      if (finished === null) open = true;
      else years.add(Number(dayOfUnix(finished).slice(0, 4)));
    }
    return { years: [...years].sort((a, b) => b - a), open };
  }

  // ---- one card, per reading (item 28) ------------------------------------

  async highlightsForReading(readingId: number): Promise<HighlightDto[]> {
    // Scoped to the read, not to the book — and the reader's own annotation is
    // instance state here for `listHighlights`' reason.
    return Object.values(HIGHLIGHTS)
      .flat()
      .filter((h) => h.reading_id === readingId)
      .map((h) =>
        h.id in this.#annotations ? { ...h, annotation: this.#annotations[h.id] ?? null } : h,
      );
  }

  /** The engine's stated answer, looked up. See [`CARD_PASSAGE`] for why. */
  async cardPassage(readingId: number): Promise<HighlightDto | null> {
    const chosen = CARD_PASSAGE[readingId];
    if (chosen === undefined) return null;
    const all = Object.values(HIGHLIGHTS).flat();
    return all.find((h) => h.id === chosen) ?? null;
  }

  /**
   * `WHERE reading_id = ?`, with **no** fall-back to the book's unanchored
   * notes — which is literally what `NoteScope::Reading` is. A fake that widened
   * to the book when a read had nothing would hide the one thing a card's empty
   * note band has to be honest about.
   */
  async notesForReading(readingId: number): Promise<NoteDto[]> {
    return this.#notes.filter((n) => n.reading_id === readingId);
  }

  // ---- moments (item 23) --------------------------------------------------

  async pendingMoments(limit = 1): Promise<MomentDto[]> {
    // Newest first, and what has been surfaced is gone. `limit` takes from the
    // newest end, which is the only lever the wire offers and the only one here.
    return MOMENTS.filter((m) => !this.#surfaced.has(m.id)).slice(0, limit);
  }

  /**
   * Idempotent, and it does **not** check that the moment is still derivable —
   * both of which are the engine's own behaviour. A well-formed id for a moment
   * that never existed costs one inert row there and one set entry here.
   */
  async acknowledgeMoment(id: string): Promise<void> {
    this.#surfaced.add(id);
  }

  // ---- the reading life (items 21, 31, 42) --------------------------------

  /**
   * The summary for the span asked about.
   *
   * A span lying inside one calendar year gets that year's; anything wider gets
   * the whole life. That is a **fixture lookup** keyed on the request, not an
   * aggregation of the months — see [`SUMMARIES`], and the module rule that this
   * file states the engine's answers rather than computing them.
   *
   * `range` is echoed back because the DTO carries it and a client may read it.
   */
  async activitySummary(from: string, to: string): Promise<ActivitySummaryDto> {
    const year = from.slice(0, 4);
    const within = year === to.slice(0, 4) && from.endsWith('-01-01');
    const found: PeriodFigures = (within ? SUMMARIES[year] : undefined) ?? WHOLE_LIFE;
    return { range: { from, to }, ...this.#measured(found) };
  }

  async activityByDay(from: string, to: string): Promise<DayActivityDto[]> {
    return DAYS.filter((d) => d.day >= from && d.day <= to).map((d) => this.#measured(d));
  }

  async activityByMonth(from: string, to: string): Promise<MonthActivityDto[]> {
    // Both ends inclusive, and `YYYY-MM` compares correctly against the first
    // seven characters of a `YYYY-MM-DD` — the same property `substr(day, 1, 7)`
    // rests on, used again rather than a second date function that could
    // disagree with it. A month at the edge is reported, never widened away.
    return MONTHS.filter((m) => m.month >= from.slice(0, 7) && m.month <= to.slice(0, 7)).map((m) =>
      this.#measured(m),
    );
  }

  /**
   * A library with no device behind it has no minutes and no pages — anywhere,
   * at any grain. Applied at the seam rather than in a second copy of every
   * fixture, so the two libraries cannot drift about which months exist.
   */
  #measured<T extends { minutes: number | null; pages: number | null }>(row: T): T {
    return this.#device ? row : { ...row, minutes: null, pages: null };
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

  async openReflection(bookId: number, readingId: number | null = null): Promise<CreatedNoteDto> {
    return this.#openAnchored(bookId, 'reflection', readingId);
  }

  async openReview(bookId: number): Promise<CreatedNoteDto> {
    return this.#openAnchored(bookId, 'review', null);
  }

  /**
   * Open **or mint** — the engine's `open_anchored`, which is why it is one call.
   *
   * A named `readingId` narrows the search to that read, which is what makes a
   * moment about a *closed* reading open that reading's reflection rather than
   * the current one's. A reread has two, and picking wrong here would put
   * January's thoughts under the note for the read that is still open.
   */
  async #openAnchored(
    bookId: number,
    kind: NoteKindDto,
    readingId: number | null,
  ): Promise<CreatedNoteDto> {
    const existing = this.#notes.find(
      (n) =>
        n.book_id === bookId &&
        n.kind === kind &&
        (readingId === null || n.reading_id === readingId),
    );
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
      reading_id: readingId,
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

  /**
   * One entry per id asked, **in the order asked, empties and duplicates
   * included** — which is the contract, not a convenience.
   *
   * A fake that dropped the empties would make the caller's zip untestable at
   * the layer it is tested: the page holds a list of notes and a list of
   * passages and marks the second from the first, and a reply missing a row
   * silently shifts that alignment. `every_requested_note_gets_a_row_in_the_order_asked`
   * is the engine's half of this.
   *
   * The ids inside an entry are in **citation order here and reading order in
   * the engine** (`h.page ASC, h.ko_datetime ASC, h.id ASC`), and that is a
   * deliberate difference rather than an omission: the surface asking is
   * drawing set membership, so a fake reproducing the `ORDER BY` would be a
   * second implementation of a rule nothing above the seam may depend on.
   */
  async citationsForNotes(noteIds: number[]): Promise<NoteCitationsDto[]> {
    return noteIds.map((note_id) => ({
      note_id,
      highlight_ids: this.#citations.filter((c) => c.note === note_id).map((c) => c.highlight),
    }));
  }

  // ---- searching one book's marks (item 50) -------------------------------

  /**
   * A book's notes and passages in one list, near enough to draw with.
   *
   * **This is not fts5 and does not pretend to be.** It matches every token as
   * a case-insensitive substring, which is a coarser question than bm25 asks —
   * so what this fixture can be trusted for is the *shape* of the answer (two
   * kinds in one list, a snippet with markers around the terms, the scope, the
   * limit, the empty query) and never the *ranking*. A test asserting a
   * particular order here would be asserting this file's ordering rule, which
   * exists nowhere below the seam and is the one thing about the real method
   * that a frontend must never depend on.
   *
   * What it does copy exactly is the arithmetic a caller can get wrong:
   * `limit` is a hard ceiling and `0` or less is an **empty list**, an empty or
   * whitespace query is not a search and answers with nothing, and the merge
   * takes from the two kinds by within-source position so a book with one note
   * and thirty passages does not show a wall of one kind.
   */
  async searchMarks(
    query: string,
    bookId: number | null,
    limit: number,
    source: SearchSourceDto | null = null,
  ): Promise<SearchHitDto[]> {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0 || limit <= 0) return [];
    const hit = (fields: (string | null)[]): string | null => {
      // Every term, as fts5's implicit AND has it — and the snippet comes from
      // whichever field matched, which is `snippet(…, -1, …)`'s own behaviour
      // and the reason a caller may not assume it is the row's main text.
      const found = fields.find(
        (f): f is string => f !== null && terms.every((t) => f.toLowerCase().includes(t)),
      );
      return found === undefined ? null : mark(found, terms);
    };

    const notes = this.#notes
      .filter((n) => bookId === null || n.book_id === bookId)
      .map((note) => ({ note, snippet: hit([note.title, BODIES[note.id] ?? null]) }))
      .filter((x): x is { note: NoteDto; snippet: string } => x.snippet !== null)
      .map(({ note, snippet }): SearchHitDto => ({ kind: 'note', note, snippet }));

    const marks = (bookId === null ? Object.values(HIGHLIGHTS).flat() : (HIGHLIGHTS[bookId] ?? []))
      .map((highlight) => ({
        highlight,
        snippet: hit([highlight.text, highlight.ko_note, highlight.annotation]),
      }))
      .filter((x): x is { highlight: HighlightDto; snippet: string } => x.snippet !== null)
      .map(({ highlight, snippet }): SearchHitDto => ({ kind: 'highlight', highlight, snippet }));

    // The narrowing is the engine's `source` argument, copied: it selects which
    // index is read at all, so a scoped search spends its whole `limit` on the
    // kind that was asked for rather than filtering a mixed list afterwards.
    if (source === 'note') return notes.slice(0, limit);
    if (source === 'highlight') return marks.slice(0, limit);

    const merged: SearchHitDto[] = [];
    for (let i = 0; i < Math.max(notes.length, marks.length); i++) {
      // Bound before testing: `noUncheckedIndexedAccess` does not carry a
      // truthiness check on `notes[i]` across to a second `notes[i]`, because
      // `i` is a mutable loop variable and the index could have moved.
      const n = notes[i];
      const m = marks[i];
      if (n) merged.push(n);
      if (m) merged.push(m);
    }
    return merged.slice(0, limit);
  }

  // ---- flashcards (items 45, 49) ------------------------------------------

  /**
   * `true` created it, `false` means you already had it — and the existing card
   * is left **exactly** as it was.
   *
   * `ON CONFLICT(book_id, word) DO NOTHING`, restated: a second capture of the
   * same word must not repoint the card at a different passage or overwrite its
   * context. A fake that returned `true` twice, or that updated on the repeat,
   * would leave the confirmation's two faces rendered by nothing.
   *
   * The two refusals are modelled because the frontend deliberately does not
   * pre-validate the pair: a `highlightId` from another book is the engine's
   * `InvalidInput`, and the trim-then-empty check is its `InvalidInput` too.
   */
  async createFlashcard({
    bookId,
    word,
    highlightId = null,
    context = null,
  }: NewFlashcard): Promise<boolean> {
    // Trimmed **before** the uniqueness check, as `Engine::create_flashcard`
    // does: `" mot"` is not a second word.
    const clean = word.trim();
    if (clean === '') throw new Error('a flashcard needs a word');
    if (highlightId !== null) {
      const owner = Object.values(HIGHLIGHTS)
        .flat()
        .find((h) => h.id === highlightId);
      if (owner === undefined) throw new Error(`no highlight with id ${highlightId}`);
      if (owner.book_id !== bookId) {
        throw new Error(
          `highlight ${highlightId} belongs to book ${owner.book_id}, not book ${bookId}`,
        );
      }
    }
    if (this.#flashcards.some((c) => c.book_id === bookId && c.word === clean)) return false;
    const book = BOOKS.find((b) => b.id === bookId);
    if (book === undefined) throw new Error(`no book with id ${bookId}`);
    this.#flashcards.push({
      id: this.#nextCardId++,
      book_id: bookId,
      highlight_id: highlightId,
      word: clean,
      context,
      book_title: book.title ?? '',
      exported: false,
    });
    return true;
  }

  /** Pending and exported alike, which is what `list_flashcards_for_book` returns. */
  async listFlashcardsForBook(bookId: number): Promise<FlashcardDto[]> {
    return this.#flashcards.filter((c) => c.book_id === bookId).map((c) => ({ ...c }));
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

  // ---- the devices page (items 15a, 55) -----------------------------------

  async pairedDevices(): Promise<PairedDeviceDto[]> {
    // Newest-seen first, which is the engine's own order
    // (`COALESCE(last_seen_at, installed_at) DESC`). Stated here rather than
    // left to array order, because a screen that re-sorted would look right
    // against a fixture that happened to already be sorted.
    return (
      this.#paired
        .filter((d) => !this.#forgotten.has(d.device_id))
        // `in`, never `?? d.label`. A **cleared** name is stored as `null`, and
        // `??` cannot tell that apart from *no rename happened* — so the blank
        // that the engine deliberately stores as NULL would render as the old
        // name for ever. The same trap is one keystroke away in any client.
        .map((d) => ({
          ...d,
          label: d.device_id in this.#labels ? this.#labels[d.device_id]! : d.label,
        }))
        .sort((a, b) => (b.last_seen_at ?? b.installed_at) - (a.last_seen_at ?? a.installed_at))
    );
  }

  async candidateMounts(): Promise<string[]> {
    return this.#plugged ? [...MOUNTS] : [];
  }

  async pluginStatus(mount: string): Promise<PluginStatusDto> {
    const status = PLUGINS[mount];
    // The real engine refuses a path that is not a KOReader install, and it is
    // an *error* rather than an empty status — a screen that treated the two
    // the same would offer to install onto a USB stick.
    if (status === undefined) throw new Error(`${mount} is not a KOReader install`);
    return { ...status, ...(this.#installed[mount] ?? {}) };
  }

  async installPlugin(mount: string): Promise<InstallReportDto> {
    const status = await this.pluginStatus(mount);
    if (status.condition === 'obstructed') {
      throw new Error('readingbuddy will not write over a plugin it did not put here');
    }
    const deviceId = status.device_id ?? `fake${mount.length}0000000000`;
    this.#installed[mount] = {
      installed: true,
      installed_version: status.our_version,
      paired: true,
      device_id: deviceId,
      condition: 'current',
    };
    this.#forgotten.delete(deviceId);
    return {
      plugin_dir: status.plugin_dir,
      device_id: deviceId,
      version: status.our_version,
      written: ['_meta.lua', 'main.lua', 'manifest.lua', 'pairing.lua'],
      upgraded_from: status.installed ? status.installed_version : null,
    };
  }

  async uninstallPlugin(mount: string): Promise<UninstallReportDto> {
    const status = await this.pluginStatus(mount);
    this.#installed[mount] = {
      installed: false,
      installed_version: null,
      paired: false,
      device_id: null,
      condition: 'absent',
    };
    if (status.device_id !== null) this.#forgotten.add(status.device_id);
    return {
      plugin_dir: status.plugin_dir,
      removed: ['_meta.lua', 'main.lua', 'manifest.lua', 'pairing.lua'],
      forgot_device: status.device_id,
    };
  }

  async forgetDevice(deviceId: string): Promise<boolean> {
    if (this.#forgotten.has(deviceId)) return false;
    if (!this.#paired.some((d) => d.device_id === deviceId)) return false;
    this.#forgotten.add(deviceId);
    return true;
  }

  async renameDevice(deviceId: string, label: string): Promise<boolean> {
    if (!this.#paired.some((d) => d.device_id === deviceId)) return false;
    if (this.#forgotten.has(deviceId)) return false;
    // Blank **clears**, exactly as the engine does — a row holding `"   "` is
    // what a screen's fallback could not recover from.
    this.#labels[deviceId] = label.trim() === '' ? null : label.trim();
    return true;
  }

  async scanDevice(root: string): Promise<DeviceScanDto> {
    const scan = SCANS[root];
    if (scan === undefined) throw new Error(`${root} is not a KOReader install`);
    if (this.#synced.has(root)) {
      // After a sync, everything the scan found is here. `books` keeps its
      // length: *nothing new* is a device with books in an `unchanged` state,
      // and `books: []` is a different sentence.
      return {
        ...scan,
        books: scan.books.map((b) =>
          b.state.state === 'unreadable' ? b : { ...b, state: { state: 'unchanged' } },
        ),
        parsed: 0,
        cached: scan.books.length,
      };
    }
    return scan;
  }

  async syncMount(mount: string): Promise<MountSyncDto> {
    const scan = await this.scanDevice(mount);
    const syncable = scan.books.filter(
      (b) => b.state.state === 'new' || b.state.state === 'updated',
    );
    this.#synced.add(mount);
    const status = PLUGINS[mount];
    return {
      mount,
      // `null` when the mount is nobody's paired reader — the books still came
      // across, and only the stamp is skipped.
      device_id: status?.paired ? (status.device_id ?? null) : null,
      found: scan.books.length,
      synced: syncable.length,
      reports: syncable.map((b) => ({
        stats: {
          book_id: b.book_id ?? 0,
          book_title: b.title ?? 'Untitled',
          inserted: 4,
          updated: 1,
          skipped: 0,
          flashcards: 0,
          matched_by: b.book_id === null ? 'new' : 'md5',
          percent_finished: b.ko_percent,
          status: b.ko_status,
          rating: null,
        },
        warnings: [],
      })),
      warnings: scan.warnings,
    };
  }

  async importDeviceStatistics(_mount: string): Promise<StatsImportReportDto> {
    return {
      schema_version: 20221111,
      books_in_db: 42,
      books_matched: 12,
      days: 96,
      events: { inserted: 96, updated: 3 },
      warnings: [],
    };
  }

  // ---- calibre ------------------------------------------------------------

  async calibreStatus(): Promise<CalibreStatusDto> {
    return this.#calibre
      ? { ebook_convert: '/usr/bin/ebook-convert', calibredb: '/usr/bin/calibredb' }
      : // Both absent is a perfectly good answer and **not an error**: calibre
        // is feature-detected and nothing in this app asks anybody to install it.
        { ebook_convert: null, calibredb: null };
  }

  async calibreLibrary(): Promise<CalibreBookDto[]> {
    return [];
  }

  async importCalibreLibrary(options: { dryRun?: boolean } = {}): Promise<CalibreReportDto> {
    return {
      dry_run: options.dryRun ?? false,
      rows: 214,
      books: [],
      unmatched: [],
      warnings: [],
    };
  }
}

// ---------------------------------------------------------------------------
// The wall's filter and its three orders, as fixtures of `ReadingFilter` and
// `reading_order_by` (items 43, 47).
//
// These state the engine's clauses rather than re-deriving anything: every one
// of them is an equality or a comparison on a column that is **on the wire**,
// which is what separates them from `listBooks`' sort — deliberately ignored
// there, because ordering by a rule the engine owns would give this file a place
// that rule could be broken and still look tested. `finished_at DESC` is not a
// rule; it is the field. And a fake that ignored the filter would make the wall's
// own subject — the year picker — untestable at the two layers it is tested at.
// ---------------------------------------------------------------------------

/**
 * The one refusal, in the one place it lives.
 *
 * `ReadingFilter` is fallible where `BookFilter` is not, and only because of the
 * year: `DayRange` refuses an inverted span so a backwards range is an
 * `InvalidInput` from **both** doors rather than a confident empty wall. Stated
 * here so that property is observable at layers 1 and 2; nothing above the seam
 * carries a second copy of it, and nothing above the seam can construct one.
 */
function refuseInvertedSpan(filter: ReadingFilterDto | null): void {
  const span = filter?.finished_in;
  if (span && span.from > span.to) {
    throw new Error(`inverted day range: ${span.from} is after ${span.to}`);
  }
}

/** A unix second as the UTC day it fell on, which is the engine's convention. */
function dayOfUnix(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

function matchesReadingFilter(row: ReadingRow, filter: ReadingFilterDto | null): boolean {
  if (filter === null) return true;
  const r = row.reading;
  if (filter.book_id !== null && r.book_id !== filter.book_id) return false;
  if (filter.status !== null && r.status.state !== filter.status.state) return false;
  // `open` is `finished_at IS NULL` and is **not** redundant with `status`:
  // `abandon_reading` leaves the reading open, so *abandoned* and *reading* are
  // both open and a wall of finished reads cannot be written as a status.
  if (filter.open !== null && (r.finished_at === null) !== filter.open) return false;
  if (filter.finished_in !== null) {
    // An open reading finished in no year — the engine's clause is `finished_at
    // >= ? AND < ?` against the bare column and a NULL fails both comparisons.
    // The bounds are midnight of `from` to midnight after `to`, i.e. **both days
    // inclusive**, which on day strings is exactly this.
    if (r.finished_at === null) return false;
    const day = dayOfUnix(r.finished_at);
    if (day < filter.finished_in.from || day > filter.finished_in.to) return false;
  }
  return true;
}

/**
 * One of `reading_order_by`'s three clauses, as a comparator.
 *
 * All three are descending and all three end in `readings.id DESC`, which is the
 * tie-break that makes paging a total order — item 18 learned one table over
 * that a behavioural test cannot catch its absence, so it is stated here too
 * rather than left to insertion order.
 */
function readingOrder(sort: ReadingSortDto): (a: ReadingRow, b: ReadingRow) => number {
  const key = (row: ReadingRow): number | null => {
    const r = row.reading;
    switch (sort) {
      case 'finished':
        return r.finished_at;
      case 'started':
        // `COALESCE(started_at, created_at)` — the CSV has no start date and
        // `goodreads.rs` refuses to invent one, so the fallback is when we
        // learned of the read.
        return r.started_at ?? r.created_at;
      case 'last_modified':
        return r.last_modified;
      default:
        return null;
    }
  };
  return (a, b) => {
    const x = key(a);
    const y = key(b);
    // Open readings have no `finished_at` and land **last**, which is SQLite's
    // own ordering for NULLs under `DESC` and is where a read that has not ended
    // belongs on a list of reads that did.
    if (x === null && y === null) return b.reading.id - a.reading.id;
    if (x === null) return 1;
    if (y === null) return -1;
    return y - x || b.reading.id - a.reading.id;
  };
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
