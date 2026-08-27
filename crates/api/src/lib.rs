//! readingbuddy's API — the versioned surface, and the boundary that matters.
//!
//! `docs/decisions.md`: *"the boundary is the API, not the process. A versioned
//! API crate holds the whole surface; the daemon is a thin transport wrapper
//! with no logic. That's what keeps iOS (which has no daemons) able to link the
//! same crate in-process."*
//!
//! So there are two ways to use this crate and they are the same code:
//!
//! - **In-process.** Hold an [`Api`] and call its typed methods. They take and
//!   return [`dto`] types, so a caller is already speaking the wire vocabulary
//!   and can adopt a transport later without rewriting a call site.
//! - **Across a transport.** Hand [`Api::call`] a [`protocol::Call`] and write
//!   back the [`protocol::Reply`]. `crates/daemon` does exactly that and
//!   nothing else — it never names a method.
//!
//! [`Api::dispatch`] is the join between the two, and it is deliberately
//! **pure fan-out**: one arm per request, unpacking arguments and calling the
//! typed method of the same name. No arm decides anything. That is the property
//! that keeps the two ways honest — a rule implemented in `dispatch` would be a
//! rule the in-process caller does not get.
//!
//! ## What crossing this seam costs, and why the cost is paid here
//!
//! The engine's domain types carry `PathBuf`, `OffsetDateTime` and `Duration`,
//! take `&mut Book`, and wrap `sqlx::Error`. None of that can be serialized,
//! and none of it *should* be — see [`dto`] and [`error`] for the arguments.
//! This crate is where the translation lives so that neither the engine nor a
//! frontend has to hold both vocabularies.
//!
//! ## Handles do not cross
//!
//! Three facade methods take a domain struct the caller was previously given
//! back — `update_note_body(&NoteRecord)`, `delete_note(&NoteRecord)`,
//! `file_path(&BookFile)`. Over a transport that would mean the client echoing
//! state back, and a client holding a stale `NoteRecord` would write to a path
//! that has since moved. Here they take an **id** and the row is re-read. Not a
//! translation of the facade so much as a correction of it, and the reason the
//! seam is worth building rather than generating.

pub mod dto;
pub mod error;
pub mod protocol;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use readingbuddy::{
    Book, BookFilter, BookQuery, CalibreImportOptions, DayRange, Engine, EngineError,
    FileImportOptions, GoodreadsImportOptions, NoteKind, NoteRecord, NoteScope, RatingScale,
    ReadingFilter, ReadingQuery,
};

pub use dto::*;
pub use error::{ApiError, ApiResult, ErrorCode};
pub use protocol::{API_VERSION, CRATE_VERSION, Call, Outcome, Reply, Request, Response};

/// The whole surface, over one engine.
///
/// Cheap to clone — it is an `Arc` — because a transport wants one per
/// connection and they must all be the same library.
#[derive(Clone)]
pub struct Api {
    engine: Arc<Engine>,
}

impl Api {
    pub fn new(engine: Arc<Engine>) -> Api {
        Api { engine }
    }

    /// Open a library at `data_dir` and wrap it — for a client that should never
    /// name the engine at all.
    ///
    /// `Api::new` requires an `Arc<Engine>`, so every caller of it depends on
    /// `readingbuddy`. For `readingbuddyd` that is harmless: it is a byte pump
    /// and names no method, so having the engine in scope tempts nothing. For a
    /// **semantic** client it is the whole problem — `gui/CLAUDE.md`'s first rule
    /// is that a gap in this surface must be a compile error rather than a
    /// temptation, and it cannot be either if the engine is one `use` away.
    ///
    /// So this exists to let `gui/src-tauri` depend on this crate and nothing
    /// else. The knobs `EngineConfig` carries beyond the root — a calibre binary
    /// directory, a Google key — are not parameters here on purpose: a client
    /// that needs one is asking for a configuration surface, and that is a
    /// request on this protocol, not a constructor argument.
    pub async fn open(data_dir: &Path) -> ApiResult<Api> {
        let engine = Engine::open(readingbuddy::EngineConfig::rooted_at(data_dir)).await?;
        Ok(Api::new(Arc::new(engine)))
    }

    /// The engine underneath, for a host that also drives it directly — the
    /// mount watcher and the **vault watcher**, neither of which this
    /// vocabulary carries (see [`protocol`]).
    ///
    /// The vault watcher is the load-bearing case for this accessor. A watcher
    /// is a long-lived thing a host owns, not a request/reply pair, and item 24
    /// deliberately kept it off the wire: because the watcher performs the
    /// re-index itself, a host that drives one has correct note searches with no
    /// server-initiated frame, and a host that does not still has
    /// `RefreshNoteFromDisk` and `Engine::reconcile_vault`.
    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    // ---- meta and configuration -------------------------------------------

    pub fn api_version(&self) -> (u32, String) {
        (API_VERSION, CRATE_VERSION.to_string())
    }

    pub fn paths(&self) -> PathsDto {
        PathsDto {
            db_url: self.engine.db_url().to_string(),
            images_dir: self.engine.images_dir().display().to_string(),
            vault_dir: self.engine.vault_dir().display().to_string(),
            files_dir: self.engine.files_dir().display().to_string(),
            log_dir: self.engine.log_dir().display().to_string(),
        }
    }

    /// Whether a key is set, **never the key**.
    ///
    /// A secret that has been written should not be readable back out over a
    /// socket: nothing needs it (the engine already holds it), and a settings
    /// screen shows a mask. `bool` is the honest answer to the only question
    /// anyone actually asks.
    pub fn has_google_api_key(&self) -> bool {
        self.engine.google_api_key().is_some()
    }

    pub fn set_google_api_key(&self, key: Option<String>) {
        self.engine.set_google_api_key(key);
    }

    /// Live-check a key against Google Books. The engine's own verifier, so
    /// there is one definition of "this key works".
    pub async fn verify_google_key(&self, key: &str) -> ApiResult<bool> {
        match readingbuddy::verify_google_key(key).await {
            Ok(()) => Ok(true),
            // A refused key is an answer, not a failure — the caller asked
            // whether it works, and "no" is a valid reply to that question.
            Err(_) => Ok(false),
        }
    }

    // ---- metadata search ---------------------------------------------------

    pub async fn search(&self, request: SearchRequestDto) -> ApiResult<SearchOutcomeDto> {
        Ok(self.engine.search(&request.into()).await?.into())
    }

    pub async fn lookup_isbn(&self, isbn: &str) -> ApiResult<Option<BookDto>> {
        Ok(self.engine.lookup_isbn(isbn).await?.map(Into::into))
    }

    // ---- library -----------------------------------------------------------

    pub async fn save_book(&self, book: BookDto) -> ApiResult<BookDto> {
        Ok(self.engine.save_book(&book.into()).await?.into())
    }

    /// One page of the library (item 18).
    ///
    /// One struct rather than four arguments because `limit` and `offset` are
    /// both `i64` and adjacent, which is a swap no type checker catches. The
    /// **request** carries the same four values flat, so a payload written
    /// against the old two-field method still means what it did.
    pub async fn list_books(&self, query: BookQueryDto) -> ApiResult<Vec<BookDto>> {
        let query: BookQuery = query.into();
        Ok(map(self.engine.list_books(&query).await?))
    }

    /// How many books match. See [`Request::CountBooks`] for why this is its own
    /// call and not a field beside the rows.
    pub async fn count_books(&self, filter: Option<BookFilterDto>) -> ApiResult<i64> {
        let filter: BookFilter = filter.map(Into::into).unwrap_or_default();
        Ok(self.engine.count_books(&filter).await?)
    }

    /// What is behind each of these books, one call for a whole page.
    pub async fn book_summaries(&self, book_ids: &[i64]) -> ApiResult<Vec<BookSummaryDto>> {
        Ok(map(self.engine.book_summaries(book_ids).await?))
    }

    pub async fn get_book(&self, id: i64) -> ApiResult<Option<BookDto>> {
        Ok(self.engine.get_book(id).await?.map(Into::into))
    }

    /// A user-typed selector: id, ISBN, or title fragment. Empty means nothing
    /// matched and more than one means ambiguous — both are answers the caller
    /// resolves, not errors.
    pub async fn resolve_books(&self, selector: &str) -> ApiResult<Vec<BookDto>> {
        Ok(map(self.engine.resolve_books(selector).await?))
    }

    pub async fn book_tags(&self, book_id: i64) -> ApiResult<Vec<BookTagDto>> {
        Ok(map(self.engine.book_tags(book_id).await?))
    }

    /// Ask the providers about a book we already have (item 30).
    pub async fn enrich_book(&self, book_id: i64) -> ApiResult<EnrichReportDto> {
        Ok(self
            .engine
            .enrich_book_from_providers(book_id)
            .await?
            .into())
    }

    /// Record a correction as the user's, and return the book as it now stands.
    ///
    /// Takes a `BookDto` and not a set of named columns, because the engine's
    /// own signature is a partial `Book` and a second vocabulary here would be
    /// a rule the in-process caller never meets. The four reading projections
    /// on it are ignored downstream, where that rule already lives.
    pub async fn set_book_fields(&self, book_id: i64, fields: BookDto) -> ApiResult<BookDto> {
        Ok(self
            .engine
            .set_book_fields(book_id, &Book::from(fields))
            .await?
            .into())
    }

    /// Where each field of a book came from, and when (item 29).
    pub async fn field_provenance(&self, book_id: i64) -> ApiResult<Vec<FieldSourceDto>> {
        Ok(map(self.engine.field_provenance(book_id).await?))
    }

    pub async fn currently_reading(&self, limit: i64) -> ApiResult<Vec<OpenReadingDto>> {
        Ok(map(self.engine.currently_reading(limit).await?))
    }

    pub async fn delete_book(&self, id: i64) -> ApiResult<()> {
        Ok(self.engine.delete_book(id).await?)
    }

    pub async fn fetch_cover(&self, book_id: i64) -> ApiResult<Option<String>> {
        Ok(self
            .engine
            .fetch_cover(book_id)
            .await?
            .map(|p| p.display().to_string()))
    }

    pub async fn merge_books(&self, src: i64, dst: i64) -> ApiResult<MergeReportDto> {
        Ok(self.engine.merge_books(src, dst).await?.into())
    }

    // ---- readings ----------------------------------------------------------

    /// Every reading of a book, oldest first, each carrying **its own**
    /// progress (item 22).
    ///
    /// Through `readings_with_progress` rather than `list_readings`, because
    /// pairing a read with the book's length is a derivation and this layer
    /// must not do it — see `ReadingDto::progress`.
    pub async fn list_readings(&self, book_id: i64) -> ApiResult<Vec<ReadingDto>> {
        Ok(self
            .engine
            .readings_with_progress(book_id)
            .await?
            .into_iter()
            .map(|(r, p)| ReadingDto::new(r, p))
            .collect())
    }

    /// One page of the library's readings (item 43).
    ///
    /// One struct rather than four arguments, for [`Api::list_books`]' reason.
    /// **Fallible before it reaches the engine**, and only because of the year:
    /// the filter's `finished_in` is validated by the engine's own `DayRange`,
    /// so an inverted span is refused here exactly as it is for
    /// `ActivitySummary` rather than becoming a confident empty wall.
    pub async fn list_reading_rows(&self, query: ReadingQueryDto) -> ApiResult<Vec<ReadingRowDto>> {
        let query: ReadingQuery = query.try_into()?;
        Ok(map(self.engine.list_reading_rows(&query).await?))
    }

    /// How many readings match. See [`Request::CountReadings`] for why this is
    /// its own call and not a field beside the rows.
    pub async fn count_readings(&self, filter: Option<ReadingFilterDto>) -> ApiResult<i64> {
        let filter: ReadingFilter = filter
            .map(TryInto::try_into)
            .transpose()?
            .unwrap_or_default();
        Ok(self.engine.count_readings(&filter).await?)
    }

    /// Which years this filter's readings ended in (item 51).
    ///
    /// Fallible for `list_reading_rows`' reason and through the identical
    /// conversion: `finished_in` is a `DayRange` the engine validates, so an
    /// inverted span is `InvalidInput` from both doors rather than a picker
    /// that confidently offers nothing.
    pub async fn reading_years(
        &self,
        filter: Option<ReadingFilterDto>,
    ) -> ApiResult<ReadingYearsDto> {
        let filter: ReadingFilter = filter
            .map(TryInto::try_into)
            .transpose()?
            .unwrap_or_default();
        Ok(self.engine.reading_years(&filter).await?.into())
    }

    pub async fn get_reading(&self, id: i64) -> ApiResult<Option<ReadingDto>> {
        Ok(self
            .engine
            .reading_with_progress(id)
            .await?
            .map(|(r, p)| ReadingDto::new(r, p)))
    }

    pub async fn active_reading(&self, book_id: i64) -> ApiResult<Option<ReadingDto>> {
        Ok(self
            .engine
            .active_reading_with_progress(book_id)
            .await?
            .map(|(r, p)| ReadingDto::new(r, p)))
    }

    pub async fn update_progress(
        &self,
        book_id: i64,
        page: Option<i64>,
        finished: Option<bool>,
    ) -> ApiResult<BookDto> {
        Ok(self
            .engine
            .update_progress(book_id, page, finished)
            .await?
            .into())
    }

    pub async fn reread(&self, book_id: i64) -> ApiResult<i64> {
        Ok(self.engine.reread(book_id).await?)
    }

    // ---- highlights --------------------------------------------------------

    pub async fn list_highlights(&self, book_id: i64) -> ApiResult<Vec<HighlightDto>> {
        Ok(map(self.engine.list_highlights(book_id).await?))
    }

    pub async fn highlights_for_reading(&self, reading_id: i64) -> ApiResult<Vec<HighlightDto>> {
        Ok(map(self.engine.highlights_for_reading(reading_id).await?))
    }

    /// The one passage a card shows for this reading (item 44), or `None`.
    ///
    /// Which passage is the engine's rule, not this layer's and not a client's;
    /// `Engine::card_passage` carries the argument. Served as its own method
    /// rather than a field on `ReadingDto` so that the reader's highlight text
    /// does not ride along on every row of every reading list.
    pub async fn card_passage(&self, reading_id: i64) -> ApiResult<Option<HighlightDto>> {
        Ok(self.engine.card_passage(reading_id).await?.map(Into::into))
    }

    pub async fn set_annotation(
        &self,
        highlight_id: i64,
        annotation: Option<&str>,
    ) -> ApiResult<()> {
        Ok(self.engine.set_annotation(highlight_id, annotation).await?)
    }

    // ---- epub and owned files ----------------------------------------------

    pub async fn import_epub(&self, path: &Path) -> ApiResult<BookDto> {
        Ok(self.engine.import_epub(path).await?.into())
    }

    pub async fn import_file(&self, path: &Path, new: bool) -> ApiResult<FileImportReportDto> {
        Ok(self
            .engine
            .import_file(path, FileImportOptions { new })
            .await?
            .into())
    }

    pub async fn add_file_to_book(
        &self,
        book_id: i64,
        path: &Path,
    ) -> ApiResult<FileImportReportDto> {
        Ok(self.engine.add_file_to_book(book_id, path).await?.into())
    }

    pub async fn identify_file(&self, path: &Path) -> ApiResult<FileIdentityDto> {
        Ok(self.engine.identify_file(path).await?.into())
    }

    pub async fn book_files(&self, book_id: i64) -> ApiResult<Vec<BookFileDto>> {
        Ok(map(self.engine.book_files(book_id).await?))
    }

    /// The book's own chapter list, from the first epub it owns.
    ///
    /// `None` means there is no file here we can read; `Some` with no entries
    /// means the epub carries no navigable TOC. Both are ordinary and they are
    /// different — see [`TableOfContentsDto`].
    pub async fn table_of_contents(&self, book_id: i64) -> ApiResult<Option<TableOfContentsDto>> {
        Ok(self
            .engine
            .table_of_contents(book_id)
            .await?
            .map(Into::into))
    }

    /// Where a file's bytes are, from its content address.
    ///
    /// By sha256, not by the `BookFile` the facade takes: see the crate doc on
    /// handles. It also means the answer is derived from the row the engine
    /// holds now rather than from whatever the client last saw.
    pub async fn file_path(&self, sha256: &str) -> ApiResult<String> {
        let file = self.engine.book_file(sha256).await?.ok_or_else(|| {
            ApiError::new(
                ErrorCode::NotFound,
                format!("no owned file with sha {sha256}"),
            )
        })?;
        Ok(self.engine.file_path(&file).display().to_string())
    }

    pub async fn remove_file(&self, sha256: &str) -> ApiResult<bool> {
        Ok(self.engine.remove_file(sha256).await?)
    }

    // ---- koreader ----------------------------------------------------------

    pub async fn import_koreader(&self, path: &Path, dry_run: bool) -> ApiResult<ImportReportDto> {
        Ok(self.engine.import_koreader(path, dry_run).await?.into())
    }

    pub async fn pull_book_from_sidecar(&self, path: &Path) -> ApiResult<PullReportDto> {
        Ok(self.engine.pull_book_from_sidecar(path).await?.into())
    }

    pub async fn sidecar_candidates(&self, path: &Path) -> ApiResult<Vec<MatchCandidateDto>> {
        Ok(map(self.engine.sidecar_candidates(path).await?))
    }

    /// Record that this sidecar is that book. Returns the `partial_md5` the
    /// link was keyed on, so a caller can say what it recorded.
    pub async fn link_sidecar(&self, path: &Path, book_id: i64) -> ApiResult<String> {
        Ok(self.engine.link_sidecar(path, book_id).await?)
    }

    // ---- the device --------------------------------------------------------

    pub fn candidate_mounts(&self) -> Vec<String> {
        readingbuddy::candidate_mounts()
            .iter()
            .map(|p| p.display().to_string())
            .collect()
    }

    pub fn is_koreader_mount(&self, path: &Path) -> bool {
        readingbuddy::is_koreader_mount(path)
    }

    pub async fn scan_device(&self, root: &Path) -> ApiResult<DeviceScanDto> {
        Ok(self.engine.scan_device(root).await?.into())
    }

    pub async fn sync_device(&self, paths: &[PathBuf]) -> ApiResult<Vec<PullReportDto>> {
        Ok(map(self.engine.sync_device(paths).await?))
    }

    // ---- the plugin (item 15a) ---------------------------------------------

    /// What readingbuddy's plugin looks like on a mounted reader. Read-only.
    pub async fn plugin_status(&self, mount: &Path) -> ApiResult<PluginStatusDto> {
        Ok(self.engine.plugin_status(mount).await?.into())
    }

    /// Install or upgrade the plugin, and pair with the reader.
    ///
    /// **Never call this on a mount event.** See the request's doc.
    pub async fn install_plugin(&self, mount: &Path) -> ApiResult<InstallReportDto> {
        Ok(self.engine.install_plugin(mount).await?.into())
    }

    /// Remove exactly what was installed, and forget the pairing.
    pub async fn uninstall_plugin(&self, mount: &Path) -> ApiResult<UninstallReportDto> {
        Ok(self.engine.uninstall_plugin(mount).await?.into())
    }

    /// Every paired reader, without its token — see [`PairedDeviceDto`].
    pub async fn paired_devices(&self) -> ApiResult<Vec<PairedDeviceDto>> {
        Ok(map(self.engine.paired_devices().await?))
    }

    /// Measured reading time out of a mounted device's `statistics.sqlite3`
    /// (item 31), as rows in the activity log.
    ///
    /// **Not on `sync_device`'s path**, deliberately — see the request's doc.
    /// A device whose owner never enabled the statistics plugin comes back with
    /// an empty report carrying a warning, never an error.
    pub async fn import_device_statistics(&self, mount: &Path) -> ApiResult<StatsImportReportDto> {
        Ok(self.engine.import_device_statistics(mount).await?.into())
    }

    // ---- the activity log --------------------------------------------------

    /// Rebuild the log from what is already stored. Idempotent.
    pub async fn refill_reading_events(&self) -> ApiResult<RefillReportDto> {
        Ok(self.engine.refill_reading_events().await?.into())
    }

    /// One book's activity log, oldest day first.
    pub async fn reading_events(&self, book_id: i64) -> ApiResult<Vec<ReadingEventDto>> {
        Ok(map(self.engine.reading_events(book_id).await?))
    }

    /// What is known about a period.
    ///
    /// The two days are validated **here**, by the engine's own `DayRange`, so
    /// an inverted or malformed span is an `InvalidInput` rather than a
    /// confident zero — the failure `DayRange::new` exists to prevent, and one
    /// this layer must not be able to route around.
    pub async fn activity_summary(&self, from: &str, to: &str) -> ApiResult<ActivitySummaryDto> {
        let range = DayRange::new(from, to)?;
        Ok(self.engine.activity_summary(&range).await?.into())
    }

    /// The days of a period that carry an event, oldest first.
    pub async fn activity_by_day(&self, from: &str, to: &str) -> ApiResult<Vec<DayActivityDto>> {
        let range = DayRange::new(from, to)?;
        Ok(map(self.engine.activity_by_day(&range).await?))
    }

    /// The months of a period that carry an event, oldest first (item 42).
    ///
    /// Validated through the engine's own `DayRange` like its two siblings, so
    /// an inverted span is refused here too rather than becoming an empty year.
    pub async fn activity_by_month(
        &self,
        from: &str,
        to: &str,
    ) -> ApiResult<Vec<MonthActivityDto>> {
        let range = DayRange::new(from, to)?;
        Ok(map(self.engine.activity_by_month(&range).await?))
    }

    // ---- moments -----------------------------------------------------------

    /// What is worth noticing and has not been shown yet, newest first.
    ///
    /// Derived on every call and stored nowhere, so this is safe to poll — and
    /// polling is the only way to read it, because this protocol has no push
    /// channel and a moment stream would reopen the argument
    /// [`protocol`]'s module doc settles about the mount watcher.
    pub async fn pending_moments(&self, limit: Option<i64>) -> ApiResult<Vec<MomentDto>> {
        Ok(map(self.engine.pending_moments(limit).await?))
    }

    /// Record that a moment was surfaced. Idempotent.
    pub async fn acknowledge_moment(&self, id: &str) -> ApiResult<()> {
        Ok(self.engine.acknowledge_moment(id).await?)
    }

    // ---- notes -------------------------------------------------------------

    pub async fn create_note(&self, note: NewNoteDto) -> ApiResult<CreatedNoteDto> {
        Ok(self.engine.create_note(note.into()).await?.into())
    }

    /// Notes, newest first. Both narrowings absent is every note — see
    /// [`Request::ListNotes`].
    ///
    /// The two ids are **alternatives, not a conjunction**, and the wire says so
    /// by refusing the pair rather than by quietly preferring one: a reading
    /// belongs to exactly one book, so `{book_id, reading_id}` is either
    /// redundant or a contradiction whose honest answer is an empty list that a
    /// client cannot tell from an empty vault. `NoteScope` is the shape below
    /// this line and cannot represent it at all.
    pub async fn list_notes(
        &self,
        book_id: Option<i64>,
        reading_id: Option<i64>,
        limit: Option<i64>,
    ) -> ApiResult<Vec<NoteDto>> {
        let scope = match (book_id, reading_id) {
            (Some(_), Some(_)) => {
                return Err(ApiError::new(
                    ErrorCode::InvalidInput,
                    "list_notes takes book_id or reading_id, not both — a reading names its book",
                ));
            }
            (_, Some(r)) => NoteScope::Reading(r),
            (b, None) => NoteScope::of_book(b),
        };
        Ok(map(self.engine.list_notes(scope, limit).await?))
    }

    /// Notes and highlights matching one query, as one ranked list.
    ///
    /// See [`Request::SearchMarks`] for why there is one method here and not
    /// two, and `readingbuddy`'s `storage::fts` for the ordering rule and for
    /// why `book_id` has to be answered below this line.
    pub async fn search_marks(
        &self,
        query: &str,
        source: Option<SearchSourceDto>,
        book_id: Option<i64>,
        limit: i64,
    ) -> ApiResult<Vec<SearchHitDto>> {
        Ok(map(self
            .engine
            .search_marks(query, source.map(Into::into), book_id, limit)
            .await?))
    }

    pub async fn get_note(&self, id: i64) -> ApiResult<Option<NoteDto>> {
        Ok(self.engine.get_note(id).await?.map(Into::into))
    }

    pub async fn note_for_reading(
        &self,
        reading_id: i64,
        kind: NoteKindDto,
    ) -> ApiResult<Option<NoteDto>> {
        let kind: NoteKind = kind.into();
        Ok(self
            .engine
            .note_for_reading(reading_id, kind.as_str())
            .await?
            .map(Into::into))
    }

    pub async fn note_path(&self, note_id: i64) -> ApiResult<String> {
        let note = self.note(note_id).await?;
        Ok(self.engine.note_path(&note).display().to_string())
    }

    pub async fn note_body(&self, note_id: i64) -> ApiResult<String> {
        let note = self.note(note_id).await?;
        Ok(self.engine.note_body(&note)?)
    }

    pub async fn update_note_body(&self, note_id: i64, body: &str) -> ApiResult<()> {
        let note = self.note(note_id).await?;
        Ok(self.engine.update_note_body(&note, body).await?)
    }

    pub async fn delete_note(&self, note_id: i64) -> ApiResult<()> {
        let note = self.note(note_id).await?;
        Ok(self.engine.delete_note(&note).await?)
    }

    pub async fn refresh_note_from_disk(&self, note_id: i64) -> ApiResult<()> {
        let note = self.note(note_id).await?;
        Ok(self.engine.refresh_note_from_disk(&note).await?)
    }

    pub async fn outgoing_links(&self, note_id: i64) -> ApiResult<Vec<OutgoingLinkDto>> {
        Ok(map(self.engine.outgoing_links(note_id).await?))
    }

    pub async fn backlinks(&self, note_id: i64) -> ApiResult<Vec<NoteDto>> {
        Ok(map(self.engine.backlinks(note_id).await?))
    }

    // ---- reflection and review ---------------------------------------------

    pub async fn open_reflection(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
    ) -> ApiResult<CreatedNoteDto> {
        Ok(self
            .engine
            .open_reflection(book_id, reading_id)
            .await?
            .into())
    }

    pub async fn open_review(
        &self,
        book_id: i64,
        reading_id: Option<i64>,
    ) -> ApiResult<CreatedNoteDto> {
        Ok(self.engine.open_review(book_id, reading_id).await?.into())
    }

    // ---- ratings -----------------------------------------------------------

    pub async fn set_rating(&self, note_id: i64, value: f64) -> ApiResult<()> {
        Ok(self.engine.set_rating(note_id, value).await?)
    }

    pub async fn review_rating(&self, note_id: i64) -> ApiResult<Option<RatingDto>> {
        Ok(self.engine.review_rating(note_id).await?.map(Into::into))
    }

    pub async fn clear_review_rating(&self, note_id: i64) -> ApiResult<bool> {
        Ok(self.engine.clear_review_rating(note_id).await?)
    }

    /// This review on Goodreads' integer 0–5.
    ///
    /// An unmapped value is [`ErrorCode::UnmappedRating`] and never a rounded
    /// integer — the lookup table exists precisely to refuse that, and a
    /// transport is not a reason to start guessing.
    pub async fn goodreads_rating(&self, note_id: i64) -> ApiResult<Option<u8>> {
        Ok(self.engine.goodreads_rating(note_id).await?)
    }

    pub async fn put_rating_scale(
        &self,
        name: &str,
        min: f64,
        max: f64,
        step: f64,
    ) -> ApiResult<RatingScaleDto> {
        Ok(self
            .engine
            .put_rating_scale(name, min, max, step)
            .await?
            .into())
    }

    pub async fn rating_scales(&self) -> ApiResult<Vec<RatingScaleDto>> {
        Ok(map(self.engine.rating_scales().await?))
    }

    pub async fn active_rating_scale(&self) -> ApiResult<Option<RatingScaleDto>> {
        Ok(self.engine.active_rating_scale().await?.map(Into::into))
    }

    pub async fn rating_scale_by_name(&self, name: &str) -> ApiResult<Option<RatingScaleDto>> {
        Ok(self
            .engine
            .rating_scale_by_name(name)
            .await?
            .map(Into::into))
    }

    pub async fn rating_map(&self, scale_id: i64) -> ApiResult<Vec<RatingMapEntryDto>> {
        Ok(self
            .engine
            .rating_map(scale_id)
            .await?
            .into_iter()
            .map(|(value, goodreads)| RatingMapEntryDto { value, goodreads })
            .collect())
    }

    /// Map one point of a scale onto a Goodreads integer.
    ///
    /// By `scale_id`, and the scale is re-read here — the facade takes a whole
    /// `&RatingScale`, and a client echoing one back could map a point against
    /// bounds that have since been redefined.
    pub async fn map_rating(&self, scale_id: i64, value: f64, goodreads: u8) -> ApiResult<()> {
        let scale = self.scale(scale_id).await?;
        Ok(self.engine.map_rating(&scale, value, goodreads).await?)
    }

    // ---- citations ---------------------------------------------------------

    pub async fn cite(&self, note_id: i64, highlight_id: i64) -> ApiResult<()> {
        Ok(self.engine.cite(note_id, highlight_id).await?)
    }

    pub async fn uncite(&self, note_id: i64, highlight_id: i64) -> ApiResult<bool> {
        Ok(self.engine.uncite(note_id, highlight_id).await?)
    }

    pub async fn citations_for(&self, note_id: i64) -> ApiResult<Vec<HighlightDto>> {
        Ok(map(self.engine.citations_for(note_id).await?))
    }

    /// Which passages each of these notes cites, in one call (item 46).
    ///
    /// One entry per id, in the order asked, empties included — the shape
    /// `book_summaries` set, and for the same reason: a caller zips it against
    /// a page it already has. Ids rather than rows; see [`NoteCitationsDto`].
    pub async fn citations_for_notes(&self, note_ids: &[i64]) -> ApiResult<Vec<NoteCitationsDto>> {
        Ok(map(self.engine.citations_for_notes(note_ids).await?))
    }

    // ---- goodreads ---------------------------------------------------------

    pub async fn import_goodreads(
        &self,
        path: &Path,
        dry_run: bool,
        create_ambiguous: bool,
    ) -> ApiResult<GoodreadsReportDto> {
        Ok(self
            .engine
            .import_goodreads(
                path,
                GoodreadsImportOptions {
                    dry_run,
                    create_ambiguous,
                },
            )
            .await?
            .into())
    }

    pub async fn export_goodreads(&self) -> ApiResult<(String, Vec<DiagnosticDto>)> {
        let (csv, warnings) = self.engine.export_goodreads().await?;
        Ok((csv, map(warnings)))
    }

    /// Record that a Goodreads row is that book, keyed on Goodreads' `Book Id`.
    pub async fn link_goodreads_row(&self, external_id: &str, book_id: i64) -> ApiResult<()> {
        Ok(self.engine.link_goodreads_row(external_id, book_id).await?)
    }

    // ---- calibre -----------------------------------------------------------

    /// Which calibre tools this machine has.
    ///
    /// Both `None` is a perfectly good answer and **not an error**: calibre is
    /// feature-detected, so a client shows the feature as absent and never tells
    /// anyone to install anything.
    pub fn calibre_status(&self) -> CalibreStatusDto {
        self.engine.calibre().into()
    }

    pub async fn convert_ebook(
        &self,
        input: &Path,
        output: &Path,
        overwrite: bool,
    ) -> ApiResult<String> {
        Ok(self
            .engine
            .convert_ebook(input, output, overwrite)
            .await?
            .display()
            .to_string())
    }

    pub async fn calibre_library(&self, library: Option<&Path>) -> ApiResult<Vec<CalibreBookDto>> {
        Ok(map(self.engine.calibre_library(library).await?))
    }

    pub async fn import_calibre_library(
        &self,
        library: Option<PathBuf>,
        dry_run: bool,
        create_ambiguous: bool,
        only: Vec<i64>,
    ) -> ApiResult<CalibreReportDto> {
        Ok(self
            .engine
            .import_calibre_library(&CalibreImportOptions {
                library,
                dry_run,
                create_ambiguous,
                only,
            })
            .await?
            .into())
    }

    /// Record that a calibre book is that book of ours, keyed on calibre's uuid.
    pub async fn link_calibre_book(&self, uuid: &str, book_id: i64) -> ApiResult<()> {
        Ok(self.engine.link_calibre_book(uuid, book_id).await?)
    }

    // ---- flashcards --------------------------------------------------------

    /// Capture a card (item 45). `true` created, `false` you already had it.
    ///
    /// Both ids are re-read in the engine before anything is written — the pair
    /// is two handles a client supplies independently, and nothing in the
    /// schema stops them naming different books.
    pub async fn create_flashcard(
        &self,
        book_id: i64,
        highlight_id: Option<i64>,
        word: &str,
        context: Option<&str>,
    ) -> ApiResult<bool> {
        Ok(self
            .engine
            .create_flashcard(book_id, highlight_id, word, context)
            .await?)
    }

    pub async fn list_flashcards(&self, include_exported: bool) -> ApiResult<Vec<FlashcardDto>> {
        Ok(map(self.engine.list_flashcards(include_exported).await?))
    }

    pub async fn list_flashcards_for_book(&self, book_id: i64) -> ApiResult<Vec<FlashcardDto>> {
        Ok(map(self.engine.list_flashcards_for_book(book_id).await?))
    }

    pub async fn export_flashcards(&self, include_exported: bool) -> ApiResult<(String, usize)> {
        Ok(self.engine.export_flashcards(include_exported).await?)
    }

    // ---- the two lookups the id-taking methods share -----------------------

    async fn note(&self, note_id: i64) -> ApiResult<NoteRecord> {
        self.engine
            .get_note(note_id)
            .await?
            .ok_or_else(|| ApiError::from(EngineError::NotFound(format!("note id {note_id}"))))
    }

    async fn scale(&self, scale_id: i64) -> ApiResult<RatingScale> {
        self.engine
            .rating_scales()
            .await?
            .into_iter()
            .find(|s| s.id == scale_id)
            .ok_or_else(|| ApiError::from(EngineError::NotFound(format!("scale id {scale_id}"))))
    }

    // ---- the join ----------------------------------------------------------

    /// Run one call and produce a reply that is always well-formed.
    ///
    /// Never returns `Err`: a transport's job is to write *something* back for
    /// every line it read, and a failure it has to invent a reply for is a
    /// failure it has to have logic about.
    pub async fn call(&self, call: Call) -> Reply {
        match self.dispatch(call.request).await {
            Ok(response) => Reply::ok(call.id, response),
            Err(error) => Reply::err(call.id, error),
        }
    }

    /// One arm per request, and **no arm decides anything**.
    ///
    /// Every line here unpacks arguments and calls the typed method above it.
    /// A rule that lived in this match would be a rule an in-process caller
    /// never met, and the two ways of using this crate would stop being the
    /// same code.
    pub async fn dispatch(&self, request: Request) -> ApiResult<Response> {
        use Request as R;
        Ok(match request {
            R::ApiVersion => {
                let (api, crate_version) = self.api_version();
                Response::Version { api, crate_version }
            }
            R::Paths => Response::Where(self.paths()),
            R::GoogleApiKey => Response::Bool(self.has_google_api_key()),
            R::SetGoogleApiKey { key } => {
                self.set_google_api_key(key);
                Response::Unit
            }
            R::VerifyGoogleKey { key } => Response::Bool(self.verify_google_key(&key).await?),

            R::Search { request } => Response::SearchOutcome(self.search(request).await?),
            R::LookupIsbn { isbn } => Response::Book(self.lookup_isbn(&isbn).await?),

            R::SaveBook { book } => Response::Book(Some(self.save_book(book).await?)),
            // Assembling the struct is unpacking, not deciding: the request
            // carries the four values flat for wire compatibility and the typed
            // method takes them as one. No arm here may do more than this.
            R::ListBooks {
                limit,
                sort,
                offset,
                filter,
            } => Response::Books(
                self.list_books(BookQueryDto {
                    sort,
                    filter,
                    limit,
                    offset,
                })
                .await?,
            ),
            R::CountBooks { filter } => Response::Count(self.count_books(filter).await?),
            R::BookSummaries { book_ids } => {
                Response::BookSummaries(self.book_summaries(&book_ids).await?)
            }
            R::GetBook { id } => Response::Book(self.get_book(id).await?),
            R::ResolveBooks { selector } => Response::Books(self.resolve_books(&selector).await?),
            R::BookTags { book_id } => Response::BookTags(self.book_tags(book_id).await?),
            R::EnrichBook { book_id } => Response::EnrichReport(self.enrich_book(book_id).await?),
            R::SetBookFields { book_id, fields } => {
                Response::Book(Some(self.set_book_fields(book_id, fields).await?))
            }
            R::FieldProvenance { book_id } => {
                Response::FieldProvenance(self.field_provenance(book_id).await?)
            }
            R::CurrentlyReading { limit } => {
                Response::OpenReadings(self.currently_reading(limit).await?)
            }
            R::DeleteBook { id } => {
                self.delete_book(id).await?;
                Response::Unit
            }
            R::FetchCover { book_id } => Response::MaybePath(self.fetch_cover(book_id).await?),
            R::MergeBooks { src, dst } => Response::MergeReport(self.merge_books(src, dst).await?),

            R::ListReadings { book_id } => Response::Readings(self.list_readings(book_id).await?),
            R::ListReadingRows {
                limit,
                sort,
                offset,
                filter,
            } => Response::ReadingRows(
                self.list_reading_rows(ReadingQueryDto {
                    sort,
                    filter,
                    limit,
                    offset,
                })
                .await?,
            ),
            R::CountReadings { filter } => Response::Count(self.count_readings(filter).await?),
            R::ReadingYears { filter } => Response::ReadingYears(self.reading_years(filter).await?),
            R::GetReading { id } => Response::Reading(self.get_reading(id).await?),
            R::ActiveReading { book_id } => Response::Reading(self.active_reading(book_id).await?),
            R::UpdateProgress {
                book_id,
                page,
                finished,
            } => Response::Book(Some(self.update_progress(book_id, page, finished).await?)),
            R::Reread { book_id } => Response::Id(self.reread(book_id).await?),

            R::ListHighlights { book_id } => {
                Response::Highlights(self.list_highlights(book_id).await?)
            }
            R::HighlightsForReading { reading_id } => {
                Response::Highlights(self.highlights_for_reading(reading_id).await?)
            }
            R::CardPassage { reading_id } => {
                Response::Highlight(self.card_passage(reading_id).await?)
            }
            R::SetAnnotation {
                highlight_id,
                annotation,
            } => {
                self.set_annotation(highlight_id, annotation.as_deref())
                    .await?;
                Response::Unit
            }

            R::ImportEpub { path } => {
                Response::Book(Some(self.import_epub(Path::new(&path)).await?))
            }
            R::ImportFile { path, new } => {
                Response::FileImport(self.import_file(Path::new(&path), new).await?)
            }
            R::AddFileToBook { book_id, path } => {
                Response::FileImport(self.add_file_to_book(book_id, Path::new(&path)).await?)
            }
            R::IdentifyFile { path } => {
                Response::FileIdentity(self.identify_file(Path::new(&path)).await?)
            }
            R::BookFiles { book_id } => Response::BookFiles(self.book_files(book_id).await?),
            R::TableOfContents { book_id } => {
                Response::TableOfContents(self.table_of_contents(book_id).await?)
            }
            R::FilePath { sha256 } => Response::Text(self.file_path(&sha256).await?),
            R::RemoveFile { sha256 } => Response::Bool(self.remove_file(&sha256).await?),

            R::ImportKoreader { path, dry_run } => {
                Response::ImportReport(self.import_koreader(Path::new(&path), dry_run).await?)
            }
            R::PullBookFromSidecar { path } => {
                Response::PullReport(self.pull_book_from_sidecar(Path::new(&path)).await?)
            }
            R::SidecarCandidates { path } => {
                Response::Candidates(self.sidecar_candidates(Path::new(&path)).await?)
            }
            R::LinkSidecar { path, book_id } => {
                Response::Text(self.link_sidecar(Path::new(&path), book_id).await?)
            }

            R::CandidateMounts => Response::Paths(self.candidate_mounts()),
            R::IsKoreaderMount { path } => Response::Bool(self.is_koreader_mount(Path::new(&path))),
            R::ScanDevice { root } => {
                Response::DeviceScan(self.scan_device(Path::new(&root)).await?)
            }
            R::PluginStatus { mount } => {
                Response::PluginStatus(self.plugin_status(Path::new(&mount)).await?)
            }
            R::InstallPlugin { mount } => {
                Response::PluginInstalled(self.install_plugin(Path::new(&mount)).await?)
            }
            R::UninstallPlugin { mount } => {
                Response::PluginUninstalled(self.uninstall_plugin(Path::new(&mount)).await?)
            }
            R::PairedDevices => Response::PairedDevices(self.paired_devices().await?),
            R::SyncDevice { paths } => {
                let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
                Response::PullReports(self.sync_device(&paths).await?)
            }
            R::ImportDeviceStatistics { mount } => {
                Response::StatsImport(self.import_device_statistics(Path::new(&mount)).await?)
            }

            R::RefillReadingEvents => Response::RefillReport(self.refill_reading_events().await?),
            R::ReadingEvents { book_id } => {
                Response::ReadingEvents(self.reading_events(book_id).await?)
            }
            R::PendingMoments { limit } => Response::Moments(self.pending_moments(limit).await?),
            R::AcknowledgeMoment { id } => {
                self.acknowledge_moment(&id).await?;
                Response::Unit
            }
            R::ActivitySummary { from, to } => {
                Response::ActivitySummary(self.activity_summary(&from, &to).await?)
            }
            R::ActivityByDay { from, to } => {
                Response::ActivityByDay(self.activity_by_day(&from, &to).await?)
            }
            R::ActivityByMonth { from, to } => {
                Response::ActivityByMonth(self.activity_by_month(&from, &to).await?)
            }

            R::CreateNote { note } => Response::CreatedNote(self.create_note(note).await?),
            R::ListNotes {
                book_id,
                reading_id,
                limit,
            } => Response::Notes(self.list_notes(book_id, reading_id, limit).await?),
            R::SearchMarks {
                query,
                source,
                book_id,
                limit,
            } => Response::SearchHits(self.search_marks(&query, source, book_id, limit).await?),
            R::GetNote { id } => Response::Note(self.get_note(id).await?),
            R::NoteForReading { reading_id, kind } => {
                Response::Note(self.note_for_reading(reading_id, kind).await?)
            }
            R::NotePath { note_id } => Response::Text(self.note_path(note_id).await?),
            R::NoteBody { note_id } => Response::Text(self.note_body(note_id).await?),
            R::UpdateNoteBody { note_id, body } => {
                self.update_note_body(note_id, &body).await?;
                Response::Unit
            }
            R::DeleteNote { note_id } => {
                self.delete_note(note_id).await?;
                Response::Unit
            }
            R::RefreshNoteFromDisk { note_id } => {
                self.refresh_note_from_disk(note_id).await?;
                Response::Unit
            }
            R::OutgoingLinks { note_id } => Response::Links(self.outgoing_links(note_id).await?),
            R::Backlinks { note_id } => Response::Notes(self.backlinks(note_id).await?),

            R::OpenReflection {
                book_id,
                reading_id,
            } => Response::CreatedNote(self.open_reflection(book_id, reading_id).await?),
            R::OpenReview {
                book_id,
                reading_id,
            } => Response::CreatedNote(self.open_review(book_id, reading_id).await?),

            R::SetRating { note_id, value } => {
                self.set_rating(note_id, value).await?;
                Response::Unit
            }
            R::ReviewRating { note_id } => Response::Rating(self.review_rating(note_id).await?),
            R::ClearReviewRating { note_id } => {
                Response::Bool(self.clear_review_rating(note_id).await?)
            }
            R::GoodreadsRating { note_id } => {
                Response::GoodreadsRating(self.goodreads_rating(note_id).await?)
            }
            R::PutRatingScale {
                name,
                min,
                max,
                step,
            } => Response::RatingScale(Some(self.put_rating_scale(&name, min, max, step).await?)),
            R::RatingScales => Response::RatingScales(self.rating_scales().await?),
            R::ActiveRatingScale => Response::RatingScale(self.active_rating_scale().await?),
            R::RatingScaleByName { name } => {
                Response::RatingScale(self.rating_scale_by_name(&name).await?)
            }
            R::RatingMap { scale_id } => Response::RatingMap(self.rating_map(scale_id).await?),
            R::MapRating {
                scale_id,
                value,
                goodreads,
            } => {
                self.map_rating(scale_id, value, goodreads).await?;
                Response::Unit
            }

            R::Cite {
                note_id,
                highlight_id,
            } => {
                self.cite(note_id, highlight_id).await?;
                Response::Unit
            }
            R::Uncite {
                note_id,
                highlight_id,
            } => Response::Bool(self.uncite(note_id, highlight_id).await?),
            R::CitationsFor { note_id } => Response::Highlights(self.citations_for(note_id).await?),
            R::CitationsForNotes { note_ids } => {
                Response::NoteCitations(self.citations_for_notes(&note_ids).await?)
            }

            R::ImportGoodreads {
                path,
                dry_run,
                create_ambiguous,
            } => Response::GoodreadsReport(
                self.import_goodreads(Path::new(&path), dry_run, create_ambiguous)
                    .await?,
            ),
            R::ExportGoodreads => {
                let (csv, warnings) = self.export_goodreads().await?;
                Response::GoodreadsExport { csv, warnings }
            }
            R::LinkGoodreadsRow {
                external_id,
                book_id,
            } => {
                self.link_goodreads_row(&external_id, book_id).await?;
                Response::Unit
            }

            R::CalibreStatus => Response::CalibreStatus(self.calibre_status()),
            R::ConvertEbook {
                input,
                output,
                overwrite,
            } => Response::Text(
                self.convert_ebook(Path::new(&input), Path::new(&output), overwrite)
                    .await?,
            ),
            R::CalibreLibrary { library } => Response::CalibreLibrary(
                self.calibre_library(library.as_deref().map(Path::new))
                    .await?,
            ),
            R::ImportCalibreLibrary {
                library,
                dry_run,
                create_ambiguous,
                only,
            } => Response::CalibreReport(
                self.import_calibre_library(
                    library.map(PathBuf::from),
                    dry_run,
                    create_ambiguous,
                    only,
                )
                .await?,
            ),
            R::LinkCalibreBook { uuid, book_id } => {
                self.link_calibre_book(&uuid, book_id).await?;
                Response::Unit
            }

            R::CreateFlashcard {
                book_id,
                highlight_id,
                word,
                context,
            } => Response::Bool(
                self.create_flashcard(book_id, highlight_id, &word, context.as_deref())
                    .await?,
            ),
            R::ListFlashcards { include_exported } => {
                Response::Flashcards(self.list_flashcards(include_exported).await?)
            }
            R::ListFlashcardsForBook { book_id } => {
                Response::Flashcards(self.list_flashcards_for_book(book_id).await?)
            }
            R::ExportFlashcards { include_exported } => {
                let (tsv, count) = self.export_flashcards(include_exported).await?;
                Response::FlashcardExport { tsv, count }
            }
        })
    }
}

/// `Vec<Domain>` to `Vec<Dto>`, once, instead of at forty call sites.
fn map<T, U: From<T>>(items: Vec<T>) -> Vec<U> {
    items.into_iter().map(U::from).collect()
}
