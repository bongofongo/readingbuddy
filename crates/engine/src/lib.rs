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
pub mod epub;
pub mod error;
pub mod files;
pub mod flashcards;
pub mod goodreads;
pub mod images;
pub mod koreader;
pub mod notes;
pub mod partial_md5;
pub mod providers;
pub mod search;
pub mod storage;
pub mod watch;

use std::path::{Path, PathBuf};

use reqwest::Client;

pub use book::{Book, isbn10_to_13, normalize_isbn};
pub use calibre::{
    Calibre, CalibreBook, CalibreBookReport, CalibreMatch, CalibreReport,
    ImportOptions as CalibreImportOptions, UnmatchedCalibreBook,
};
pub use config::EngineConfig;
pub use crash::CrashContext;
pub use device::{
    DeviceBook, DeviceScan, DeviceState, candidate_mounts, is_koreader_mount, koreader_dir,
    mount_roots, offers_reader,
};
pub use diagnostic::{Diagnostic, DiagnosticKind, ErrorClass, Severity};
pub use error::{EngineError, Result};
pub use files::{
    FileIdentity, FileImportReport, FileMatch, FileOutcome, ImportOptions as FileImportOptions,
};
pub use goodreads::{
    GoodreadsBookReport, GoodreadsMatch, GoodreadsReport, ImportOptions as GoodreadsImportOptions,
    TextOutcome, UnmatchedRow,
};
pub use koreader::{
    BookImportStats, ImportReport, KoStats, KoStatus, KoSummary, MatchCandidate, MatchMethod,
    PullReport,
};
pub use notes::{CreatedNote, NewNoteInput, NoteKind};
pub use partial_md5::partial_md5;
pub use providers::googlebooks::verify_key as verify_google_key;
pub use providers::{ProviderId, SearchRequest};
pub use search::{RankedResult, SearchOutcome};
pub use storage::{
    BookFile, BookSort, BookTag, FlashcardRow, Highlight, MergeReport, NewHighlight, NoteRecord,
    NoteSearchHit, OutgoingLink, Rating, RatingScale, Reading, Storage,
};
pub use watch::{MOUNT_QUIET, MountEvent, MountStir, MountWatcher, watch_mounts};

use providers::googlebooks::GoogleBooksProvider;
use providers::openlibrary::OpenLibraryProvider;
use providers::{MetadataProvider, ProviderBook};

pub struct Engine {
    pub storage: Storage,
    pub config: EngineConfig,
    providers: Vec<Box<dyn MetadataProvider>>,
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
        let providers: Vec<Box<dyn MetadataProvider>> = vec![
            Box::new(OpenLibraryProvider::new(client.clone())),
            Box::new(GoogleBooksProvider::new(
                client.clone(),
                config.google_api_key.clone(),
            )),
        ];
        // Once, here — not once per book on a library import, which would be a
        // PATH sweep per book for an answer that cannot change mid-run.
        let calibre = Calibre::detect(config.calibre_bin_dir.as_deref());
        Ok(Engine {
            storage,
            config,
            providers,
            client,
            calibre,
        })
    }

    /// Swap in a new Google Books API key (or clear it) and rebuild the
    /// provider list so the change is live for the next search — frontends set
    /// this when the user enters a key at runtime.
    pub fn set_google_api_key(&mut self, key: Option<String>) {
        self.config.google_api_key = key.clone();
        self.providers = vec![
            Box::new(OpenLibraryProvider::new(self.client.clone())),
            Box::new(GoogleBooksProvider::new(self.client.clone(), key)),
        ];
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
        search::federated_search(&self.providers, &req).await
    }

    /// Direct edition lookup by ISBN, merging fields across providers.
    #[tracing::instrument(skip(self))]
    pub async fn lookup_isbn(&self, raw: &str) -> Result<Option<Book>> {
        let isbn = normalize_isbn(raw).ok_or_else(|| EngineError::InvalidIsbn(raw.to_string()))?;
        let mut found: Vec<ProviderBook> = Vec::new();
        for p in &self.providers {
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
    pub async fn save_book(&self, book: &Book) -> Result<Book> {
        let id = self.storage.upsert_book(book).await?;
        self.storage
            .get_book(id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {id}")))
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
    pub async fn delete_book(&self, id: i64) -> Result<()> {
        let files = self.storage.book_files(id).await?;
        if let Some(cover) = self.storage.delete_book(id).await? {
            std::fs::remove_file(cover).ok();
        }
        for file in &files {
            std::fs::remove_file(self.file_path(file)).ok();
        }
        Ok(())
    }

    /// Download `cover_url` into the images dir and persist `cover_path`.
    pub async fn download_cover(&self, book: &mut Book) -> Result<Option<PathBuf>> {
        let Some(url) = book.cover_url.clone() else {
            return Ok(None);
        };
        let path = images::image_from_url(&self.client, &url, &self.config.images_dir).await?;
        book.cover_path = Some(path.display().to_string());
        if book.id.is_some() || book.isbn_10.is_some() || book.isbn_13.is_some() {
            self.storage.upsert_book(book).await?;
        }
        Ok(Some(path))
    }

    // ---- epub import -------------------------------------------------------

    /// Import a local .epub: extract its ISBN, enrich via providers, extract
    /// the embedded cover, save. Falls back to epub metadata alone when the
    /// file has no usable ISBN or the providers are unreachable.
    #[tracing::instrument(skip(self), fields(path = %path.display()))]
    pub async fn import_epub(&self, path: &Path) -> Result<Book> {
        let info = epub::epub_info(path)?;
        let mut book = match &info.isbn {
            Some(isbn) => self.lookup_isbn(isbn).await?.unwrap_or_default(),
            None => Book::default(),
        };
        if book.title.is_none() {
            book.title = info.title.clone();
        }
        if book.authors.is_empty() {
            book.authors = info.authors.clone();
        }
        if book.language.is_none() {
            book.language = info.language.clone();
        }
        if book.isbn_10.is_none()
            && book.isbn_13.is_none()
            && let Some(isbn) = &info.isbn
        {
            match isbn.len() {
                10 => book.isbn_10 = Some(isbn.clone()),
                _ => book.isbn_13 = Some(isbn.clone()),
            }
        }
        if let Some(cover) = epub::extract_cover(path, &self.config.images_dir)? {
            book.cover_path = Some(cover.display().to_string());
        }
        let saved = self.save_book(&book).await?;

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
    pub async fn sync_device(&self, paths: &[PathBuf]) -> Result<Vec<PullReport>> {
        device::sync_device(&self.storage, paths).await
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
            // Same resolution as `delete_book`: the stored path as written.
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

    pub async fn list_notes(&self, book_id: Option<i64>) -> Result<Vec<NoteRecord>> {
        self.storage.list_notes(book_id).await
    }

    pub async fn search_notes(&self, query: &str, limit: i64) -> Result<Vec<NoteSearchHit>> {
        self.storage.search_notes(query, limit).await
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
        let file = self.config.vault_dir.join(&note.file_path);
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
        let file = self.config.vault_dir.join(&note.file_path);
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
        let file = self.config.vault_dir.join(&note.file_path);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.storage.delete_note(note.id).await
    }

    /// Re-read a note file from disk and refresh its FTS body (e.g. after an
    /// external Obsidian edit).
    pub async fn refresh_note_from_disk(&self, note: &NoteRecord) -> Result<()> {
        let file = self.config.vault_dir.join(&note.file_path);
        let content = std::fs::read_to_string(&file)?;
        let (_, body) = notes::parse_frontmatter(&content);
        self.storage
            .refresh_note_body(note.id, &note.title, body)
            .await?;
        // An outside edit is exactly where a new [[wikilink]] appears, so the
        // graph has to follow the file here too.
        let links = notes::extract_wikilinks(body);
        self.storage
            .set_note_links(note.id, &note.title, &links)
            .await
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

    /// Cite a highlight from a note — by reference, so the citation stays live
    /// when a device refresh rewrites the highlight's device-owned fields.
    pub async fn cite(&self, note_id: i64, highlight_id: i64) -> Result<()> {
        self.storage.add_citation(note_id, highlight_id).await?;
        Ok(())
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
