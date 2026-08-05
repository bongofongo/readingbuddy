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

/** Every field, so a new DTO column shows up here as a type error. */
function book(id: number, over: Partial<BookDto>): StoredBook {
  return {
    id,
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
    cover_path: null,
    page_count: 300,
    description: null,
    first_sentence: null,
    subjects: [],
    series: null,
    series_index: null,
    current_page: null,
    finished: false,
    reading_status: null,
    date_started: null,
    date_finished: null,
    created_at: 1735689600,
    last_modified: 1735689600 + id,
    ...over,
  } as StoredBook;
}

/**
 * The hostile set, mirroring `devdb.rs`'s `edge_cases()`.
 *
 * Order matters for the screenshots: these are the first tiles in the grid, so a
 * reviewer sees the cases and not two hundred ordinary books.
 */
const BOOKS: StoredBook[] = [
  // page_count = 0 — item 17b's false denominator. Any percentage over this is a
  // divide by zero and the honest answer is that there is no percentage.
  book(1, { title: 'A Book Of Zero Pages', page_count: 0, reading_status: 'reading', current_page: 0 }),
  // page_count NULL — absence, not zero. A progress bar has nothing to draw.
  book(2, { title: 'A Book Of Unknown Length', page_count: null, reading_status: 'reading', current_page: 40 }),
  // The fat end of item 19's thickness scale.
  book(3, { title: 'The Doorstop', page_count: 1408, reading_status: 'reading', current_page: 500 }),
  // And the thin end, where a spine has no room for a title.
  book(4, { title: 'A Pamphlet', page_count: 48 }),
  // No cover. Not a broken image and not an apology.
  book(5, { title: 'A Book With No Cover At All', cover_path: null }),
  // 220 characters. Clipping, wrapping, and the whole title on the detail page.
  book(6, {
    title:
      'A Title Of Such Considerable And Frankly Self-Indulgent Length That It Cannot Possibly Fit In A Shelf Tile Or A Column Header, Being In The Manner Of The Long Eighteenth Century, Wherein The Title Was Also The Blurb',
  }),
  // `Surname, Given` — the calibre and Goodreads form. Displaying it the other
  // way round is author-name parsing, which is item 17's and not a template's.
  book(7, { title: 'A Book Filed Under Surname', authors: ['Borges, Jorge Luis'] }),
  // A mononym. Any name-splitting rule breaks here.
  book(8, { title: 'A Book By One Name', authors: ['Colette'] }),
  // No author at all — a real state after a bare epub import, not an error.
  book(9, { title: 'A Book By Nobody', authors: [] }),
  book(10, { title: 'A Book By Three People', authors: ['Ada Ordinary', 'Grace Second', 'Bea Third'] }),
  // An abandoned reading. Never styled as failure — and note it is `finished:
  // false` with a `current_page`, identical to the open read above, which is the
  // whole reason `reading_status` had to cross.
  book(11, { title: 'A Book I Put Down', reading_status: 'abandoned', current_page: 60 }),
  book(12, { title: 'A Book I Went Back To', reading_status: 'reading', current_page: 150 }),
  book(13, {
    title: 'A Book I Finished',
    reading_status: 'finished',
    finished: true,
    current_page: 300,
    isbn_13: '9780000000017',
  }),
  // CJK. Font fallback, and character-count clipping being wrong.
  book(14, { title: '北回帰線のあたりで', authors: ['村上 春樹'] }),
  // RTL. Bidi layout, and a left-aligned column that should not be.
  book(15, { title: 'الكتاب الذي يقرأ من اليمين', authors: ['ابن خلدون'] }),
  book(16, { title: "Ærøskøbing: Ångström's Œuvre — a Naïve Façade" }),
  // An empty title — the schema's own default, reachable from a sidecar with no
  // doc_props. `titleLabel` is what stops this rendering as a blank line.
  book(17, { title: null }),
  // `series_index` is a REAL and must print as `#2`, never `#2.0`.
  book(18, { title: 'The Claw Of The Conciliator', series: 'The Book Of The New Sun', series_index: 2 }),
  // A series naming no index. The pair moves together or not at all.
  book(19, { title: 'A Book In A Series With No Number', series: 'An Unnumbered Sequence' }),
  book(20, {
    title: 'A Book With Subjects And Shelves',
    subjects: ['Philosophy', 'Essays', 'Nineteenth century'],
    reading_status: 'finished',
    finished: true,
    description:
      'Provider subjects beside minted shelves — two different things that look alike, and one of the separations migration 0013 rests on.',
  }),
  // A status this build does not know. `reading_status` crosses as a string
  // precisely because an importer can write one.
  book(21, { title: 'A Book Some Other App Touched', reading_status: 'paused' }),
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
    reading(1, 12, { status: 'finished', finished_at: 1738368000, current_page: 300 }),
    reading(2, 12, { status: 'reading', current_page: 150 }),
  ],
};

function reading(id: number, bookId: number, over: Partial<ReadingDto>): ReadingDto {
  return {
    id,
    book_id: bookId,
    started_at: 1735689600,
    finished_at: null,
    status: 'reading',
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
    if (!b || b.reading_status === null) return [];
    return [reading(100 + bookId, bookId, { status: b.reading_status })];
  }

  /**
   * No cover in the fake, ever, and that is the honest answer rather than a gap:
   * layer 2 runs in a bare browser with no asset protocol, so any URL here would
   * render as a broken image. Every tile therefore exercises the **no cover**
   * path, which is the one that has to be a designed empty state — and the
   * covers themselves are checked in the real app, against `make dev-db`.
   */
  coverSrc(): string | null {
    return null;
  }
}
