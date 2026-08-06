/**
 * The client seam — one interface, two implementations, injected.
 *
 * `docs/gui/testing.md` decided this shape and the reason is worth restating:
 * Tauri's own `mockIPC` matches on a **command-name string**, so a renamed
 * command breaks the app while every test that mocks it keeps passing. That is
 * exactly the untyped drift the generated types exist to abolish. So the *broad*
 * tool is this interface — the real client and the fake share it, and a drifted
 * field is a `tsc` error in the fake — and `mockIPC` stays the *narrow* one, used
 * in `tauri.test.ts` to check that the real client invokes the right command with
 * the right arguments, which is the only place a command-name string belongs.
 *
 * It also gives layer 2 its browser. Playwright drives the Vite dev server, where
 * there is no Tauri IPC at all; [`client`] hands it the fake, so a route
 * renders and can be screenshotted without a webview or a driver — the same
 * arrangement that makes the TUI suite headless.
 *
 * Three shapes of the wire this file is the only place to know about:
 *
 * 1. **`Response` is shaped, not named.** Ten requests answer `{shape:'books'}`,
 *    because a reply is already tied to its call by `Call.id`. Narrowing is a
 *    runtime check, so it happens once, here, and never at a call site.
 * 2. **`BookDto.id` is `number | null`** — the same struct is the input to
 *    `save_book` and the carrier for unsaved provider candidates. Anything out of
 *    `list_books`/`get_book` has an id, and [`StoredBook`] says so once instead of
 *    `book.id!` in twelve components.
 * 3. **`ErrorCode` is exhaustive in TypeScript and open on the wire.** ts-rs drops
 *    `#[serde(other)]`, which is what makes an unknown code from a newer build
 *    degrade rather than fail to parse. Keep a default arm tsc thinks is dead.
 */
import { convertFileSrc, invoke } from '@tauri-apps/api/core';

import { FakeClient } from './fake';
import type {
  ActivitySummaryDto,
  ApiError,
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
  Reply,
  Request,
  Response,
  TableOfContentsDto,
} from './bindings';

/** A book that came out of the library, so it has an id. See the module doc. */
export type StoredBook = BookDto & { id: number };

/**
 * An open reading, with the book it is of.
 *
 * `OpenReadingDto` narrowed the way [`StoredBook`] narrows `BookDto`: anything
 * the library hands back has an id. The **reading** travels beside the book on
 * purpose and this is not redundancy — a reread has two readings of one book,
 * so "which read is this" is a question the book alone cannot answer, and item
 * 28's card is minted per *reading*.
 */
export type OpenReading = { book: StoredBook; reading: ReadingDto };

/**
 * Everything a screen may ask for.
 *
 * Grows one method per thing a screen needs — not one per API request. A request
 * with no screen behind it does not belong here, and a screen that wants
 * something this interface cannot express is an `api-surface-auditor` question
 * before it is a code question.
 */
export interface LibraryClient {
  paths(): Promise<PathsDto>;
  listBooks(limit?: number, sort?: BookSortDto): Promise<StoredBook[]>;
  /**
   * The books with an open reading — what the shelf pulls proud (item 26).
   *
   * A **request**, not a filter over `listBooks`. The engine owns selection
   * predicates (item 17), and the two spellings are not equivalent: this
   * returns the reading, and `reading_state` on a book row cannot say *which*
   * of a reread's two readings is the open one.
   */
  currentlyReading(limit?: number): Promise<OpenReading[]>;
  getBook(id: number): Promise<StoredBook | null>;
  listHighlights(bookId: number): Promise<HighlightDto[]>;
  listNotes(bookId: number | null): Promise<NoteDto[]>;
  listReadings(bookId: number): Promise<ReadingDto[]>;

  // ---- one card, per reading (item 28) ------------------------------------

  /**
   * The passages captured during **one read**, not the book's.
   *
   * A reread has two, and `HighlightDto.reading_id` is `null` for a mark the
   * dates could not place — so this is a narrower list than `listHighlights`
   * and never a rearrangement of it.
   */
  highlightsForReading(readingId: number): Promise<HighlightDto[]>;
  /**
   * The one passage a card carries — **the engine's choice, not `[0]`** (item 44).
   *
   * Which passage is a *selection predicate*, and item 17 puts those below the
   * seam. The rule is longest-then-lowest-id and it is deliberately not restated
   * here: a frontend spelling it would be the day the TUI grows a card and the
   * two apps show a different sentence for the same reading, with neither
   * looking wrong.
   *
   * `null` is ordinary. A reading whose marks are all unattributed has no
   * passage of its own, exactly as `highlightsForReading` returns an empty list,
   * and a card draws that absence as an absence.
   *
   * One call per card. That is right for a card reached by selecting a book and
   * **wrong for a wall of cards across the library** — entry 44 says so in as
   * many words, and that wall wants item 43 before it wants N of these.
   */
  cardPassage(readingId: number): Promise<HighlightDto | null>;
  /**
   * The notes belonging to **one read**, which arrived with item 40.
   *
   * A separate method rather than a second argument to [`listNotes`], because
   * the two scopes are mutually exclusive **on the wire** — a reading belongs to
   * one book, so naming both is redundant when they agree and an `InvalidInput`
   * when they do not. Two methods make the pair unrepresentable at a call site
   * rather than merely refused after it.
   */
  notesForReading(readingId: number): Promise<NoteDto[]>;

  // ---- moments (item 23) --------------------------------------------------

  /**
   * What is worth noticing and has not been shown yet, newest first.
   *
   * **Polled, never subscribed**: this protocol has no push channel, and
   * `Api::pending_moments` says polling is safe because a moment is derived on
   * every call and stored nowhere.
   *
   * `limit` takes from the newest end and is the only lever there is. There is
   * no count on this surface and there must never be one — `MomentDto`'s own doc
   * refuses `pending: 3` on the wire by name, and a `.length` rendered here
   * would be the same badge one layer up.
   */
  pendingMoments(limit?: number): Promise<MomentDto[]>;
  /**
   * Record that a moment was surfaced. **Idempotent**, through both doors.
   *
   * It deliberately does not re-derive: `run_ended` depends on the clock, so a
   * moment that was pending when it was shown can stop being pending a moment
   * later, and refusing the acknowledgement then would replay it for ever.
   */
  acknowledgeMoment(id: string): Promise<void>;

  // ---- the reading life (items 21, 31, 42) --------------------------------

  /**
   * What is known about a period. Both days inclusive, `YYYY-MM-DD`, UTC.
   *
   * `minutes` and `pages` are `null` for *nothing measured* and are **not
   * zero** — a reader whose library came from a Goodreads CSV has no device
   * data anywhere, and a page that renders that as `0` is telling them they
   * read for no time at all.
   */
  activitySummary(from: string, to: string): Promise<ActivitySummaryDto>;
  /**
   * The months of a period that carry an event, oldest first (item 42).
   *
   * **Never bucket [`activitySummary`]'s daily sibling into months here.**
   * `minutes: null` collapses to `0` on the first `reduce` in any language that
   * has one, which is the lie spread over a calendar; and `books` is distinct
   * over the whole month, so it cannot be recovered from the days at all — a
   * reader who opened the same two books on twelve days read two books, not
   * twenty-four. That is a semantic claim about what a month means and item 17
   * puts those below the seam, which is the entire reason this request exists.
   */
  activityByMonth(from: string, to: string): Promise<MonthActivityDto[]>;

  // ---- what is behind one book (item 27) ---------------------------------

  /** Minted shelves out of `book_tags` — **not** `BookDto.subjects`. */
  bookTags(bookId: number): Promise<BookTagDto[]>;
  /** The ebook files this library owns for the book. */
  bookFiles(bookId: number): Promise<BookFileDto[]>;
  /**
   * Who said what about this book, and when (item 29).
   *
   * An **empty list is ordinary**: every book predating migration `0012`
   * reports one however well populated it is, which is why a screen must
   * render absence as *unattributed* and never as "unknown provider".
   */
  fieldProvenance(bookId: number): Promise<FieldSourceDto[]>;
  /**
   * The chapter list, read out of an owned file on every call.
   *
   * `null` and `{ entries: [] }` are **different answers** — no file here we
   * can read, against an EPUB that carries no TOC — and `TableOfContentsDto`
   * says in as many words that a client collapsing them tells its reader the
   * same thing about a missing file and an ordinary book. Loaded on demand
   * rather than with the page, because the engine derives it from the file
   * each time and stores nothing.
   */
  tableOfContents(bookId: number): Promise<TableOfContentsDto | null>;

  // ---- the reader's own marks --------------------------------------------

  /**
   * The reader's annotation on a passage — **ours**, and the only field of a
   * highlight an import never touches. `ko_note` beside it is the device's and
   * is rewritten toward the device on every pull; that split is the whole of
   * `docs/decisions.md`'s highlight-ownership seam, and this is the writer for
   * our half.
   */
  setAnnotation(highlightId: number, annotation: string | null): Promise<void>;

  // ---- notes --------------------------------------------------------------

  getNote(id: number): Promise<NoteDto | null>;
  /** The markdown body. Not on `NoteDto`: `notes` has no body column. */
  noteBody(noteId: number): Promise<string>;
  /** Rewrites the file, and reindexes both the FTS row and the wikilink edges. */
  updateNoteBody(noteId: number, body: string): Promise<void>;
  createNote(note: NewNoteDto): Promise<CreatedNoteDto>;
  deleteNote(noteId: number): Promise<void>;
  /**
   * Open (or mint) this book's reflection or review.
   *
   * `readingId` is **omitted by every caller that does not have one**, and that
   * is still item 27's ruling rather than a hole in it: a reread has two reads,
   * picking one is a decision, the engine's `open_anchored` already makes it
   * from the reading state, and a frontend guessing would be item 17's finding
   * with a new coat on.
   *
   * Item 28 is the one caller that has one **without guessing**. `MomentDto`
   * carries `reading_id` beside `book_id` precisely because a card is minted per
   * reading and a moment identified by its book cannot select between two —
   * entry 23 records that item 28's audit asked for the field. Passing the
   * engine's own answer back is relaying, not choosing, which is why this
   * argument is optional rather than required.
   *
   * Returns a [`CreatedNoteDto`], which is what a *creating* caller wants — an
   * id, a title and a path. Everything that then edits the note wants a
   * `NoteDto`, and `Engine::open_reflection_record` exists for exactly that and
   * **is not on the wire**, so the screen pays one `getNote` afterwards. That
   * is one extra request rather than a re-derivation, which is the right side
   * of the seam to be on; the DTO is the cheaper fix and belongs to whoever
   * owns `crates/api`.
   */
  openReflection(bookId: number, readingId?: number | null): Promise<CreatedNoteDto>;
  openReview(bookId: number): Promise<CreatedNoteDto>;
  /** This reading's note of that kind, when it has one. */
  noteForReading(readingId: number, kind: NoteKindDto): Promise<NoteDto | null>;

  // ---- the graph ----------------------------------------------------------

  /**
   * The `[[wikilinks]]` this note writes.
   *
   * `note: null` is a **forward reference**, not an error — it resolves itself
   * the day that note is written — so a pane shows it as text rather than
   * dropping it.
   */
  outgoingLinks(noteId: number): Promise<OutgoingLinkDto[]>;
  /** The notes that link *here*. */
  backlinks(noteId: number): Promise<NoteDto[]>;

  // ---- citations ----------------------------------------------------------

  /**
   * Tie a note to a passage, **by reference**: the citation survives a device
   * refresh rewriting the highlight's device-owned fields.
   */
  cite(noteId: number, highlightId: number): Promise<void>;
  uncite(noteId: number, highlightId: number): Promise<boolean>;
  /** Which passages one note cites. One call per note, so ask it for one. */
  citationsFor(noteId: number): Promise<HighlightDto[]>;

  // ---- the rating on a review --------------------------------------------

  /** The scale a bare number would otherwise be unreadable against. */
  activeRatingScale(): Promise<RatingScaleDto | null>;
  /** A rating belongs to a **review note**, never to a book. */
  reviewRating(noteId: number): Promise<RatingDto | null>;
  setRating(noteId: number, value: number): Promise<void>;
  clearReviewRating(noteId: number): Promise<boolean>;

  // ---- covers -------------------------------------------------------------

  /** A cover as a URL this environment can actually load, or null. */
  coverSrc(book: BookDto): string | null;
  /**
   * The **hero shot** — `cover_path`, the largest jacket a provider published.
   *
   * Not [`coverSrc`], which is the shelf tier: a grid of sixty tiles must not
   * load sixty full-size jackets, and a detail view showing one book wants the
   * file that has not been downscaled. Two methods rather than a boolean, so a
   * call site says which it meant.
   */
  heroSrc(book: BookDto): string | null;
}

export class ApiCallFailed extends Error {
  constructor(readonly error: ApiError) {
    super(error.message);
    this.name = 'ApiCallFailed';
  }
}

/** The reply said something true, but not the shape this call needed. */
export class UnexpectedShape extends Error {
  constructor(want: string, got: string) {
    super(`expected a ${want} reply, got ${got}`);
    this.name = 'UnexpectedShape';
  }
}

function expect<S extends Response['shape']>(
  r: Response,
  shape: S,
): Extract<Response, { shape: S }> {
  if (r.shape !== shape) throw new UnexpectedShape(shape, r.shape);
  return r as Extract<Response, { shape: S }>;
}

/**
 * The real client: one `invoke`, over the one Tauri command.
 *
 * `@tauri-apps/api/core` is imported statically and that is safe in a bare
 * browser: the module only *defines* `invoke`, which throws when called without
 * IPC. Nothing here calls it at load time, so layer 2 loads this file, uses the
 * fake, and never touches it.
 */
export class TauriClient implements LibraryClient {
  /**
   * Monotonic per session. The id exists so a transport can match a reply to its
   * call; over Tauri's IPC the await already does that, but sending a distinct
   * one keeps this arm honest about the vocabulary a socket arm will need — and
   * it is what the daemon logs.
   */
  #nextId = 1;

  async #call(request: Request): Promise<Response> {
    const reply = await invoke<Reply>('api_call', {
      call: { id: this.#nextId++, request },
    });
    if (reply.outcome.status === 'error') throw new ApiCallFailed(reply.outcome.error);
    return reply.outcome.response;
  }

  async paths(): Promise<PathsDto> {
    return expect(await this.#call({ method: 'paths' }), 'where').value;
  }

  /**
   * `offset` and `filter` reached the wire with item 18 and are passed at their
   * do-nothing values here on purpose: this interface grows one method per thing
   * a *screen* needs, and no screen pages or filters yet. The shelf (item 26) is
   * what widens the signature — the request already answers it.
   */
  async listBooks(limit = 200, sort: BookSortDto = 'last_modified'): Promise<StoredBook[]> {
    const r = expect(
      await this.#call({ method: 'list_books', params: { limit, sort, offset: 0, filter: null } }),
      'books',
    );
    // Every row from the library has an id. Narrowed once, here.
    return r.value as StoredBook[];
  }

  /**
   * `limit` is a cap on a strip, not a page of one. A shelf pulls a handful of
   * books proud; a user reading forty at once gets the first twelve and the
   * rest stay in the shelf below, which is where they already were.
   */
  async currentlyReading(limit = 12): Promise<OpenReading[]> {
    const r = expect(
      await this.#call({ method: 'currently_reading', params: { limit } }),
      'open_readings',
    );
    return r.value as OpenReading[];
  }

  async getBook(id: number): Promise<StoredBook | null> {
    const r = expect(await this.#call({ method: 'get_book', params: { id } }), 'book');
    return r.value === null ? null : (r.value as StoredBook);
  }

  async listHighlights(bookId: number): Promise<HighlightDto[]> {
    return expect(
      await this.#call({ method: 'list_highlights', params: { book_id: bookId } }),
      'highlights',
    ).value;
  }

  /**
   * `limit: null` is every note, which is what this call has always meant. The
   * book detail screen shows a book's whole note list, so a cap here would be a
   * cap on the page rather than a page of it.
   *
   * `reading_id: null` arrived with item 40 and is stated rather than omitted:
   * the two scopes are **mutually exclusive on the wire** — a reading belongs to
   * one book, so naming both is redundant when they agree and an error when they
   * do not, and `Api::list_notes` refuses the pair rather than preferring one.
   * `#[serde(default)]` makes an old *payload* parse, but `ts-rs` emits the
   * field as required regardless, so this is a `tsc` error rather than a
   * silently-omitted key. That is the seam behaving correctly.
   *
   * Scoping to a reading is item 28's — a card's notes are per read, and a
   * reread has two.
   */
  async listNotes(bookId: number | null): Promise<NoteDto[]> {
    return expect(
      await this.#call({
        method: 'list_notes',
        params: { book_id: bookId, reading_id: null, limit: null },
      }),
      'notes',
    ).value;
  }

  async listReadings(bookId: number): Promise<ReadingDto[]> {
    return expect(
      await this.#call({ method: 'list_readings', params: { book_id: bookId } }),
      'readings',
    ).value;
  }

  // ---- item 28 ------------------------------------------------------------

  async highlightsForReading(readingId: number): Promise<HighlightDto[]> {
    return expect(
      await this.#call({ method: 'highlights_for_reading', params: { reading_id: readingId } }),
      'highlights',
    ).value;
  }

  async cardPassage(readingId: number): Promise<HighlightDto | null> {
    return expect(
      await this.#call({ method: 'card_passage', params: { reading_id: readingId } }),
      'highlight',
    ).value;
  }

  async notesForReading(readingId: number): Promise<NoteDto[]> {
    // `book_id: null` beside a `reading_id`, never both — see the interface.
    return expect(
      await this.#call({
        method: 'list_notes',
        params: { book_id: null, reading_id: readingId, limit: null },
      }),
      'notes',
    ).value;
  }

  async pendingMoments(limit = 1): Promise<MomentDto[]> {
    return expect(await this.#call({ method: 'pending_moments', params: { limit } }), 'moments')
      .value;
  }

  async acknowledgeMoment(id: string): Promise<void> {
    expect(await this.#call({ method: 'acknowledge_moment', params: { id } }), 'unit');
  }

  async activitySummary(from: string, to: string): Promise<ActivitySummaryDto> {
    return expect(
      await this.#call({ method: 'activity_summary', params: { from, to } }),
      'activity_summary',
    ).value;
  }

  async activityByMonth(from: string, to: string): Promise<MonthActivityDto[]> {
    return expect(
      await this.#call({ method: 'activity_by_month', params: { from, to } }),
      'activity_by_month',
    ).value;
  }

  /**
   * `cover_path` is a **whole path** — the engine stores `images_dir.join(name)`
   * — so this must not join it with `images_dir` again, which would double the
   * prefix. It is absolute because the Tauri backend roots the engine at an
   * absolute data dir on purpose, and the asset protocol is scoped to that
   * directory at startup.
   *
   * Through `convertFileSrc` rather than a hand-spelled `asset://` URL, because
   * the scheme differs by platform (`http://asset.localhost` on Windows) and a
   * literal here would be a second copy of Tauri's own rule.
   */
  /**
   * **`cover_shelf_path`, not `cover_path`** (item 20c, corrected in item 38).
   *
   * Providers are now asked for the largest jacket they publish, so `cover_path`
   * is a hero shot and a shelf of sixty tiles would load sixty of them. The
   * shelf tier is the downscaled sibling where one exists and the original where
   * it does not, and **the engine decides which** — a frontend that picked would
   * have to know `THUMB_MAX`, and one reading `cover_thumb_path` itself shows
   * nothing for every cover small enough to have no tier. `make dev-db`'s covers
   * are 240×360, i.e. exactly that case, so this is the ordinary path and not
   * the exotic one.
   *
   * This read `cover_path` from the scaffold until item 38, which is a bug no
   * screenshot could show: with a dev library it renders the identical file.
   */
  coverSrc(book: BookDto): string | null {
    return book.cover_shelf_path ? convertFileSrc(book.cover_shelf_path) : null;
  }

  /** `cover_path`, for the one screen that shows one book. See the interface. */
  heroSrc(book: BookDto): string | null {
    return book.cover_path ? convertFileSrc(book.cover_path) : null;
  }

  // ---- item 27 ------------------------------------------------------------

  async bookTags(bookId: number): Promise<BookTagDto[]> {
    return expect(await this.#call({ method: 'book_tags', params: { book_id: bookId } }), 'book_tags')
      .value;
  }

  async bookFiles(bookId: number): Promise<BookFileDto[]> {
    return expect(
      await this.#call({ method: 'book_files', params: { book_id: bookId } }),
      'book_files',
    ).value;
  }

  async fieldProvenance(bookId: number): Promise<FieldSourceDto[]> {
    return expect(
      await this.#call({ method: 'field_provenance', params: { book_id: bookId } }),
      'field_provenance',
    ).value;
  }

  async tableOfContents(bookId: number): Promise<TableOfContentsDto | null> {
    return expect(
      await this.#call({ method: 'table_of_contents', params: { book_id: bookId } }),
      'table_of_contents',
    ).value;
  }

  async setAnnotation(highlightId: number, annotation: string | null): Promise<void> {
    expect(
      await this.#call({ method: 'set_annotation', params: { highlight_id: highlightId, annotation } }),
      'unit',
    );
  }

  async getNote(id: number): Promise<NoteDto | null> {
    return expect(await this.#call({ method: 'get_note', params: { id } }), 'note').value;
  }

  async noteBody(noteId: number): Promise<string> {
    return expect(await this.#call({ method: 'note_body', params: { note_id: noteId } }), 'text')
      .value;
  }

  async updateNoteBody(noteId: number, body: string): Promise<void> {
    expect(await this.#call({ method: 'update_note_body', params: { note_id: noteId, body } }), 'unit');
  }

  async createNote(note: NewNoteDto): Promise<CreatedNoteDto> {
    return expect(await this.#call({ method: 'create_note', params: { note } }), 'created_note')
      .value;
  }

  async deleteNote(noteId: number): Promise<void> {
    expect(await this.#call({ method: 'delete_note', params: { note_id: noteId } }), 'unit');
  }

  async openReflection(bookId: number, readingId: number | null = null): Promise<CreatedNoteDto> {
    return expect(
      await this.#call({
        method: 'open_reflection',
        params: { book_id: bookId, reading_id: readingId },
      }),
      'created_note',
    ).value;
  }

  async openReview(bookId: number): Promise<CreatedNoteDto> {
    return expect(
      await this.#call({ method: 'open_review', params: { book_id: bookId, reading_id: null } }),
      'created_note',
    ).value;
  }

  async noteForReading(readingId: number, kind: NoteKindDto): Promise<NoteDto | null> {
    return expect(
      await this.#call({ method: 'note_for_reading', params: { reading_id: readingId, kind } }),
      'note',
    ).value;
  }

  async outgoingLinks(noteId: number): Promise<OutgoingLinkDto[]> {
    return expect(await this.#call({ method: 'outgoing_links', params: { note_id: noteId } }), 'links')
      .value;
  }

  async backlinks(noteId: number): Promise<NoteDto[]> {
    return expect(await this.#call({ method: 'backlinks', params: { note_id: noteId } }), 'notes')
      .value;
  }

  async cite(noteId: number, highlightId: number): Promise<void> {
    expect(
      await this.#call({ method: 'cite', params: { note_id: noteId, highlight_id: highlightId } }),
      'unit',
    );
  }

  async uncite(noteId: number, highlightId: number): Promise<boolean> {
    return expect(
      await this.#call({ method: 'uncite', params: { note_id: noteId, highlight_id: highlightId } }),
      'bool',
    ).value;
  }

  async citationsFor(noteId: number): Promise<HighlightDto[]> {
    return expect(
      await this.#call({ method: 'citations_for', params: { note_id: noteId } }),
      'highlights',
    ).value;
  }

  async activeRatingScale(): Promise<RatingScaleDto | null> {
    return expect(await this.#call({ method: 'active_rating_scale' }), 'rating_scale').value;
  }

  async reviewRating(noteId: number): Promise<RatingDto | null> {
    return expect(await this.#call({ method: 'review_rating', params: { note_id: noteId } }), 'rating')
      .value;
  }

  async setRating(noteId: number, value: number): Promise<void> {
    expect(await this.#call({ method: 'set_rating', params: { note_id: noteId, value } }), 'unit');
  }

  async clearReviewRating(noteId: number): Promise<boolean> {
    return expect(
      await this.#call({ method: 'clear_review_rating', params: { note_id: noteId } }),
      'bool',
    ).value;
  }
}

// ---- injection ------------------------------------------------------------

let current: LibraryClient | null = null;

/** Layer 1 and layer 2 call this. Nothing in the app does. */
export function setClient(c: LibraryClient | null): void {
  current = c;
}

/**
 * Whether real IPC exists.
 *
 * Tauri 2 exposes `__TAURI_INTERNALS__` on the window; `invoke` throws without
 * it. Testing for it is what lets one build serve both the webview and a bare
 * browser, which is layer 2's whole premise.
 */
export function hasTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * The client this environment should use.
 *
 * Deliberately **not** silently falling back to the fake inside a webview: if
 * `hasTauri()` is true the real client is used and its failures surface, because
 * a GUI that quietly rendered fixture data when the engine failed to open would
 * be the worst possible bug — it would look like a working app showing somebody
 * else's library.
 */
export function client(): LibraryClient {
  if (current) return current;
  current = hasTauri() ? new TauriClient() : new FakeClient();
  return current;
}

