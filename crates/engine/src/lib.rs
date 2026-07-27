//! readingbuddy — engine for a personal reading companion.
//!
//! The engine performs **no terminal I/O**: every user interaction lives in a
//! frontend (CLI today, TUI later). Frontends drive it through [`Engine`].

pub mod book;
pub mod config;
pub mod crash;
pub mod diagnostic;
pub mod epub;
pub mod error;
pub mod flashcards;
pub mod images;
pub mod koreader;
pub mod notes;
pub mod providers;
pub mod search;
pub mod storage;

use std::path::{Path, PathBuf};

use reqwest::Client;

pub use book::{Book, isbn10_to_13, normalize_isbn};
pub use config::EngineConfig;
pub use crash::CrashContext;
pub use diagnostic::{Diagnostic, DiagnosticKind, ErrorClass, Severity};
pub use error::{EngineError, Result};
pub use koreader::{ImportReport, MatchMethod};
pub use notes::{CreatedNote, NewNoteInput, NoteKind};
pub use providers::googlebooks::verify_key as verify_google_key;
pub use providers::{ProviderId, SearchRequest};
pub use search::{RankedResult, SearchOutcome};
pub use storage::{BookSort, FlashcardRow, Highlight, NoteRecord, NoteSearchHit, Storage};

use providers::googlebooks::GoogleBooksProvider;
use providers::openlibrary::OpenLibraryProvider;
use providers::{MetadataProvider, ProviderBook};

pub struct Engine {
    pub storage: Storage,
    pub config: EngineConfig,
    providers: Vec<Box<dyn MetadataProvider>>,
    client: Client,
}

impl Engine {
    pub async fn open(config: EngineConfig) -> Result<Engine> {
        std::fs::create_dir_all(&config.images_dir)?;
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
        Ok(Engine {
            storage,
            config,
            providers,
            client,
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

    /// Delete a book and its cover image file.
    pub async fn delete_book(&self, id: i64) -> Result<()> {
        if let Some(cover) = self.storage.delete_book(id).await? {
            std::fs::remove_file(cover).ok();
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
        self.save_book(&book).await
    }

    // ---- koreader ----------------------------------------------------------

    /// Import KOReader highlights/notes from a sidecar file, .sdr dir, or
    /// library root. Idempotent; single-word highlights become flashcard
    /// candidates.
    #[tracing::instrument(skip(self), fields(path = %path.display()))]
    pub async fn import_koreader(&self, path: &Path, dry_run: bool) -> Result<ImportReport> {
        koreader::import(&self.storage, path, dry_run).await
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

    /// The body text of a note (its markdown minus the frontmatter header).
    pub fn note_body(&self, note: &NoteRecord) -> Result<String> {
        let file = self.config.vault_dir.join(&note.file_path);
        let content = std::fs::read_to_string(&file)?;
        let (_, body) = notes::frontmatter_and_body(&content);
        Ok(body.trim_end().to_string())
    }

    /// Replace a note's body, preserving its frontmatter header, and reindex
    /// it in FTS. Used by the in-house editor.
    pub async fn update_note_body(&self, note: &NoteRecord, body: &str) -> Result<()> {
        let file = self.config.vault_dir.join(&note.file_path);
        let content = std::fs::read_to_string(&file)?;
        let (header, _) = notes::frontmatter_and_body(&content);
        std::fs::write(&file, format!("{header}{}\n", body.trim_end()))?;
        self.storage
            .refresh_note_body(note.id, &note.title, body)
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
