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
    BookSort, CalibreImportOptions, Engine, EngineError, FileImportOptions, GoodreadsImportOptions,
    NoteKind, NoteRecord, RatingScale,
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

    /// The engine underneath, for a host that also drives it directly — the
    /// mount watcher, for one, which this vocabulary deliberately does not
    /// carry (see [`protocol`]).
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

    pub async fn list_books(&self, limit: i64, sort: BookSortDto) -> ApiResult<Vec<BookDto>> {
        let sort: BookSort = sort.into();
        Ok(map(self.engine.list_books(limit, sort).await?))
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

    pub async fn list_readings(&self, book_id: i64) -> ApiResult<Vec<ReadingDto>> {
        Ok(map(self.engine.list_readings(book_id).await?))
    }

    pub async fn get_reading(&self, id: i64) -> ApiResult<Option<ReadingDto>> {
        Ok(self.engine.get_reading(id).await?.map(Into::into))
    }

    pub async fn active_reading(&self, book_id: i64) -> ApiResult<Option<ReadingDto>> {
        Ok(self.engine.active_reading(book_id).await?.map(Into::into))
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

    // ---- notes -------------------------------------------------------------

    pub async fn create_note(&self, note: NewNoteDto) -> ApiResult<CreatedNoteDto> {
        Ok(self.engine.create_note(note.into()).await?.into())
    }

    pub async fn list_notes(&self, book_id: Option<i64>) -> ApiResult<Vec<NoteDto>> {
        Ok(map(self.engine.list_notes(book_id).await?))
    }

    pub async fn search_notes(&self, query: &str, limit: i64) -> ApiResult<Vec<NoteSearchHitDto>> {
        Ok(map(self.engine.search_notes(query, limit).await?))
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
    ) -> ApiResult<CalibreReportDto> {
        Ok(self
            .engine
            .import_calibre_library(&CalibreImportOptions {
                library,
                dry_run,
                create_ambiguous,
            })
            .await?
            .into())
    }

    // ---- flashcards --------------------------------------------------------

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
            R::ListBooks { limit, sort } => Response::Books(self.list_books(limit, sort).await?),
            R::GetBook { id } => Response::Book(self.get_book(id).await?),
            R::ResolveBooks { selector } => Response::Books(self.resolve_books(&selector).await?),
            R::BookTags { book_id } => Response::BookTags(self.book_tags(book_id).await?),
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
            R::SyncDevice { paths } => {
                let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
                Response::PullReports(self.sync_device(&paths).await?)
            }

            R::CreateNote { note } => Response::CreatedNote(self.create_note(note).await?),
            R::ListNotes { book_id } => Response::Notes(self.list_notes(book_id).await?),
            R::SearchNotes { query, limit } => {
                Response::NoteHits(self.search_notes(&query, limit).await?)
            }
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
            } => Response::CalibreReport(
                self.import_calibre_library(library.map(PathBuf::from), dry_run, create_ambiguous)
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
