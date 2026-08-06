//! readingbuddy — engine for a personal reading companion.
//!
//! The engine performs **no terminal I/O**: every user interaction lives in a
//! frontend (CLI today, TUI later). Frontends drive it through [`Engine`].

pub mod book;
pub mod calibre;
pub mod config;
pub mod crash;
pub mod device;
pub mod diagnostic;
/// The physical shape of one edition, as proportions — item 19. Height is 1.0
/// and a frontend picks the unit, so no scene constant travels with it.
pub mod edition;
pub mod enrich;
pub mod epub;
pub mod error;
pub mod files;
pub mod flashcards;
pub mod goodreads;
pub mod images;
pub mod ko_statistics;
pub mod koreader;
/// The one answer to "is this the book I already have". Internal: a frontend
/// asks an import path, never the matcher.
pub(crate) mod matching;
/// Human names, parsed — item 17. Filing order and display order, off one
/// reading of the comma.
pub mod names;
pub mod notes;
pub mod partial_md5;
/// A PDF's length and, occasionally, its name — item 22. `epub`'s twin, and the
/// module that makes "there is a page and no percentage" reachable rather than
/// a divide by a sentinel.
pub mod pdf;
/// How far into a book a reading is, as a value — item 17b.
pub mod progress;
pub mod providers;
pub mod search;
/// The two derived sort keys the shelf is indexed on — item 34.
pub mod sort;
pub mod storage;
pub mod watch;

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use reqwest::Client;

pub use book::{Book, ReadingState, isbn10_to_13, normalize_isbn, series_index_text};
pub use calibre::{
    Calibre, CalibreBook, CalibreBookReport, CalibreMatch, CalibreReport, CalibreRowState,
    ImportOptions as CalibreImportOptions, UnmatchedCalibreBook,
};
pub use config::EngineConfig;
pub use crash::CrashContext;
pub use device::{
    DeviceBook, DeviceScan, DeviceState, candidate_mounts, is_koreader_mount, koreader_dir,
    mount_roots, offers_reader,
};
pub use diagnostic::{Diagnostic, DiagnosticKind, ErrorClass, Severity};
pub use edition::{EditionShape, ShapeSource};
pub use enrich::{
    EnrichCandidate, EnrichMatch, EnrichOutcome, EnrichReport, FieldChange, HeldField,
};
pub use epub::{TableOfContents, TocEntry};
pub use error::{EngineError, Result};
pub use files::{
    FileIdentity, FileImportReport, FileMatch, FileOutcome, ImportOptions as FileImportOptions,
};
pub use goodreads::{
    GoodreadsBookReport, GoodreadsMatch, GoodreadsReport, ImportOptions as GoodreadsImportOptions,
    TextOutcome, UnmatchedRow,
};
pub use ko_statistics::{StatsImportReport, statistics_db};
pub use koreader::{
    BookImportStats, ImportReport, KoStats, KoStatus, KoSummary, MatchCandidate, MatchMethod,
    PullReport,
};
pub use notes::{CreatedNote, NewNoteInput, NoteKind, VaultReconcile};
pub use partial_md5::partial_md5;
pub use pdf::{PdfInfo, pdf_info};
pub use progress::{Fraction, FractionSource, Progress};
pub use providers::googlebooks::verify_key as verify_google_key;
pub use providers::{ProviderId, SearchRequest};
pub use search::{RankedResult, SearchOutcome};
pub use storage::{
    ActivitySummary, BookFile, BookFilter, BookQuery, BookSort, BookSummary, BookTag, Confidence,
    DayActivity, DayRange, FieldSource, FillStats, FlashcardRow, Highlight, MergeReport,
    NewHighlight, NewReadingEvent, NoteRecord, NoteSearchHit, OutgoingLink, Rating, RatingScale,
    ReadNumbering, Reading, ReadingEvent, RefillReport, Source, StatusFilter, Storage,
};
pub use watch::{
    MOUNT_QUIET, MountEvent, MountStir, MountWatcher, VAULT_QUIET, VaultEvent, VaultStir,
    VaultWatcher, watch_mounts,
};

use providers::googlebooks::GoogleBooksProvider;
use providers::openlibrary::OpenLibraryProvider;
use providers::{MetadataProvider, ProviderBook};

fn build_providers(client: &Client, key: Option<String>) -> Vec<Box<dyn MetadataProvider>> {
    vec![
        Box::new(OpenLibraryProvider::new(client.clone())),
        Box::new(GoogleBooksProvider::new(client.clone(), key)),
    ]
}

/// Take a lock, and take it back off a poisoned one.
///
/// A panic somewhere else must not turn every later search into a panic here:
/// the engine is a library, and neither of the two values behind these locks has
/// an invariant a half-finished write could break — each is replaced wholesale
/// or not at all.
/// Copy a stored cover onto an in-memory record — the `&mut Book` half of
/// [`Engine::download_cover`], which the CLI's add-flow holds before there is a
/// row to write to.
///
/// All five fields together, here as everywhere: `Storage::set_cover` is the
/// only writer of the four measurements *because* a width without the path it
/// measures is a record describing an image it is not pointing at, and a helper
/// that set three of them would be the first way to build one.
fn write_cover_onto(book: &mut Book, cover: &images::CoverFile) {
    book.cover_path = Some(cover.path.display().to_string());
    book.cover_thumb_path = cover.thumb_path.as_ref().map(|p| p.display().to_string());
    book.cover_width = cover.metrics.map(|m| m.width);
    book.cover_height = cover.metrics.map(|m| m.height);
    book.cover_accent = cover.metrics.map(|m| m.accent);
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

pub struct Engine {
    /// The storage boundary. **Private since item 14** — the facade is what a
    /// frontend talks to, and `pub storage` was the hole every frontend reached
    /// through instead. [`Engine::storage`] still exists for the engine's own
    /// integration tests, but only behind the `internals` feature.
    storage: Storage,
    config: EngineConfig,
    /// The provider list, behind a lock because [`Engine::set_google_api_key`]
    /// rebuilds it at runtime and item 14 needs that to happen through `&self`.
    ///
    /// `Arc` inside the lock rather than a guard held across the await: a
    /// federated search takes seconds, and a read guard held that long would
    /// make a key change wait for it. Cloning the `Arc` is one refcount bump
    /// and the guard is dropped before anything is awaited.
    providers: RwLock<Arc<Vec<Box<dyn MetadataProvider>>>>,
    /// The **live** Google Books key. `EngineConfig::google_api_key` is only the
    /// value it was seeded with — after a `set_google_api_key` the two disagree,
    /// which is why nothing outside this module may read the config's copy and
    /// why [`Engine::google_api_key`] is the accessor.
    google_api_key: RwLock<Option<String>>,
    client: Client,
    /// Which calibre tools this machine has, resolved **once per run**.
    ///
    /// Private, with [`Engine::calibre`] to read it — item 14's complaint is
    /// that `storage` and `config` are public fields both frontends reach past
    /// the facade through, and a new subsystem must not add a third.
    calibre: Calibre,
}

impl Engine {
    pub async fn open(config: EngineConfig) -> Result<Engine> {
        std::fs::create_dir_all(&config.images_dir)?;
        std::fs::create_dir_all(&config.files_dir)?;
        std::fs::create_dir_all(&config.vault_dir)?;
        if let Some(db_path) = config.db_url.strip_prefix("sqlite://")
            && let Some(parent) = Path::new(db_path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let client = Client::builder()
            .user_agent(concat!("readingbuddy/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let storage = Storage::connect(&config.db_url).await?;
        let key = config.google_api_key.clone();
        let providers = build_providers(&client, key.clone());
        // Once, here — not once per book on a library import, which would be a
        // PATH sweep per book for an answer that cannot change mid-run.
        let calibre = Calibre::detect(config.calibre_bin_dir.as_deref());
        Ok(Engine {
            storage,
            config,
            providers: RwLock::new(Arc::new(providers)),
            google_api_key: RwLock::new(key),
            client,
            calibre,
        })
    }

    // ---- the seam: storage, config, and what is deliberately not exposed ----

    /// The storage boundary, for the engine's **own** integration tests.
    ///
    /// Behind a feature so it cannot be a frontend's escape hatch. `tests/` is a
    /// separate crate, so it cannot reach a `pub(crate)` field, and giving every
    /// `Storage` method a facade twin purely to satisfy a test would bloat the
    /// facade with surface no product ever calls — which is the opposite of what
    /// item 14 is for. The feature is the honest middle: the door exists, and
    /// only a test build has the key.
    #[cfg(any(test, feature = "internals"))]
    #[doc(hidden)]
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Where the database is. A frontend shows this on a settings screen; it is
    /// not a handle to anything.
    pub fn db_url(&self) -> &str {
        &self.config.db_url
    }

    pub fn images_dir(&self) -> &Path {
        &self.config.images_dir
    }

    pub fn vault_dir(&self) -> &Path {
        &self.config.vault_dir
    }

    pub fn files_dir(&self) -> &Path {
        &self.config.files_dir
    }

    pub fn log_dir(&self) -> &Path {
        &self.config.log_dir
    }

    /// The Google Books key in force **now**, which after a runtime change is
    /// not the one `EngineConfig` was built with.
    ///
    /// Six narrow accessors rather than one `config()` returning `&EngineConfig`
    /// precisely because of this field: a struct getter would hand out the stale
    /// copy beside the live one and there would be no way to tell them apart at
    /// the call site.
    pub fn google_api_key(&self) -> Option<String> {
        read(&self.google_api_key).clone()
    }

    /// Swap in a new Google Books API key (or clear it) and rebuild the
    /// provider list so the change is live for the next search — frontends set
    /// this when the user enters a key at runtime.
    ///
    /// `&self`, not `&mut self`: item 14's transport hands the same engine to
    /// several connections at once through an `Arc`, and `&mut` on the facade is
    /// a method no shared owner can ever call.
    pub fn set_google_api_key(&self, key: Option<String>) {
        *write(&self.google_api_key) = key.clone();
        *write(&self.providers) = Arc::new(build_providers(&self.client, key));
    }

    /// The provider list as of this instant. Cloned out of the lock so nothing
    /// holds a guard across an await.
    fn providers(&self) -> Arc<Vec<Box<dyn MetadataProvider>>> {
        read(&self.providers).clone()
    }

    // ---- metadata search ---------------------------------------------------

    /// Federated fielded search: fan out to all providers, dedup, rank.
    // The query itself is the user's private reading interest — `skip` keeps it
    // out of the span, and only its presence is recorded.
    #[tracing::instrument(skip_all, fields(has_isbn = req.isbn.is_some()))]
    pub async fn search(&self, req: &SearchRequest) -> Result<SearchOutcome> {
        let mut req = req.clone();
        if let Some(raw) = &req.isbn {
            req.isbn =
                Some(normalize_isbn(raw).ok_or_else(|| EngineError::InvalidIsbn(raw.clone()))?);
        }
        let providers = self.providers();
        search::federated_search(&providers, &req).await
    }

    /// Direct edition lookup by ISBN, merging fields across providers.
    #[tracing::instrument(skip(self))]
    pub async fn lookup_isbn(&self, raw: &str) -> Result<Option<Book>> {
        let isbn = normalize_isbn(raw).ok_or_else(|| EngineError::InvalidIsbn(raw.to_string()))?;
        let mut found: Vec<ProviderBook> = Vec::new();
        let providers = self.providers();
        for p in providers.iter() {
            match p.by_isbn(&isbn).await {
                Ok(Some(pb)) => found.push(pb),
                Ok(None) => {}
                // One provider down must not kill the lookup — but it used to
                // vanish entirely here: no warning, no log, nothing. There is
                // no diagnostic channel on this return type, so at minimum it
                // gets logged.
                Err(e) => {
                    tracing::warn!(
                        provider = %p.id(),
                        isbn = %isbn,
                        error = %e,
                        "provider lookup failed; continuing with the others"
                    );
                }
            }
        }
        Ok(search::merge_provider_books(found))
    }

    // ---- library -----------------------------------------------------------

    /// Save (insert-or-merge) and return the stored copy.
    ///
    /// **Writes no field provenance, and that is a refusal rather than an
    /// omission.** The `Book` reaching here has already been through
    /// `search::merge_provider_books`, which keeps the winning value per field
    /// and throws away which provider supplied it — so this path genuinely
    /// cannot say where `page_count` came from, and stamping the whole record
    /// with whichever provider is guessed at would be inventing exactly the
    /// thing `field_provenance` exists to record. Item 30 merges provider by
    /// provider and can answer honestly; until then a book added by search is
    /// unattributed, the same state every book that predates migration `0012`
    /// is in.
    pub async fn save_book(&self, book: &Book) -> Result<Book> {
        let id = self.storage.upsert_book(book, None).await?;
        self.storage
            .get_book(id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {id}")))
    }

    /// One page of the library, newest-touched first unless told otherwise.
    ///
    /// `BookQuery::new(limit, sort)` is the whole of what this used to take.
    /// [`BookQuery`] adds the filter and the offset; see its module for why the
    /// page is an offset rather than a cursor.
    pub async fn list_books(&self, query: &BookQuery) -> Result<Vec<Book>> {
        self.storage.list_books(query).await
    }

    /// How many books match — the number a shelf needs *before* it needs the
    /// rows, and the one thing `list_books` could not answer without returning
    /// the whole library.
    ///
    /// The same `WHERE` the page is built from, so the count and the pages
    /// cannot disagree. See [`BookFilter`].
    pub async fn count_books(&self, filter: &BookFilter) -> Result<i64> {
        self.storage.count_books(filter).await
    }

    /// What is behind each of these books — highlights, notes, owned files —
    /// in one call for a whole page.
    ///
    /// The detail screen makes four queries for one book. A list of eight
    /// hundred cannot, which is why nothing in a list could show this before.
    pub async fn book_summaries(&self, book_ids: &[i64]) -> Result<Vec<BookSummary>> {
        self.storage.book_summaries(book_ids).await
    }

    /// One book by its internal id. [`Engine::resolve_books`] is what a
    /// user-typed selector goes through; this is the id path.
    pub async fn get_book(&self, id: i64) -> Result<Option<Book>> {
        self.storage.get_book(id).await
    }

    /// The shelf names another system minted for this book. **Inert
    /// provenance** — nothing reads these to decide anything, and collections
    /// are deliberately still deferred (`docs/decisions.md`); they are here so
    /// that design can be made against real shelf names.
    pub async fn book_tags(&self, book_id: i64) -> Result<Vec<BookTag>> {
        self.storage.book_tags(book_id).await
    }

    /// What you are reading: one row per **open** reading, most-recently-touched
    /// first.
    ///
    /// On the facade rather than left to `engine.storage`, which is public and
    /// which both frontends reach into today — unpicking that is most of item
    /// 14, and a new screen should not add to it.
    ///
    /// The [`Reading`] comes back beside the [`Book`] because the two are not
    /// interchangeable here: `Book`'s progress fields are projections of the
    /// *current* reading, which for a finished book is a closed one, while this
    /// row is specifically the open one and carries its own `status`, `source`
    /// and device mirror.
    pub async fn currently_reading(&self, limit: i64) -> Result<Vec<(Book, Reading)>> {
        self.storage.list_open_readings(limit).await
    }

    /// Resolve a user-supplied selector: numeric id, ISBN, or title fragment.
    /// Returns all candidates (empty = nothing matched, >1 = ambiguous).
    pub async fn resolve_books(&self, selector: &str) -> Result<Vec<Book>> {
        // ISBN before id, deliberately. A bare ISBN-13 is thirteen digits and
        // therefore *also* parses as an i64, so trying the id first meant
        // `show 9781784161880` looked up row 9781784161880, found nothing, and
        // reported no match — plain ISBN selectors never resolved at all, while
        // hyphenated ones did.
        if let Some(isbn) = normalize_isbn(selector) {
            let hits: Vec<Book> = self
                .storage
                .find_book_by_isbn(&isbn)
                .await?
                .into_iter()
                .collect();
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
        if let Ok(id) = selector.parse::<i64>() {
            let hits: Vec<Book> = self.storage.get_book(id).await?.into_iter().collect();
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
        // Each branch falls through rather than returning early on a miss, so a
        // selector that looks like one kind but matches another still resolves.
        self.storage.find_books_by_title(selector).await
    }

    /// Delete a book, its cover image, and the ebook files it owned.
    ///
    /// The rows go by cascade; the bytes are the engine's to remove, the same
    /// contract the cover has had since the beginning. They are listed *before*
    /// the delete because after it there is nothing left to list them by — and
    /// they are safe to remove unconditionally: `book_files` is keyed on the
    /// sha256 alone, so no other book can be holding this content.
    ///
    /// The **cover** no longer gets that guarantee for free — content-addressed
    /// names mean two books can share one jacket — so `Storage::delete_book`
    /// hands back a path only when nothing else references it. The shelf tier
    /// goes with it, derived from the cover's own name rather than read back
    /// from a row that no longer exists.
    pub async fn delete_book(&self, id: i64) -> Result<()> {
        let files = self.storage.book_files(id).await?;
        if let Some(cover) = self.storage.delete_book(id).await? {
            let cover = PathBuf::from(cover);
            std::fs::remove_file(images::thumb_path_of(&cover)).ok();
            std::fs::remove_file(cover).ok();
        }
        for file in &files {
            std::fs::remove_file(self.file_path(file)).ok();
        }
        Ok(())
    }

    /// Download `cover_url` into the images dir and persist the cover.
    pub async fn download_cover(&self, book: &mut Book) -> Result<Option<PathBuf>> {
        let Some(url) = book.cover_url.clone() else {
            return Ok(None);
        };
        let cover = self.download_cover_file(&url).await?;
        write_cover_onto(book, &cover);
        match book.id {
            // **Not `upsert_book`.** Its third branch — no `isbn_10`, no
            // `isbn_13` — is a plain unconditional insert that ignores
            // `Book::id`, so a stored book with an id and no ISBN got a
            // *duplicate row* here instead of a cover. That is every book this
            // path matters most for: a sidecar-seeded one. Found by running
            // item 30's enrichment against exactly that case, and reachable
            // before it through `Engine::fetch_cover`.
            //
            // **And not `enrich_book` either, since item 20.** This used to
            // write the *whole* record back to move one field — which is why it
            // could not name a source without claiming fifteen columns it had
            // nothing to do with — and, worse, it went through the no-clobber
            // merge, so a re-fetch that produced a *different* file left the row
            // pointing at the old one. `set_cover` writes the path and the four
            // measurements together and replaces rather than fills.
            Some(id) => self.storage.set_cover(id, &cover, None).await?,
            None if book.isbn_10.is_some() || book.isbn_13.is_some() => {
                let id = self.storage.upsert_book(book, None).await?;
                self.storage.set_cover(id, &cover, None).await?;
            }
            // An unsaved candidate with no ISBN: the caller is holding it and
            // the cover has been written onto it, which is all it asked for.
            None => {}
        }
        Ok(Some(cover.path))
    }

    /// Fetch the bytes at a cover URL. The half of [`Engine::download_cover`]
    /// that is not about *which* row to write, so enrichment can persist the
    /// file it fetched without writing the whole record back.
    async fn download_cover_file(&self, url: &str) -> Result<images::CoverFile> {
        images::image_from_url(&self.client, url, &self.config.images_dir).await
    }

    /// Measure the covers already on disk — item 20's back-fill.
    ///
    /// **A command and not a migration**, and it could not have been a
    /// migration: `cover_width` is the result of decoding a PNG and SQLite
    /// cannot decode one. `Storage::unmeasured_covers` states the work list
    /// (`cover_path IS NOT NULL AND cover_width IS NULL`) and this reads each
    /// file, measures it, writes the shelf tier where one is worth having, and
    /// persists all five columns through the same `set_cover` the download path
    /// uses — so a back-filled row and a freshly-downloaded one are the same
    /// row, which is the property a second code path would have quietly given
    /// up.
    ///
    /// Files are **not renamed**. A cover written before content addressing is
    /// not named after its own hash, but the stored `cover_path` is what a
    /// webview resolves and `docs/gui/` documents its shape; renaming every
    /// image would be a destructive change dressed as a measurement, and buys
    /// nothing — the collision it would fix is in the *write* path, which is
    /// already fixed.
    ///
    /// Returns `(measured, unreadable)`. A file that has gone missing or will
    /// not decode is counted and skipped, never an error: one bad image must not
    /// stop the other two hundred, and the next run retries it for free.
    pub async fn measure_stored_covers(&self) -> Result<(usize, usize)> {
        let mut measured = 0;
        let mut unreadable = 0;
        for (id, path) in self.storage.unmeasured_covers().await? {
            match images::measure_stored(Path::new(&path)) {
                Ok(cover) if cover.metrics.is_some() => {
                    self.storage.set_cover(id, &cover, None).await?;
                    measured += 1;
                }
                _ => unreadable += 1,
            }
        }
        Ok((measured, unreadable))
    }

    /// Compute the filing keys of every book that has never had them — item
    /// 34's back-fill.
    ///
    /// **A command and not a migration**, and like
    /// [`Engine::measure_stored_covers`] it could not have been one: `sort_title`
    /// drops a leading article and `sort_author` is
    /// [`crate::names::sort_key`]'s parse of a human name, and SQLite can do
    /// neither. `Storage::stale_sort_keys` states the work list
    /// (`sort_author IS NULL`, which is *never computed* and never *no author*)
    /// and every row goes through the same `refresh_sort_keys` that every live
    /// write goes through, so a back-filled row and a freshly-written one are
    /// the same row.
    ///
    /// Returns how many books it filed. **Idempotent** — a second run finds an
    /// empty work list — which is what lets `make dev-db` run it unconditionally
    /// and what makes the CLI's already-done wording the line a user usually
    /// sees.
    pub async fn rebuild_sort_keys(&self) -> Result<usize> {
        self.storage.rebuild_sort_keys().await
    }

    // ---- looking a book up again (item 30) ---------------------------------

    /// Ask the providers about a book we already have, and merge what they say.
    ///
    /// The gap this closes: `Storage::enrich_book` has existed since item 13 and
    /// only calibre ever called it, so every book created without an ISBN — from
    /// a sidecar, or from a filename stem — had no cover, no description and no
    /// page count, permanently.
    ///
    /// An **explicit action on one book**. Not the device pull path, which
    /// `docs/decisions.md:231` puts out of scope and which stays fully offline;
    /// nothing automatic, nothing periodic, and no count of books that have not
    /// had it done to them. There is deliberately no bulk form: the per-book cost
    /// is a provider fan-out, and a loop over the shelf is a rate-limit policy
    /// nobody has decided yet.
    ///
    /// Everything else — the one matcher, the refusal band, the per-field
    /// attribution, the user guard — is argued in [`enrich`].
    pub async fn enrich_book_from_providers(&self, book_id: i64) -> Result<EnrichReport> {
        let providers = self.providers();
        let mut report = enrich::enrich_metadata(&self.storage, &providers, book_id).await?;

        // The cover last, and only where there is no file. The merge above may
        // have just supplied the URL — which is the ordinary case, since a book
        // with no cover usually had no `cover_url` either — and a book that
        // already has an image does not re-fetch one.
        let book = self
            .storage
            .get_book(book_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))?;
        if book.cover_path.is_none()
            && let Some(url) = book.cover_url.as_deref()
        {
            match self.download_cover_file(url).await {
                Ok(cover) => {
                    // One field, one claim: whoever supplied the URL is the
                    // origin of the file fetched from it. `None` when the URL
                    // predates this run, which is the honest answer — we did not
                    // learn it here and cannot say who did. The claim is on
                    // `cover_path` alone; the measurements beside it have no
                    // origin to name.
                    let source = report.source_of("cover_url");
                    self.storage.set_cover(book_id, &cover, source).await?;
                    report.cover = Some(cover.path);
                }
                // A cover that will not download degrades; it does not fail the
                // call. The metadata landed and is worth keeping, and the next
                // run tries again for free.
                Err(e) => report.warnings.push(Diagnostic::cover_unavailable(&e)),
            }
        }
        Ok(report)
    }

    /// Write fields the **user** supplied, and record that they did.
    ///
    /// This is the door item 29 built the `user` rank for and could not open:
    /// `field_provenance`'s whole reason to exist is that a hand correction must
    /// survive the next provider merge, and until this method there was no way
    /// for a field to *become* the user's outside a test — a protection nothing
    /// could trigger. Item 30 is what made it urgent rather than tidy, in both
    /// directions: enrichment is the first writer that would silently overwrite
    /// a correction, and correcting a title or supplying an ISBN is the
    /// **next move** its refusal offers. A refusal whose remedy did not exist
    /// would be the dead end `docs/decisions.md` bans.
    ///
    /// The merge is the ordinary partial-record one — a field the record is
    /// silent about is left alone — so this **sets, and cannot clear**. Clearing
    /// a field needs a statement that can write NULL and therefore a way to say
    /// "I mean it", which no caller has asked for; saying so is better than
    /// implying it works.
    pub async fn set_book_fields(&self, book_id: i64, fields: &Book) -> Result<Book> {
        if self.storage.get_book(book_id).await?.is_none() {
            return Err(EngineError::NotFound(format!("book id {book_id}")));
        }
        self.storage
            .enrich_book(book_id, fields, Some(Source::User))
            .await?;
        self.storage
            .get_book(book_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))
    }

    /// Where each field of a book came from, and when (item 29).
    ///
    /// On the facade because item 30 gives it its first real content and its
    /// first real reader: a frontend showing "page count: 512 (openlibrary)"
    /// needs this, and reaching through `Engine::storage` for it is the seam
    /// item 14 closed. **An absent field means nobody has claimed it** — every
    /// book predating migration `0012` reports an empty list however
    /// well-populated it is.
    pub async fn field_provenance(&self, book_id: i64) -> Result<Vec<FieldSource>> {
        self.storage.field_provenance(book_id).await
    }

    /// [`Engine::download_cover`] for a book that is already stored.
    ///
    /// The `&mut Book` version is the one the CLI's add-flow wants: it is
    /// holding an unsaved candidate and needs the path written back onto it.
    /// Every *other* caller has an id and wants the stored row updated — and a
    /// `&mut` domain type is the shape item 14 names as not crossing a
    /// transport, since the mutation is the return value.
    pub async fn fetch_cover(&self, book_id: i64) -> Result<Option<PathBuf>> {
        let mut book = self
            .storage
            .get_book(book_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))?;
        self.download_cover(&mut book).await
    }

    // ---- readings ----------------------------------------------------------

    /// Every reading of this book, oldest first. A reread is a row, not a flag,
    /// so this is a history rather than a count.
    pub async fn list_readings(&self, book_id: i64) -> Result<Vec<Reading>> {
        self.storage.list_readings(book_id).await
    }

    /// One reading by id.
    pub async fn get_reading(&self, id: i64) -> Result<Option<Reading>> {
        self.storage.get_reading(id).await
    }

    /// Every reading of this book, oldest first, **each with its own
    /// progress**.
    ///
    /// The pairing is the derivation, and it is why this is a facade method and
    /// not a `From` impl above the seam. `readings` has no page count — length
    /// is bibliographic and lives on `books` — so a caller holding only a
    /// `Reading` cannot reach [`Progress::of_reading`] at all, and a caller
    /// holding both has to know which book's length goes with which read. Item
    /// 22 is where that bit: a frontend showing a reread's second read would
    /// otherwise print `BookDto::progress`, which `progress.rs` warns in so many
    /// words "will show the current read's numbers under an older read's
    /// heading". The alternative it was reaching for is `current_page /
    /// page_count` computed above the API, which is the row-state derivation
    /// `gui/CLAUDE.md` bans and which walks into all three hazards
    /// [`Progress`] documents.
    ///
    /// One book read once returns one pair, and it agrees with
    /// [`Progress::of_book`] — the two forms differ only where there is more
    /// than one read to disagree about.
    pub async fn readings_with_progress(&self, book_id: i64) -> Result<Vec<(Reading, Progress)>> {
        let length = self.length_of(book_id).await?;
        Ok(self
            .storage
            .list_readings(book_id)
            .await?
            .into_iter()
            .map(|r| {
                let p = Progress::of_reading(&r, length);
                (r, p)
            })
            .collect())
    }

    /// One named reading with its own progress. See
    /// [`Engine::readings_with_progress`].
    pub async fn reading_with_progress(&self, id: i64) -> Result<Option<(Reading, Progress)>> {
        let Some(r) = self.storage.get_reading(id).await? else {
            return Ok(None);
        };
        let length = self.length_of(r.book_id).await?;
        let p = Progress::of_reading(&r, length);
        Ok(Some((r, p)))
    }

    /// The book's open reading with its own progress, if it has one.
    pub async fn active_reading_with_progress(
        &self,
        book_id: i64,
    ) -> Result<Option<(Reading, Progress)>> {
        let Some(r) = self.storage.active_reading(book_id).await? else {
            return Ok(None);
        };
        let length = self.length_of(book_id).await?;
        let p = Progress::of_reading(&r, length);
        Ok(Some((r, p)))
    }

    /// A book's length, or absence.
    ///
    /// A missing *book* answers `None` rather than erroring: the three callers
    /// above are already holding a reading, whose foreign key guarantees the
    /// book exists, so the only way to reach the `None` arm is a race — and a
    /// read with no denominator is a state [`Progress`] already handles.
    async fn length_of(&self, book_id: i64) -> Result<Option<i64>> {
        Ok(self
            .storage
            .get_book(book_id)
            .await?
            .and_then(|b| b.page_count))
    }

    /// The book's open reading, if it has one. At most one can exist —
    /// `idx_readings_one_open` makes that an invariant.
    pub async fn active_reading(&self, book_id: i64) -> Result<Option<Reading>> {
        self.storage.active_reading(book_id).await
    }

    /// Write reading progress and return the book with its projections
    /// refreshed. Opens a reading when none is open.
    pub async fn update_progress(
        &self,
        book_id: i64,
        page: Option<i64>,
        finished: Option<bool>,
    ) -> Result<Book> {
        self.storage.update_progress(book_id, page, finished).await
    }

    /// Close the open reading and start a fresh one. Returns its id.
    pub async fn reread(&self, book_id: i64) -> Result<i64> {
        self.storage.reread(book_id).await
    }

    // ---- reading events ----------------------------------------------------

    /// Rebuild the activity log from everything already in the database:
    /// highlight stamps, note timestamps, reading endpoints.
    ///
    /// Explicit, and called by nothing here. An import that also refilled would
    /// make the log a side effect of whichever importer ran last, and a filler
    /// that needs a device (item 31) or a typed page (item 22) writes through
    /// [`Storage::record_reading_events`] instead.
    ///
    /// Idempotent: a second call with nothing changed upstream reports zero
    /// inserted and zero updated.
    pub async fn refill_reading_events(&self) -> Result<RefillReport> {
        self.storage.refill_reading_events().await
    }

    /// One book's activity log, oldest day first.
    pub async fn reading_events(&self, book_id: i64) -> Result<Vec<ReadingEvent>> {
        self.storage.reading_events(book_id).await
    }

    /// What is known about a period — books finished, days with activity, notes
    /// and links written, and minutes and pages **where they were measured**.
    ///
    /// `minutes` and `pages` come back `None` when nothing in the period
    /// measured them. That is not the same answer as zero and must not be
    /// rendered as one: a reader whose library came from a Goodreads CSV has no
    /// minutes at all, and a screen that shows them `0` has told them something
    /// false about their own reading.
    pub async fn activity_summary(&self, range: &DayRange) -> Result<ActivitySummary> {
        self.storage.activity_summary(range).await
    }

    /// The days of a period that carry an event. The set behind
    /// `ActivitySummary::activity_days`, for a caller that wants to show them.
    pub async fn activity_by_day(&self, range: &DayRange) -> Result<Vec<DayActivity>> {
        self.storage.activity_by_day(range).await
    }

    // ---- highlights --------------------------------------------------------

    /// This book's highlights, device-owned fields and all.
    ///
    /// Every reading of the book, in one list. Each row carries the
    /// `reading_id` it was attributed to — `None` where no reading's window
    /// holds it — so grouping by read is the caller's to do, and the rows that
    /// belong to no read stay reachable.
    pub async fn list_highlights(&self, book_id: i64) -> Result<Vec<Highlight>> {
        self.storage.list_highlights(book_id).await
    }

    /// What was highlighted during one reading.
    ///
    /// The reading-scoped half of [`Engine::list_highlights`], and the reason
    /// `highlights.reading_id` exists: reflections and reviews already anchor to
    /// a reading, so "what did I mark the second time through?" is a question
    /// the schema could answer well before anything could ask it.
    pub async fn highlights_for_reading(&self, reading_id: i64) -> Result<Vec<Highlight>> {
        self.storage.highlights_for_reading(reading_id).await
    }

    /// Write **our** annotation on a highlight — the column beside `ko_note`
    /// that an import never touches.
    ///
    /// New surface, and the one genuine gap the facade audit turned up: the
    /// ownership seam of migration `0004` gave the reader a field of their own
    /// and then gave no frontend a way to write it.
    pub async fn set_annotation(&self, highlight_id: i64, annotation: Option<&str>) -> Result<()> {
        self.storage.set_annotation(highlight_id, annotation).await
    }

    // ---- epub import -------------------------------------------------------

    /// Import a local .epub: extract its ISBN, enrich via providers, extract
    /// the embedded cover, save. Falls back to epub metadata alone when the
    /// file has no usable ISBN or the providers are unreachable.
    #[tracing::instrument(skip(self), fields(path = %path.display()))]
    pub async fn import_epub(&self, path: &Path) -> Result<Book> {
        let info = epub::epub_info(path)?;
        let mut seed = match &info.isbn {
            Some(isbn) => self.lookup_isbn(isbn).await?.unwrap_or_default(),
            None => Book::default(),
        };

        // Two records with two origins, merged by the same statement every other
        // partial record goes through — where this used to fold the file's
        // metadata onto the provider's field by field, with a hand-written
        // `is_none()` per column. That chain *was* `MERGE_RULES`'s no-clobber
        // merge, spelled a second time, and it produced one `Book` with two
        // origins that no single `field_provenance` stamp could describe
        // honestly. The precedence is unchanged: the provider wins, the file
        // fills the gaps, which is what `Storage::fill_book` means.
        let mut from_epub = Book {
            title: info.title.clone(),
            authors: info.authors.clone(),
            language: info.language.clone(),
            ..Default::default()
        };
        if seed.isbn_10.is_none()
            && seed.isbn_13.is_none()
            && let Some(isbn) = &info.isbn
        {
            match isbn.len() {
                10 => from_epub.isbn_10 = Some(isbn.clone()),
                _ => from_epub.isbn_13 = Some(isbn.clone()),
            }
            // The seed carries the ISBN too, and *only* as a conflict key: with
            // no ISBN on it `upsert_book` takes its third branch, a plain
            // insert, and re-importing the same file would make a second book.
            // The claim on the column is stamped by the fill below, which is
            // where it belongs — the file is what said it.
            seed.isbn_10.clone_from(&from_epub.isbn_10);
            seed.isbn_13.clone_from(&from_epub.isbn_13);
        }
        let cover = epub::extract_cover(path, &self.config.images_dir)?;
        if let Some(cover) = &cover {
            from_epub.cover_path = Some(cover.path.display().to_string());
        }

        let id = self.storage.upsert_book(&seed, None).await?;
        self.storage
            .fill_book(id, &from_epub, Some(storage::Source::Epub))
            .await?;
        // The measurements go in beside the path the fill just wrote — but only
        // when the fill actually took *this* cover. `fill_book` lets the stored
        // row win, so a book that already had a jacket keeps it, and writing
        // the epub's dimensions over that row would describe an image it is not
        // pointing at. Read back rather than predicted: the merge is the
        // authority on which path survived.
        if let Some(cover) = &cover
            && let Some(stored) = self.storage.get_book(id).await?
            && stored.cover_path.as_deref() == Some(cover.path.display().to_string().as_str())
        {
            self.storage
                .set_cover(id, cover, Some(storage::Source::Epub))
                .await?;
        }
        let saved = self
            .get_book(id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {id}")))?;

        // Record the file's KOReader identity now, while we are holding the
        // file. The payoff arrives later and elsewhere: when this book's
        // sidecar comes off the device, `match_book` takes its `md5` branch and
        // links it outright, instead of guessing at the title — and that branch
        // is the only one that works when KOReader is configured to keep
        // sidecars in `dir` or `hash` mode, away from the book.
        //
        // `link_device_book`, never `set_device_link`: this is a scan, and a
        // scan must not relabel a link the user made by hand.
        if let Some(id) = saved.id {
            let md5 = partial_md5::partial_md5(path)?;
            self.storage
                .link_device_book(&md5, id, storage::LinkedBy::Auto)
                .await?;
        }
        Ok(saved)
    }

    // ---- owned files -------------------------------------------------------

    /// Take ownership of an ebook file: work out whose it is, copy the bytes
    /// into the content store, and attach the row.
    ///
    /// Creates a book only when nothing in the library plausibly claims the
    /// file. A near-miss title comes back as [`FileOutcome::Unmatched`] with the
    /// candidates and **nothing written**, which `FileImportOptions { new: true }`
    /// overrides — the same refusal-with-a-next-move shape `ko pull` has.
    #[tracing::instrument(skip(self), fields(path = %path.display(), new = opts.new))]
    pub async fn import_file(
        &self,
        path: &Path,
        opts: files::ImportOptions,
    ) -> Result<FileImportReport> {
        files::import(self, path, opts).await
    }

    /// Attach a file to a book the caller has already decided on — dedup level
    /// 2, the epub-and-azw3 case. No matching and no creation.
    pub async fn add_file_to_book(&self, book_id: i64, path: &Path) -> Result<FileImportReport> {
        if self.storage.get_book(book_id).await?.is_none() {
            return Err(EngineError::NotFound(format!("book id {book_id}")));
        }
        files::attach(&self.storage, &self.config.files_dir, book_id, path).await
    }

    /// What a file is and what it looks like a copy of. Read-only — no bytes
    /// are copied and no row is written, so a frontend can show the answer
    /// before the user commits to it.
    pub async fn identify_file(&self, path: &Path) -> Result<FileIdentity> {
        files::identify(&self.storage, path).await
    }

    /// The files this book owns.
    pub async fn book_files(&self, book_id: i64) -> Result<Vec<BookFile>> {
        self.storage.book_files(book_id).await
    }

    /// The chapter list of the epub this book owns — **read from the file, not
    /// from the database** (item 32).
    ///
    /// `None` means there is no file here this can read: no owned epub at all,
    /// or only formats it cannot parse. `Some(entries)` with an empty list
    /// means the epub itself carries no navigable TOC, which is a different
    /// answer and an ordinary one. Neither is an error; a file we own that will
    /// not parse *is*, because those bytes are ours and were verified on the
    /// way in.
    ///
    /// Why nothing is stored is argued at [`epub::table_of_contents`]. The
    /// short of it: the file is content-addressed, so it is always the current
    /// answer, and a chapter list has no origin to attribute or user to correct
    /// it — which is what every other column migration `0013` added does have.
    ///
    /// The **first** epub, when a book owns several. Two epubs on one book is
    /// legitimate (a re-download, a second edition) and choosing between them is
    /// a question no caller has asked yet; `book_files` order is stable, so the
    /// answer at least does not flicker.
    pub async fn table_of_contents(&self, book_id: i64) -> Result<Option<TableOfContents>> {
        let Some(file) = self
            .storage
            .book_files(book_id)
            .await?
            .into_iter()
            .find(|f| f.format == "epub")
        else {
            return Ok(None);
        };
        let entries = epub::table_of_contents(&self.file_path(&file))?;
        Ok(Some(TableOfContents {
            sha256: file.sha256,
            entries,
        }))
    }

    /// One owned file by its content address. `sha256` is the primary key, so
    /// this is the whole of "who has these bytes" — at most one book can.
    pub async fn book_file(&self, sha256: &str) -> Result<Option<BookFile>> {
        self.storage.book_file(sha256).await
    }

    /// Where a file's bytes are. Derived from the row, never stored — the whole
    /// point of a content address is that the path is not a second fact that can
    /// disagree with the first.
    pub fn file_path(&self, file: &BookFile) -> PathBuf {
        files::content_path(&self.config.files_dir, &file.sha256, &file.format)
    }

    /// Give up a file: forget the row and remove the bytes.
    ///
    /// Returns false when the sha was not ours, which makes a repeat call a
    /// no-op rather than an error.
    pub async fn remove_file(&self, sha256: &str) -> Result<bool> {
        let Some(file) = self.storage.book_file(sha256).await? else {
            return Ok(false);
        };
        self.storage.delete_book_file(sha256).await?;
        std::fs::remove_file(self.file_path(&file)).ok();
        Ok(true)
    }

    // ---- koreader ----------------------------------------------------------

    /// Import KOReader highlights/notes from a sidecar file, .sdr dir, or
    /// library root. Idempotent; single-word highlights become flashcard
    /// candidates.
    #[tracing::instrument(skip(self), fields(path = %path.display()))]
    pub async fn import_koreader(&self, path: &Path, dry_run: bool) -> Result<ImportReport> {
        koreader::import(&self.storage, path, dry_run).await
    }

    /// Pull a book in from the reader: create it from the sidecar's own
    /// metadata, then import its highlights. Offline — no provider enrichment.
    #[tracing::instrument(skip(self), fields(path = %sidecar.display()))]
    pub async fn pull_book_from_sidecar(&self, sidecar: &Path) -> Result<PullReport> {
        koreader::import_book_from_sidecar(&self.storage, sidecar).await
    }

    // ---- device ------------------------------------------------------------

    /// Walk a mounted reader and report the state of every book on it.
    /// Read-only, and pre-filtered on each sidecar's size and mtime so a
    /// re-scan does not re-evaluate several hundred Lua files.
    ///
    /// Exposed here so a frontend never reaches into [`koreader`] directly.
    #[tracing::instrument(skip(self), fields(root = %root.display()))]
    pub async fn scan_device(&self, root: &Path) -> Result<device::DeviceScan> {
        device::scan_device(&self.storage, root).await
    }

    /// Pull a selection of the device in: one report per book.
    ///
    /// **Deliberately does not import statistics.** See
    /// [`Engine::import_device_statistics`].
    pub async fn sync_device(&self, paths: &[PathBuf]) -> Result<Vec<PullReport>> {
        device::sync_device(&self.storage, paths).await
    }

    /// Import measured reading time from a mounted device's
    /// `statistics.sqlite3` into the activity log.
    ///
    /// **A verb of its own, and not part of [`Engine::sync_device`].**
    /// `docs/decisions.md` makes arrival read-only, and a device scan that
    /// silently began importing months of timing data would not be read-only in
    /// spirit even though every byte written is ours. The user asks for this by
    /// name.
    ///
    /// Absence is ordinary: a device whose owner never enabled the statistics
    /// plugin returns an empty report carrying a `Diagnostic`, not an error.
    pub async fn import_device_statistics(&self, mount: &Path) -> Result<StatsImportReport> {
        ko_statistics::import_device_statistics(&self.storage, mount).await
    }

    /// Library books that look like this sidecar's book but not enough to link
    /// unasked.
    pub async fn sidecar_candidates(&self, sidecar: &Path) -> Result<Vec<MatchCandidate>> {
        let sc = koreader::parse_sidecar(&std::fs::read_to_string(sidecar)?)?;
        koreader::match_candidates(&self.storage, &sc).await
    }

    /// Record that this sidecar is that book, so it is never re-guessed.
    pub async fn link_sidecar(&self, sidecar: &Path, book_id: i64) -> Result<String> {
        let sc = koreader::parse_sidecar(&std::fs::read_to_string(sidecar)?)?;
        let Some(md5) = sc.partial_md5 else {
            return Err(EngineError::InvalidInput(format!(
                "{} has no partial_md5_checksum, so there is nothing to link it by",
                sidecar.display()
            )));
        };
        koreader::link_sidecar(&self.storage, &md5, book_id).await?;
        Ok(md5)
    }

    /// Fold one book into another, deleting `src`. Removes `src`'s cover file
    /// when `dst` kept its own — storage reports the orphan, the caller (this)
    /// owns the filesystem.
    pub async fn merge_books(&self, src: i64, dst: i64) -> Result<MergeReport> {
        let report = self.storage.merge_books(src, dst).await?;
        if let Some(cover) = &report.orphaned_cover {
            // Same resolution as `delete_book`: the stored path as written, and
            // the shelf tier derived from it. `merge_books` has already checked
            // that nothing else points at this file — two books sharing one
            // content-addressed jacket is ordinary since item 20.
            let cover = PathBuf::from(cover);
            std::fs::remove_file(images::thumb_path_of(&cover)).ok();
            std::fs::remove_file(cover).ok();
        }
        Ok(report)
    }

    // ---- notes -------------------------------------------------------------

    pub async fn create_note(&self, input: NewNoteInput) -> Result<CreatedNote> {
        let book = match input.book_id {
            Some(id) => self.storage.get_book(id).await?,
            None => None,
        };
        notes::create_note(&self.storage, &self.config.vault_dir, book.as_ref(), input).await
    }

    /// Notes, newest first. `limit` selects along `created_at`; `None` is every
    /// note and is for callers walking the whole graph rather than filling a
    /// viewport — see [`Storage::list_notes`].
    pub async fn list_notes(
        &self,
        book_id: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<NoteRecord>> {
        self.storage.list_notes(book_id, limit).await
    }

    pub async fn search_notes(&self, query: &str, limit: i64) -> Result<Vec<NoteSearchHit>> {
        self.storage.search_notes(query, limit).await
    }

    /// One note by id.
    pub async fn get_note(&self, note_id: i64) -> Result<Option<NoteRecord>> {
        self.storage.get_note(note_id).await
    }

    /// This reading's note of that kind, if it has one. `kind` is
    /// [`NoteKind::as_str`]; the pair `idx_one_reflection`/`idx_one_review`
    /// makes "one of each per reading" an invariant, so this is at most one row.
    pub async fn note_for_reading(
        &self,
        reading_id: i64,
        kind: &str,
    ) -> Result<Option<NoteRecord>> {
        self.storage.note_for_reading(reading_id, kind).await
    }

    /// Where a note's markdown lives. Derived from the row against the vault
    /// root — `NoteRecord::file_path` is relative, and every caller that joined
    /// it by hand had to reach `config.vault_dir` to do so.
    pub fn note_path(&self, note: &NoteRecord) -> PathBuf {
        self.config.vault_dir.join(&note.file_path)
    }

    /// What this note links **out** to: each `[[wikilink]]` its body wrote, and
    /// the note it resolves to when one exists.
    ///
    /// A target with no note is kept as text rather than dropped — a forward
    /// reference to something not written yet is how a zettelkasten is built,
    /// and the edge resolves itself the moment that note appears.
    pub async fn outgoing_links(&self, note_id: i64) -> Result<Vec<OutgoingLink>> {
        self.storage.outgoing_links(note_id).await
    }

    /// What links **in** to this note.
    ///
    /// The facade had no link method at all before this pair: edges were
    /// written by `create_note` / `update_note_body` and read only inside
    /// `open_anchored`, so the graph could be built but never walked.
    pub async fn backlinks(&self, note_id: i64) -> Result<Vec<NoteRecord>> {
        self.storage.backlinks(note_id).await
    }

    /// The body text of a note (its markdown minus the frontmatter header).
    pub fn note_body(&self, note: &NoteRecord) -> Result<String> {
        let file = self.note_path(note);
        let content = std::fs::read_to_string(&file)?;
        let (_, body) = notes::frontmatter_and_body(&content);
        Ok(body.trim_end().to_string())
    }

    /// Replace a note's body, preserving its frontmatter header, and reindex it
    /// — FTS **and** its wikilink edges. Used by the in-house editor.
    ///
    /// Re-indexing the edges is what makes a reflection the hub it is meant to
    /// be: it is opened empty and written afterwards, so edges computed only at
    /// creation would leave it with none.
    pub async fn update_note_body(&self, note: &NoteRecord, body: &str) -> Result<()> {
        let file = self.note_path(note);
        let content = std::fs::read_to_string(&file)?;
        let (header, _) = notes::frontmatter_and_body(&content);
        std::fs::write(&file, format!("{header}{}\n", body.trim_end()))?;
        self.storage
            .refresh_note_body(note.id, &note.title, body)
            .await?;
        let links = notes::extract_wikilinks(body);
        self.storage
            .set_note_links(note.id, &note.title, &links)
            .await
    }

    /// Delete a note: remove its markdown file from the vault, then its DB row
    /// and FTS entry. A missing file is not an error (the DB row still goes).
    pub async fn delete_note(&self, note: &NoteRecord) -> Result<()> {
        let file = self.note_path(note);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.storage.delete_note(note.id).await
    }

    /// Re-read one note file from disk and bring its index in line with it —
    /// FTS body **and** wikilink edges, since an outside edit is exactly where
    /// a new `[[wikilink]]` appears.
    ///
    /// The single-note form of what [`Engine::reconcile_vault`] does to all of
    /// them, and worth keeping on the facade and on the wire beside it: a
    /// frontend that knows which note it just handed to `$EDITOR` should not
    /// have to sweep a whole vault to learn what it already knows.
    pub async fn refresh_note_from_disk(&self, note: &NoteRecord) -> Result<()> {
        let file = self.note_path(note);
        let content = std::fs::read_to_string(&file)?;
        let (_, body) = notes::frontmatter_and_body(&content);
        notes::reindex_from_body(&self.storage, note.id, &note.title, body).await?;
        Ok(())
    }

    // ---- vault coherence (item 24) -----------------------------------------

    /// Follow the vault, so a note edited in Obsidian is a note search can
    /// still find.
    ///
    /// A [`VaultWatcher`] does nothing until it is polled and spawns nothing
    /// when it is: the caller drives it from its own loop, and the re-index
    /// happens on the caller's task. Fails when the platform cannot watch,
    /// which is a thing to degrade around rather than abort on — the vault
    /// still works, and [`Engine::reconcile_vault`] is the whole answer for a
    /// machine that cannot watch at all.
    pub fn watch_vault(&self) -> Result<VaultWatcher> {
        watch::watch_vault(&self.config.vault_dir, self.storage.clone())
    }

    /// Bring every note's index in line with its file, once.
    ///
    /// **The half a watcher structurally cannot do.** A watcher sees only the
    /// present, and the ordinary case is a note edited in another program while
    /// readingbuddy was not running. Cheap on the common path — a `stat` per
    /// note, and a read only for the ones whose file is newer than their index.
    ///
    /// A note whose file is missing is left exactly as it is. Absence is not a
    /// deletion here, for the reasons [`VaultWatcher`] sets out.
    pub async fn reconcile_vault(&self) -> Result<VaultReconcile> {
        notes::reconcile_vault(&self.storage, &self.config.vault_dir).await
    }

    // ---- reflection + review -----------------------------------------------

    /// Open this reading's reflection, creating it on the first call and
    /// returning the same note ever after. Private, and the hub of the graph.
    ///
    /// `reading_id: None` means the book's current reading — and opens one when
    /// the book has none, because a reflection is written *mid-book* and that is
    /// the normal case, not an edge one.
    pub async fn open_reflection(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
    ) -> Result<CreatedNote> {
        self.open_anchored(book_id, reading_id, NoteKind::Reflection)
            .await
    }

    /// Open this reading's review: public prose, and the only note kind that
    /// carries a rating.
    ///
    /// **Never derived from the reflection.** A review is a rewrite for a
    /// different audience, not a subset of private thinking — no `public:`
    /// frontmatter key, no divider, no shared body.
    pub async fn open_review(&self, book_id: i64, reading_id: Option<i64>) -> Result<CreatedNote> {
        self.open_anchored(book_id, reading_id, NoteKind::Review)
            .await
    }

    /// [`Engine::open_reflection`], as the [`NoteRecord`] an editor needs.
    ///
    /// `CreatedNote` is what the *creating* caller wants — a path to hand to
    /// `$EDITOR` and the links it started with. Everything that then edits the
    /// note wants a `NoteRecord` instead: `note_body`, `update_note_body`,
    /// `delete_note` and the TUI's `TextEditor` all take one. The CLI already
    /// patches over the gap with a follow-up `storage.get_note(note.id)`, and
    /// the home screen's whole action is "open the reflection", so it meets the
    /// same gap on its first keypress.
    ///
    /// A wrapper rather than a change of return type: `open_reflection` is what
    /// the CLI calls, and widening its signature is a diff in files this has no
    /// other reason to touch. Both go through the same `open_anchored`, so
    /// there is one definition of which note this is.
    pub async fn open_reflection_record(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
    ) -> Result<NoteRecord> {
        self.open_anchored_record(book_id, reading_id, NoteKind::Reflection)
            .await
    }

    /// [`Engine::open_review`], as a [`NoteRecord`]. The twin of
    /// [`Engine::open_reflection_record`], and it exists for the same reason.
    pub async fn open_review_record(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
    ) -> Result<NoteRecord> {
        self.open_anchored_record(book_id, reading_id, NoteKind::Review)
            .await
    }

    /// Open the note, then read the row back.
    ///
    /// `NotFound` rather than an `expect`: the row was written or found a
    /// statement ago, so its absence means the database changed underneath us,
    /// and the engine is a library — it must not take a frontend down over it.
    async fn open_anchored_record(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
        kind: NoteKind,
    ) -> Result<NoteRecord> {
        let note = self.open_anchored(book_id, reading_id, kind).await?;
        self.storage
            .get_note(note.id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("note id {}", note.id)))
    }

    /// The one implementation behind both. They differ in the `kind` they
    /// carry and in nothing else — which is the point of reusing `notes`
    /// instead of building a parallel vault.
    async fn open_anchored(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
        kind: NoteKind,
    ) -> Result<CreatedNote> {
        let book = self
            .storage
            .get_book(book_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {book_id}")))?;

        let reading_id = match reading_id {
            Some(id) => {
                let reading = self
                    .storage
                    .get_reading(id)
                    .await?
                    .ok_or_else(|| EngineError::NotFound(format!("reading id {id}")))?;
                if reading.book_id != book_id {
                    return Err(EngineError::InvalidInput(format!(
                        "reading {id} belongs to book {}, not {book_id}",
                        reading.book_id
                    )));
                }
                id
            }
            // `ensure_reading`, not `open_reading`: the current reading is the
            // one being reflected on, open or not, and a second reading only
            // ever starts because the user said so (`progress --reread`).
            None => {
                let now = time::OffsetDateTime::now_utc().unix_timestamp();
                self.storage
                    .ensure_reading(book_id, Some(now), "manual")
                    .await?
            }
        };

        // Accretion is exactly this: the second call finds the first call's
        // note. The partial unique indexes are what make "the" reflection of a
        // reading a fact rather than a hope.
        if let Some(existing) = self
            .storage
            .note_for_reading(reading_id, kind.as_str())
            .await?
        {
            let links = self
                .storage
                .note_links(existing.id)
                .await?
                .into_iter()
                .map(|(target, _)| target)
                .collect();
            return Ok(CreatedNote {
                id: existing.id,
                title: existing.title,
                file: self.config.vault_dir.join(&existing.file_path),
                links,
            });
        }

        let readings = self.storage.list_readings(book_id).await?;
        let nth = readings
            .iter()
            .position(|r| r.id == reading_id)
            .map(|i| i + 1);
        let label = match kind {
            NoteKind::Review => "Review",
            _ => "Reflection",
        };
        // The title is a wikilink target, so a reread's pair must not collide
        // with the first reading's.
        let title = match nth {
            Some(n) if n > 1 => format!("{label}: {} ({n})", book.display_title()),
            _ => format!("{label}: {}", book.display_title()),
        };

        notes::create_note(
            &self.storage,
            &self.config.vault_dir,
            Some(&book),
            NewNoteInput {
                book_id: Some(book_id),
                reading_id: Some(reading_id),
                kind,
                title: Some(title),
                body: String::new(),
                ..Default::default()
            },
        )
        .await
    }

    /// Rate a review on the active scale.
    ///
    /// The **raw value and the scale id** are stored, never only what it maps
    /// to: the Goodreads map is user-editable, so the mapping has to stay
    /// re-derivable from what the user actually said.
    pub async fn set_rating(&self, note_id: i64, value: f64) -> Result<()> {
        let note = self
            .storage
            .get_note(note_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("note id {note_id}")))?;
        // A reflection that wants a private score can have one later; it is not
        // the same number and must not share this column.
        if note.kind != NoteKind::Review.as_str() {
            return Err(EngineError::InvalidInput(format!(
                "note {note_id} is a {}; a rating belongs to a review",
                note.kind
            )));
        }
        let scale = self.storage.active_rating_scale().await?.ok_or_else(|| {
            EngineError::InvalidInput(
                "no rating scale defined — `readingbuddy rating scale --min --max --step`".into(),
            )
        })?;
        self.storage.set_review_rating(note_id, &scale, value).await
    }

    /// This review's rating on Goodreads' integer 0–5, `None` when it has no
    /// rating at all.
    ///
    /// A rating the user has never mapped is [`EngineError::UnmappedRating`],
    /// not a rounded integer: Goodreads takes no halves, and quietly picking one
    /// would put a number the user never chose on a public shelf.
    pub async fn goodreads_rating(&self, note_id: i64) -> Result<Option<u8>> {
        let Some(rating) = self.storage.review_rating(note_id).await? else {
            return Ok(None);
        };
        match self
            .storage
            .goodreads_for(&rating.scale, rating.value)
            .await?
        {
            Some(g) => Ok(Some(g)),
            None => Err(EngineError::UnmappedRating {
                value: rating.value,
                scale: rating.scale.name,
            }),
        }
    }

    /// This review's rating, raw value and scale both — never only the value,
    /// which without its scale means nothing.
    pub async fn review_rating(&self, note_id: i64) -> Result<Option<Rating>> {
        self.storage.review_rating(note_id).await
    }

    /// Unrate a review. False when it had no rating, so a repeat call is a
    /// no-op rather than an error.
    pub async fn clear_review_rating(&self, note_id: i64) -> Result<bool> {
        self.storage.clear_review_rating(note_id).await
    }

    // ---- rating scales -----------------------------------------------------
    //
    // Scale admin had no facade at all: `rating scale|map|show` was written
    // entirely against `engine.storage`, which is three of the reasons item 14
    // exists in one command.

    /// Define a scale, or redefine an existing one by name. A **new** scale
    /// becomes the default; redefining one leaves the flag alone.
    pub async fn put_rating_scale(
        &self,
        name: &str,
        min: f64,
        max: f64,
        step: f64,
    ) -> Result<RatingScale> {
        self.storage.put_rating_scale(name, min, max, step).await
    }

    /// Every scale the library knows.
    pub async fn rating_scales(&self) -> Result<Vec<RatingScale>> {
        self.storage.list_rating_scales().await
    }

    /// The scale a new rating is given on — the one flagged `is_default`, with
    /// deliberately no ordering fallback.
    pub async fn active_rating_scale(&self) -> Result<Option<RatingScale>> {
        self.storage.active_rating_scale().await
    }

    pub async fn rating_scale_by_name(&self, name: &str) -> Result<Option<RatingScale>> {
        self.storage.rating_scale_by_name(name).await
    }

    /// What this scale's values mean on Goodreads' integer 0–5. An **explicit
    /// lookup table**; a point with no entry is reported, never rounded.
    pub async fn rating_map(&self, scale_id: i64) -> Result<Vec<(f64, u8)>> {
        self.storage.rating_map(scale_id).await
    }

    /// Add one entry to that table.
    pub async fn map_rating(&self, scale: &RatingScale, value: f64, goodreads: u8) -> Result<()> {
        self.storage.map_rating(scale, value, goodreads).await
    }

    // ---- citations ---------------------------------------------------------

    /// Cite a highlight from a note — by reference, so the citation stays live
    /// when a device refresh rewrites the highlight's device-owned fields.
    pub async fn cite(&self, note_id: i64, highlight_id: i64) -> Result<()> {
        self.storage.add_citation(note_id, highlight_id).await?;
        Ok(())
    }

    /// Drop a citation. False when it was not there.
    pub async fn uncite(&self, note_id: i64, highlight_id: i64) -> Result<bool> {
        self.storage.remove_citation(note_id, highlight_id).await
    }

    pub async fn citations_for(&self, note_id: i64) -> Result<Vec<Highlight>> {
        self.storage.citations_for(note_id).await
    }

    // ---- goodreads ---------------------------------------------------------

    /// Import a Goodreads CSV export. `dry_run` reports what would change and
    /// writes nothing.
    ///
    /// Needs items 3, 5 and 7 to be lossless, and all three are in: the
    /// sidecar-era matcher (so a row finds the book it already is), `readings`
    /// (so `Read Count` is history rather than a flag), and reviews with
    /// ratings (so `My Review` and `My Rating` have somewhere true to land).
    #[tracing::instrument(skip(self), fields(path = %path.display(), dry_run = opts.dry_run))]
    pub async fn import_goodreads(
        &self,
        path: &Path,
        opts: goodreads::ImportOptions,
    ) -> Result<GoodreadsReport> {
        goodreads::import(self, path, opts).await
    }

    /// Build a Goodreads-importable CSV of the library, plus every honest
    /// failure along the way. Same shape as [`Engine::export_flashcards`]:
    /// the payload comes back, the caller owns the file.
    pub async fn export_goodreads(&self) -> Result<(String, Vec<Diagnostic>)> {
        goodreads::export(self).await
    }

    /// Record that a Goodreads row is that book, so it is never re-guessed.
    ///
    /// The [`Engine::link_sidecar`] of the CSV importer, and it exists for the
    /// same reason: an unmatched row has to be a decision rather than a dead
    /// end, and until now the only escape hatch was `create_ambiguous` followed
    /// by [`Engine::merge_books`] — which creates a duplicate on purpose in
    /// order to fold it back in, and leaves the far side's id pointing at
    /// whichever of the two the merge happened to delete.
    ///
    /// Takes Goodreads' own `Book Id` because that is what
    /// [`UnmatchedRow::external_id`](goodreads::UnmatchedRow) carries and what
    /// the `ExternalId` rung of the matcher reads. A row without one cannot be
    /// linked — there is nothing durable to link *by*, the CSV having no other
    /// stable key — and says so rather than linking by title.
    pub async fn link_goodreads_row(&self, external_id: &str, book_id: i64) -> Result<()> {
        self.link_foreign_record(goodreads::SOURCE, external_id, book_id)
            .await
    }

    // ---- calibre -----------------------------------------------------------

    /// Which calibre tools this machine has. Feature detection, resolved once
    /// at [`Engine::open`] — a frontend asks this to decide whether the feature
    /// exists at all, and must never be told to install anything.
    pub fn calibre(&self) -> &Calibre {
        &self.calibre
    }

    /// Tier (i): convert a book between formats through `ebook-convert`.
    /// Calibre reads both formats off the two extensions.
    ///
    /// Refuses to overwrite unless told: the output path is typed by hand, and
    /// losing a file is the one outcome with no undo.
    #[tracing::instrument(skip(self), fields(input = %input.display(), output = %output.display()))]
    pub async fn convert_ebook(
        &self,
        input: &Path,
        output: &Path,
        overwrite: bool,
    ) -> Result<PathBuf> {
        calibre::convert(&self.calibre, input, output, overwrite).await
    }

    /// Tier (ii): what a calibre library holds, read-only.
    pub async fn calibre_library(&self, library: Option<&Path>) -> Result<Vec<CalibreBook>> {
        calibre::list_library(&self.calibre, library).await
    }

    /// Tier (ii): import a calibre library. `dry_run` reports what would change
    /// and writes nothing.
    ///
    /// The onboarding win `docs/ux-positioning.md` names: a library the user has
    /// already curated, with covers, ISBNs and tags, without typing one ISBN.
    #[tracing::instrument(skip(self), fields(dry_run = opts.dry_run))]
    pub async fn import_calibre_library(
        &self,
        opts: &calibre::ImportOptions,
    ) -> Result<CalibreReport> {
        calibre::import(self, opts).await
    }

    /// Record that a calibre book is that book of ours. The calibre twin of
    /// [`Engine::link_goodreads_row`].
    ///
    /// Keyed on calibre's **uuid, never its `id`**, for the reason
    /// `external_ids` is: calibre ids are per-library and reused after a delete,
    /// and this table has no library column to tell two libraries' id 4 apart.
    /// A row calibre gave no uuid therefore cannot be linked, which is the same
    /// row `CalibreRowNotIdentified` already warns about.
    pub async fn link_calibre_book(&self, uuid: &str, book_id: i64) -> Result<()> {
        self.link_foreign_record(calibre::SOURCE, uuid, book_id)
            .await
    }

    /// The one implementation behind the two link methods above.
    ///
    /// Shared so the two cannot drift, and separate from
    /// [`Storage::link_external_id`] because that is a bare upsert: `book_id`
    /// references `books(id)`, so an id that does not exist comes back as a raw
    /// foreign-key error naming a constraint. A frontend offering a candidate
    /// list can pass a book deleted in another pane, and every caller branches
    /// on the answer, so it gets [`EngineError::NotFound`] instead.
    async fn link_foreign_record(
        &self,
        source: &str,
        external_id: &str,
        book_id: i64,
    ) -> Result<()> {
        if external_id.trim().is_empty() {
            return Err(EngineError::InvalidInput(format!(
                "that {source} record has no id, so there is nothing to link it by"
            )));
        }
        if self.storage.get_book(book_id).await?.is_none() {
            return Err(EngineError::NotFound(format!("book {book_id}")));
        }
        self.storage
            .link_external_id(source, external_id, book_id)
            .await
    }

    // ---- flashcards --------------------------------------------------------

    pub async fn list_flashcards(&self, include_exported: bool) -> Result<Vec<FlashcardRow>> {
        self.storage.list_flashcards(include_exported).await
    }

    pub async fn list_flashcards_for_book(&self, book_id: i64) -> Result<Vec<FlashcardRow>> {
        self.storage.list_flashcards_for_book(book_id).await
    }

    /// Build the Anki TSV and mark the exported cards. Returns (tsv, count).
    pub async fn export_flashcards(&self, include_exported: bool) -> Result<(String, usize)> {
        let cards = self.storage.list_flashcards(include_exported).await?;
        let tsv = flashcards::export_tsv(&cards);
        let ids: Vec<i64> = cards.iter().map(|c| c.id).collect();
        self.storage.mark_flashcards_exported(&ids).await?;
        Ok((tsv, ids.len()))
    }
}
