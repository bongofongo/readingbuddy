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
  FlashcardDto,
  HighlightDto,
  MomentDto,
  MonthActivityDto,
  NewNoteDto,
  NoteCitationsDto,
  NoteDto,
  NoteKindDto,
  OutgoingLinkDto,
  PathsDto,
  RatingDto,
  RatingScaleDto,
  ReadingDto,
  ReadingFilterDto,
  ReadingRowDto,
  ReadingSortDto,
  ReadingYearsDto,
  Reply,
  Request,
  Response,
  SearchHitDto,
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
 * One row of the library's readings, with its book narrowed (item 43).
 *
 * [`StoredBook`]'s narrowing applied to `ReadingRowDto.book` for
 * [`OpenReading`]'s reason: anything the library hands back has an id, and
 * `book.id!` in a card's `href` is the alternative. Done once, in
 * [`TauriClient.listReadingRows`], rather than at every call site.
 *
 * Everything else is the DTO's own: `read_number` and `of_reads` are `i64` and
 * **neither is an `Option`** (item 41 has the argument), and `passage` is item
 * 44's chosen highlight or `null` for a read whose marks are all unattributed.
 */
export type ReadingRow = Omit<ReadingRowDto, 'book'> & { book: StoredBook };

/**
 * Which page of which readings, in what order.
 *
 * **An object rather than four positional arguments, and the API's own doc
 * gives the reason**: `ReadingQueryDto` says `limit` and `offset` "are both
 * `i64` and adjacent, which is a swap no type checker catches". Naming them at
 * the call site is what makes that swap impossible here too.
 *
 * The defaults are the do-nothing ones except `limit`, which has **no** default
 * on the wire on purpose — `0` is a real limit meaning a page of nothing, and
 * an omitted one would silently be that. A caller wanting everything says `-1`,
 * which is SQLite's own reading of `LIMIT -1`; this interface repeats that
 * rather than inventing a friendlier spelling of it.
 */
export type ReadingPage = {
  limit: number;
  sort?: ReadingSortDto;
  offset?: number;
  filter?: ReadingFilterDto | null;
};

/**
 * A card to capture — **an object, for [`ReadingPage`]'s reason**.
 *
 * `word` and `context` are adjacent and both string-shaped, so passing them the
 * wrong way round type-checks whenever the context is present: exactly the swap
 * `ReadingQueryDto` names `limit`/`offset` for. Naming them at the call site is
 * what makes it impossible rather than merely unlikely.
 *
 * `highlightId` and `context` default to `null` here and are **stated** on the
 * wire. `#[serde(default)]` in the Rust makes an old payload parse; `ts-rs`
 * emits the field as required regardless, and that is the seam behaving
 * correctly rather than a quirk to work around.
 */
export type NewFlashcard = {
  bookId: number;
  word: string;
  /** The passage the word came from. Re-read server-side against `bookId`. */
  highlightId?: number | null;
  context?: string | null;
};

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

  // ---- the wall of cards (items 43, 41, 47) -------------------------------

  /**
   * A page of the library's readings, each row carrying what one card draws.
   *
   * **This is the call that retires [`cardPassage`]'s N+1.** Entry 44 wrote the
   * pathology down in advance — one `CardPassage` per card is right for a card
   * reached by selecting a book and wrong for a wall across the library — and
   * item 43 minted this row so the passage rides the list instead. The engine
   * issues two statements per page whatever the page size, and the passage it
   * picks comes from the same `card_passage_order` a single `cardPassage` uses,
   * so the wall and one card cannot show different sentences for one reading.
   *
   * **It is not a card**, and the test of that is what would be added next: a
   * card would grow the rating and this will not (item 43). So a card drawn
   * from a row alone carries no rating and no note list — see `Card.svelte`'s
   * `detail` prop, and `docs/decisions.md` entry 47 for why that is the right
   * side of the trade rather than a shortfall.
   *
   * **`read_number` counts over every reading of the book, never over the
   * page.** A wall filtered to one year holds a reread's second read without
   * its first and still calls it the second — the engine uses correlated
   * subqueries rather than a window function precisely so that holds.
   *
   * Fallible where the other list calls are not: `filter.finished_in` is a
   * `DayRangeDto` the engine validates, so an inverted span is an
   * `InvalidInput` from **both** doors rather than a confident empty wall. A
   * year picker cannot produce one, which is exactly why there is no second
   * validation dialect above this seam.
   */
  listReadingRows(page: ReadingPage): Promise<ReadingRow[]>;
  /**
   * How many readings a filter matched — **its own request** (item 18).
   *
   * Not a field beside the rows, and a wall is the case that ruling was made
   * for: a count is a property of the *filter*, asked once when the reader
   * picks a year, while the page is asked again on every move through it.
   *
   * It takes the filter and deliberately not the query — a count has no page
   * and no order. Hand it the **same object** the page was asked with; the
   * engine builds both clauses from one `ReadingFilter::predicate`, so any
   * disagreement between the two numbers would be the caller's.
   */
  countReadings(filter: ReadingFilterDto | null): Promise<number>;
  /**
   * Which years this filter's readings **ended** in, newest first, and whether
   * any of them is still open (item 51).
   *
   * The request behind the year picker, and the thing it replaced is worth
   * keeping in view: the wall used to derive its years from `activityByMonth`,
   * which is a **proxy**. `reading_events` gets a row when a read started, when
   * a note was written, when a highlight carries a device date and when a device
   * measured minutes — and it holds nothing at all until `rb activity --refill`
   * has run. So a library that never refilled offered no years while plainly
   * having finished things, and a year could be offered because a note was
   * written in it while no read ended.
   *
   * **`open` is not a year and must not be rendered as one.** A wall holds open
   * readings deliberately, and they have no `finished_at`, so the years alone do
   * not partition it. The chip's own count is `countReadings` with
   * `{ open: true }` — never `total − Σ(years)`, which would be this frontend
   * inventing an arithmetic the engine did not state.
   *
   * Hand it the **same filter** the page and the count get; all three clauses
   * come from one predicate below the seam, so any disagreement is the caller's.
   */
  readingYears(filter: ReadingFilterDto | null): Promise<ReadingYearsDto>;

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

  // ---- searching one book's marks (items 40, 50) --------------------------

  /**
   * This book's notes **and** passages, in one ranked list (item 50).
   *
   * `bookId` is the engine's predicate and never a filter over the answer.
   * That is the whole of item 40 and the reason this method took a wave longer
   * than the screen that wanted it: `limit` cuts the *global* ranked list, so a
   * caller that searched the library and kept one book's hits gets **nothing**
   * whenever the best marks live in other books — which is most queries, and an
   * empty answer is indistinguishable from *you never wrote about that*.
   *
   * **One list, both kinds.** The order is the engine's — each index ordered by
   * its own bm25, the two merged by within-source position, recency breaking
   * ties — and no score crosses the wire, because a note's rank and a
   * highlight's rank come from different corpora and no constant converts one
   * into the other. Nothing above this seam re-sorts it. Asking twice with
   * `source` set and stitching the two replies together would be inventing
   * exactly the ranking the single method exists to make unnecessary.
   *
   * **`limit` is a hard ceiling with no offset and no cursor**, and — unlike
   * `listBooks`, where a negative limit means *no limit*, and `listNotes`,
   * where an absent one means *every note* — **`0` and negatives return an
   * empty list**. Three neighbouring methods, three meanings, so it is required
   * here rather than defaulted: a `?? 0` reaching this parameter would be a
   * search that silently found nothing.
   *
   * An empty or whitespace query is **not an error and not a search**: the
   * engine issues no statement and answers with an empty list, so a box may
   * send every keystroke without guarding for blankness. Punctuation is safe
   * too — every token is quoted into a phrase below the seam, so `don't` and
   * `C++` are searches rather than fts5 syntax errors.
   *
   * Each hit carries a `snippet`: plain text with `>>`/`<<` around the matched
   * terms, **unescaped**, and it may have come from any indexed column — a
   * highlight's `ko_note` or `annotation` as readily as its `text`. Render it
   * through `$lib/book/snippet`, never as HTML, and do not assume it is a
   * substring of the row beside it.
   */
  searchMarks(query: string, bookId: number | null, limit: number): Promise<SearchHitDto[]>;

  // ---- citations ----------------------------------------------------------

  /**
   * Tie a note to a passage, **by reference**: the citation survives a device
   * refresh rewriting the highlight's device-owned fields.
   */
  cite(noteId: number, highlightId: number): Promise<void>;
  uncite(noteId: number, highlightId: number): Promise<boolean>;
  /**
   * Which passages **one** note cites, as rows — the words are the point.
   *
   * It feeds the Cite/Uncite toggle, which is per *open* note, and it is what a
   * pane showing those passages would take. For the mark on a whole band of
   * passages, take [`citationsForNotes`] instead: this one in a loop is the N+1
   * item 46 exists to retire.
   */
  citationsFor(noteId: number): Promise<HighlightDto[]>;
  /**
   * Which passages a whole **page of notes** cites, as ids (item 46).
   *
   * One call for the note ids the route already holds, never one per note. The
   * reply is `{ note_id, highlight_ids }` — **ids, not rows**, because the
   * caller is a highlight list that is already holding those highlights and a
   * `HighlightDto` per citing note would put the reader's private text back on
   * the wire once per tick.
   *
   * **One entry per id asked, in the order asked, empties and duplicates
   * included.** A note id that is not a note gets an empty entry — to this
   * question *no such note* and *cites nothing* are the same answer — which is
   * what makes the reply zippable against the page the caller already has.
   *
   * Its scope is **the ids you name**. `citations` ties no note's book to the
   * highlight's, so an unsorted note could quote a passage and not be in any
   * page of notes; there is no reverse query and a mark drawn from this must
   * therefore claim only what it asked about.
   */
  citationsForNotes(noteIds: number[]): Promise<NoteCitationsDto[]>;

  // ---- flashcards (items 45, 49) ------------------------------------------

  /**
   * Capture a card. **`true` is created and `false` is *you already had it***.
   *
   * `UNIQUE(book_id, word)` dedupes through `ON CONFLICT DO NOTHING`, so a
   * repeat leaves the existing card exactly as it was — a later capture of the
   * same word does **not** repoint it at a different passage. A caller drawing
   * both as "saved" throws away the only thing the write answers.
   *
   * The pair is re-read server-side: a `highlightId` belonging to another book
   * is an `InvalidInput` and one belonging to nothing is a `NotFound`, and they
   * are different refusals. Do not pre-validate either here — the point of a
   * write taking ids is that the refusal lives where the rows do.
   *
   * It returns no id and no card, so a screen that wants to *show* the card
   * re-asks [`listFlashcardsForBook`]. Synthesizing one from what was sent
   * would be wrong exactly on `false`, where the card that exists may carry a
   * different passage and a different context than the one just offered.
   */
  createFlashcard(card: NewFlashcard): Promise<boolean>;
  /**
   * Every card captured from this book, pending and exported alike (item 45).
   *
   * `FlashcardDto` carries `highlight_id`, which is the whole point: a card can
   * be shown beside the passage its word came from. `null` there is ordinary —
   * the KOReader import's auto-capture anchors, but a card need not.
   */
  listFlashcardsForBook(bookId: number): Promise<FlashcardDto[]>;

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

  // ---- the wall of cards (items 43, 41, 47) -------------------------------

  /**
   * All four params are stated, because ts-rs emits every one as required
   * however `#[serde(default)]` the Rust is — and for `limit` that is not a
   * quirk but the design: an omitted limit would be a page of nothing.
   */
  async listReadingRows({
    limit,
    sort = 'finished',
    offset = 0,
    filter = null,
  }: ReadingPage): Promise<ReadingRow[]> {
    const r = expect(
      await this.#call({ method: 'list_reading_rows', params: { limit, sort, offset, filter } }),
      'reading_rows',
    );
    // Every book out of the library has an id. Narrowed once, here — the same
    // move `listBooks` makes, for the same reason.
    return r.value as ReadingRow[];
  }

  async countReadings(filter: ReadingFilterDto | null): Promise<number> {
    return expect(await this.#call({ method: 'count_readings', params: { filter } }), 'count')
      .value;
  }

  async readingYears(filter: ReadingFilterDto | null): Promise<ReadingYearsDto> {
    return expect(
      await this.#call({ method: 'reading_years', params: { filter } }),
      'reading_years',
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
    return expect(
      await this.#call({ method: 'book_tags', params: { book_id: bookId } }),
      'book_tags',
    ).value;
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
      await this.#call({
        method: 'set_annotation',
        params: { highlight_id: highlightId, annotation },
      }),
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
    expect(
      await this.#call({ method: 'update_note_body', params: { note_id: noteId, body } }),
      'unit',
    );
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
    return expect(
      await this.#call({ method: 'outgoing_links', params: { note_id: noteId } }),
      'links',
    ).value;
  }

  async backlinks(noteId: number): Promise<NoteDto[]> {
    return expect(await this.#call({ method: 'backlinks', params: { note_id: noteId } }), 'notes')
      .value;
  }

  /**
   * `source: null` is **both**, stated rather than omitted: one ranked list is
   * what this method is for, and narrowing to a kind here would be the caller
   * deciding something the engine's merge already decided. See the interface.
   */
  async searchMarks(query: string, bookId: number | null, limit: number): Promise<SearchHitDto[]> {
    return expect(
      await this.#call({
        method: 'search_marks',
        params: { query, source: null, book_id: bookId, limit },
      }),
      'search_hits',
    ).value;
  }

  async cite(noteId: number, highlightId: number): Promise<void> {
    expect(
      await this.#call({ method: 'cite', params: { note_id: noteId, highlight_id: highlightId } }),
      'unit',
    );
  }

  async uncite(noteId: number, highlightId: number): Promise<boolean> {
    return expect(
      await this.#call({
        method: 'uncite',
        params: { note_id: noteId, highlight_id: highlightId },
      }),
      'bool',
    ).value;
  }

  async citationsFor(noteId: number): Promise<HighlightDto[]> {
    return expect(
      await this.#call({ method: 'citations_for', params: { note_id: noteId } }),
      'highlights',
    ).value;
  }

  async citationsForNotes(noteIds: number[]): Promise<NoteCitationsDto[]> {
    return expect(
      await this.#call({ method: 'citations_for_notes', params: { note_ids: noteIds } }),
      'note_citations',
    ).value;
  }

  // ---- flashcards (items 45, 49) ------------------------------------------

  async createFlashcard({
    bookId,
    word,
    highlightId = null,
    context = null,
  }: NewFlashcard): Promise<boolean> {
    // The word is **not** trimmed here. `Engine::create_flashcard` trims before
    // it dedupes, so `" mot"` and `"mot"` are already one card; trimming here
    // too would be a second copy of a rule that has to agree with itself.
    return expect(
      await this.#call({
        method: 'create_flashcard',
        params: { book_id: bookId, highlight_id: highlightId, word, context },
      }),
      'bool',
    ).value;
  }

  async listFlashcardsForBook(bookId: number): Promise<FlashcardDto[]> {
    return expect(
      await this.#call({ method: 'list_flashcards_for_book', params: { book_id: bookId } }),
      'flashcards',
    ).value;
  }

  async activeRatingScale(): Promise<RatingScaleDto | null> {
    return expect(await this.#call({ method: 'active_rating_scale' }), 'rating_scale').value;
  }

  async reviewRating(noteId: number): Promise<RatingDto | null> {
    return expect(
      await this.#call({ method: 'review_rating', params: { note_id: noteId } }),
      'rating',
    ).value;
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
