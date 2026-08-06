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
  BookSortDto,
  HighlightDto,
  NoteDto,
  PathsDto,
  ReadingDto,
} from './bindings';
import type { LibraryClient, StoredBook } from './client';

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

const HIGHLIGHTS: Record<number, HighlightDto[]> = {
  3: [
    highlight(1, 3, 'The thing about a place is that it is still there when you are not.', {
      chapter: 'Chapter 4',
      page: 212,
      ko_note: 'What did they mean by this?',
    }),
    highlight(2, 3, 'What survives is not what was meant to.', { chapter: 'Chapter 9', page: 640 }),
    // No chapter and no page: KOReader does produce this, and the "where" line
    // must then render as nothing rather than as a stray separator.
    highlight(3, 3, 'She counted the bells and then stopped counting.', {}),
  ],
  11: [highlight(4, 11, 'It is a mistake to read a map as a promise.', { chapter: 'Chapter 2', page: 31 })],
};

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
    pos0: null,
    pos1: null,
    ko_datetime: '2025-03-04 09:12:00',
    ko_note: null,
    annotation: null,
    source: 'koreader',
    identity_hash: `fake-${id}`,
    created_at: 1735689600,
    ...over,
  } as HighlightDto;
}

const NOTES: Record<number, NoteDto[]> = {
  3: [
    {
      id: 1,
      book_id: 3,
      highlight_id: null,
      reading_id: null,
      file_path: '0001-the-doorstop.md',
      title: 'On The Doorstop',
      kind: 'note',
      page: null,
      location: null,
      created_at: 1735776000,
      last_modified: 1735776000,
    } as NoteDto,
  ],
};

const READINGS: Record<number, ReadingDto[]> = {
  // Two readings, one closed and one open. Item 28's card is per reading, so
  // this book has two of them.
  12: [
    reading(1, 12, { status: { state: 'finished' }, finished_at: 1738368000, current_page: 300 }),
    reading(2, 12, { status: { state: 'reading' }, current_page: 150 }),
  ],
};

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
    ...over,
  } as ReadingDto;
}

export class FakeClient implements LibraryClient {
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

  async getBook(id: number): Promise<StoredBook | null> {
    return BOOKS.find((b) => b.id === id) ?? null;
  }

  async listHighlights(bookId: number): Promise<HighlightDto[]> {
    return HIGHLIGHTS[bookId] ?? [];
  }

  async listNotes(bookId: number | null): Promise<NoteDto[]> {
    if (bookId === null) return Object.values(NOTES).flat();
    return NOTES[bookId] ?? [];
  }

  async listReadings(bookId: number): Promise<ReadingDto[]> {
    const own = READINGS[bookId];
    if (own) return own;
    const b = BOOKS.find((x) => x.id === bookId);
    if (!b || b.reading_state === null) return [];
    return [reading(100 + bookId, bookId, { status: b.reading_state })];
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
}
