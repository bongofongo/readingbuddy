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
  ApiError,
  BookDto,
  BookSortDto,
  HighlightDto,
  NoteDto,
  PathsDto,
  ReadingDto,
  Reply,
  Request,
  Response,
} from './bindings';

/** A book that came out of the library, so it has an id. See the module doc. */
export type StoredBook = BookDto & { id: number };

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
  getBook(id: number): Promise<StoredBook | null>;
  listHighlights(bookId: number): Promise<HighlightDto[]>;
  listNotes(bookId: number | null): Promise<NoteDto[]>;
  listReadings(bookId: number): Promise<ReadingDto[]>;
  /** A cover as a URL this environment can actually load, or null. */
  coverSrc(book: BookDto): string | null;
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
   */
  async listNotes(bookId: number | null): Promise<NoteDto[]> {
    return expect(
      await this.#call({ method: 'list_notes', params: { book_id: bookId, limit: null } }),
      'notes',
    ).value;
  }

  async listReadings(bookId: number): Promise<ReadingDto[]> {
    return expect(
      await this.#call({ method: 'list_readings', params: { book_id: bookId } }),
      'readings',
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

