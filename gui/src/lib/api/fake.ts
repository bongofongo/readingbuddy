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
  ActivitySummaryDto,
  BookDto,
  BookFileDto,
  BookSortDto,
  BookTagDto,
  CreatedNoteDto,
  FieldSourceDto,
  HighlightDto,
  MomentDto,
  MonthActivityDto,
  NewNoteDto,
  NoteDto,
  NoteKindDto,
  OutgoingLinkDto,
  PathsDto,
  RatingDto,
  RatingScaleDto,
  ReadingDto,
  ReadingFilterDto,
  ReadingSortDto,
  TableOfContentsDto,
} from './bindings';
import type { LibraryClient, OpenReading, ReadingPage, ReadingRow, StoredBook } from './client';

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
  11: [highlight(4, 11, 'It is a mistake to read a map as a promise.', { chapter: 'Chapter 2', page: 31 })],
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
    highlight(
      13,
      20,
      'A shelf is an argument about what a person intends to have been.',
      { chapter: 'On Shelving', page: 44, reading_id: 120 },
    ),
    highlight(14, 20, 'Everything else is filing.', { chapter: 'On Shelving', page: 45, reading_id: 120 }),
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
   * is a real property of a real vault and is why `NotePane`'s plain notes (1–3)
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
  /** Moments already handed back. Acknowledging is idempotent, so this is a set. */
  #surfaced = new Set<string>();
  #device: boolean;

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
  constructor(options: { device?: boolean } = {}) {
    this.#device = options.device ?? true;
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

  // ---- one card, per reading (item 28) ------------------------------------

  async highlightsForReading(readingId: number): Promise<HighlightDto[]> {
    // Scoped to the read, not to the book — and the reader's own annotation is
    // instance state here for `listHighlights`' reason.
    return Object.values(HIGHLIGHTS)
      .flat()
      .filter((h) => h.reading_id === readingId)
      .map((h) => (h.id in this.#annotations ? { ...h, annotation: this.#annotations[h.id] ?? null } : h));
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
        n.book_id === bookId && n.kind === kind && (readingId === null || n.reading_id === readingId),
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
