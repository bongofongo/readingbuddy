//! The wire types.
//!
//! Every one of these is the serializable mirror of a domain type, with a
//! `From` in the direction the data actually flows. **The domain types stay
//! free of `serde`, and that is the decision this module exists to keep.**
//!
//! Three reasons, and none of them is taste:
//!
//! - `Book` carries `OffsetDateTime`, `FileIdentity` and half the reports carry
//!   `PathBuf`, and `Diagnostic` carries a `Duration`. Deriving `Serialize` on
//!   those picks a wire encoding for each **by accident** — whatever the
//!   dependency happens to do this year — and then that accident is the API.
//! - A `#[derive(Serialize)]` on a domain struct makes every field name a
//!   public promise. Renaming `ko_percent` would then be a breaking API change
//!   rather than a refactor.
//! - The engine would gain `serde` on its hot path for the benefit of a caller
//!   it cannot see.
//!
//! ## Paths and JSON
//!
//! A `PathBuf` is bytes on unix and JSON is UTF-8, so a path crosses this seam
//! as `String` via `to_string_lossy`. **A path that is not valid UTF-8 does not
//! round-trip** — it comes back with replacement characters and names a
//! different file. That is a real limit and not a hidden one: there is no
//! lossless JSON encoding of an arbitrary unix path short of base64, which
//! would make every path in the protocol unreadable to a human debugging it.
//! The trade is made deliberately in favour of the ordinary case.
//!
//! ## Optional fields
//!
//! Everything nullable or repeated carries `#[serde(default)]`, so a payload
//! written by an older client still deserializes when a field is added. Nothing
//! carries `skip_serializing_if`: an explicit `null` is a stable shape, and a
//! field that vanishes when empty is a field every client has to special-case.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use readingbuddy::GoodreadsMatch;
use readingbuddy::koreader::UnmatchedSidecar;
use readingbuddy::providers::ProviderId;
use readingbuddy::{
    Book, BookFile, BookImportStats, BookSort, BookTag, CalibreBook, CalibreBookReport,
    CalibreMatch, CalibreReport, CreatedNote, DeviceBook, DeviceScan, DeviceState, Diagnostic,
    DiagnosticKind, ErrorClass, FileIdentity, FileImportReport, FileMatch, FileOutcome,
    FlashcardRow, GoodreadsBookReport, GoodreadsReport, Highlight, ImportReport, KoStatus,
    MatchCandidate, MatchMethod, MergeReport, NewNoteInput, NoteKind, NoteRecord, NoteSearchHit,
    OutgoingLink, PullReport, RankedResult, Rating, RatingScale, Reading, SearchOutcome,
    SearchRequest, Severity, TextOutcome, UnmatchedRow,
};

/// A path, as far as JSON can carry one. See the module doc.
fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn opt_path(p: &Option<PathBuf>) -> Option<String> {
    p.as_deref().map(path_str)
}

// ---- books ----------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BookDto {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub sort_title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub translators: Vec<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub publish_year: Option<i64>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub isbn_10: Option<String>,
    #[serde(default)]
    pub isbn_13: Option<String>,
    #[serde(default)]
    pub openlibrary_key: Option<String>,
    #[serde(default)]
    pub googlebooks_id: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub cover_path: Option<String>,
    #[serde(default)]
    pub page_count: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub first_sentence: Option<String>,
    /// Read-only projections of the **current** reading. Sending them back in a
    /// `save_book` changes nothing: `upsert_book` has ignored these four since
    /// migration `0005`, and `update_progress` is the writer.
    #[serde(default)]
    pub current_page: Option<i64>,
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub date_started: Option<i64>,
    #[serde(default)]
    pub date_finished: Option<i64>,
    /// Unix seconds. `OffsetDateTime`'s own serde format is a dependency's
    /// choice; an integer is ours.
    #[serde(default)]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub last_modified: Option<i64>,
}

impl From<Book> for BookDto {
    fn from(b: Book) -> Self {
        BookDto {
            id: b.id,
            title: b.title,
            sort_title: b.sort_title,
            authors: b.authors,
            translators: b.translators,
            publisher: b.publisher,
            publish_year: b.publish_year,
            language: b.language,
            isbn_10: b.isbn_10,
            isbn_13: b.isbn_13,
            openlibrary_key: b.openlibrary_key,
            googlebooks_id: b.googlebooks_id,
            cover_url: b.cover_url,
            cover_path: b.cover_path,
            page_count: b.page_count,
            description: b.description,
            first_sentence: b.first_sentence,
            current_page: b.current_page,
            finished: b.finished,
            date_started: b.date_started,
            date_finished: b.date_finished,
            created_at: b.created_at.map(|t| t.unix_timestamp()),
            last_modified: b.last_modified.map(|t| t.unix_timestamp()),
        }
    }
}

impl From<BookDto> for Book {
    /// The way back, for `save_book`.
    ///
    /// `created_at`/`last_modified` are **dropped, not parsed back**: they are
    /// the storage layer's to stamp, and a client that could set them could
    /// backdate a row by mistake. The four reading projections are carried
    /// through because the struct has the fields, and are ignored downstream by
    /// `upsert_book` — which is where that rule already lives, and where it
    /// should stay rather than being re-implemented here.
    fn from(d: BookDto) -> Self {
        Book {
            id: d.id,
            title: d.title,
            sort_title: d.sort_title,
            authors: d.authors,
            translators: d.translators,
            publisher: d.publisher,
            publish_year: d.publish_year,
            language: d.language,
            isbn_10: d.isbn_10,
            isbn_13: d.isbn_13,
            openlibrary_key: d.openlibrary_key,
            googlebooks_id: d.googlebooks_id,
            cover_url: d.cover_url,
            cover_path: d.cover_path,
            page_count: d.page_count,
            description: d.description,
            first_sentence: d.first_sentence,
            current_page: d.current_page,
            finished: d.finished,
            date_started: d.date_started,
            date_finished: d.date_finished,
            created_at: None,
            last_modified: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookSortDto {
    #[default]
    LastModified,
    Title,
    Progress,
}

impl From<BookSortDto> for BookSort {
    fn from(s: BookSortDto) -> Self {
        match s {
            BookSortDto::LastModified => BookSort::LastModified,
            BookSortDto::Title => BookSort::Title,
            BookSortDto::Progress => BookSort::Progress,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookTagDto {
    pub tag: String,
    pub source: String,
    /// The origin's own string, before our normalization. Kept because the
    /// normalization is ours and the shelf name is theirs — and collections are
    /// deliberately still deferred, so this is the raw material that design will
    /// eventually be made against.
    #[serde(default)]
    pub raw: Option<String>,
}

impl From<BookTag> for BookTagDto {
    fn from(t: BookTag) -> Self {
        BookTagDto {
            tag: t.tag,
            source: t.source,
            raw: t.raw,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeReportDto {
    pub src_existed: bool,
    pub highlights_moved: usize,
    pub highlights_dropped: usize,
    pub notes_moved: usize,
    pub readings_moved: usize,
    pub flashcards_moved: usize,
    pub flashcards_dropped: usize,
    pub device_links_moved: usize,
    pub files_moved: usize,
    #[serde(default)]
    pub orphaned_cover: Option<String>,
}

impl From<MergeReport> for MergeReportDto {
    fn from(r: MergeReport) -> Self {
        MergeReportDto {
            src_existed: r.src_existed,
            highlights_moved: r.highlights_moved,
            highlights_dropped: r.highlights_dropped,
            notes_moved: r.notes_moved,
            readings_moved: r.readings_moved,
            flashcards_moved: r.flashcards_moved,
            flashcards_dropped: r.flashcards_dropped,
            device_links_moved: r.device_links_moved,
            files_moved: r.files_moved,
            orphaned_cover: r.orphaned_cover,
        }
    }
}

// ---- readings and highlights ----------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingDto {
    pub id: i64,
    pub book_id: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    /// `null` means open, and at most one reading per book may be.
    #[serde(default)]
    pub finished_at: Option<i64>,
    pub status: String,
    pub source: String,
    #[serde(default)]
    pub current_page: Option<i64>,
    #[serde(default)]
    pub ko_status: Option<String>,
    #[serde(default)]
    pub ko_percent: Option<f64>,
    #[serde(default)]
    pub ko_rating: Option<i64>,
    pub created_at: i64,
    pub last_modified: i64,
}

impl From<Reading> for ReadingDto {
    fn from(r: Reading) -> Self {
        ReadingDto {
            id: r.id,
            book_id: r.book_id,
            started_at: r.started_at,
            finished_at: r.finished_at,
            status: r.status,
            source: r.source,
            current_page: r.current_page,
            ko_status: r.ko_status,
            ko_percent: r.ko_percent,
            ko_rating: r.ko_rating,
            created_at: r.created_at,
            last_modified: r.last_modified,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightDto {
    pub id: i64,
    pub book_id: i64,
    pub text: String,
    #[serde(default)]
    pub chapter: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    /// KOReader's note — theirs, and rewritten toward the device on every
    /// import.
    #[serde(default)]
    pub ko_note: Option<String>,
    /// The reader's own annotation — ours, and never touched by an import.
    #[serde(default)]
    pub annotation: Option<String>,
    #[serde(default)]
    pub ko_datetime: Option<String>,
    pub source: String,
    pub created_at: i64,
}

impl From<Highlight> for HighlightDto {
    fn from(h: Highlight) -> Self {
        HighlightDto {
            id: h.id,
            book_id: h.book_id,
            text: h.text,
            chapter: h.chapter,
            page: h.page,
            ko_note: h.ko_note,
            annotation: h.annotation,
            ko_datetime: h.ko_datetime,
            source: h.source,
            created_at: h.created_at,
        }
    }
}

// ---- notes ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKindDto {
    #[default]
    Note,
    Session,
    Reflection,
    Review,
}

impl From<NoteKindDto> for NoteKind {
    fn from(k: NoteKindDto) -> Self {
        match k {
            NoteKindDto::Note => NoteKind::Note,
            NoteKindDto::Session => NoteKind::Session,
            NoteKindDto::Reflection => NoteKind::Reflection,
            NoteKindDto::Review => NoteKind::Review,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteDto {
    pub id: i64,
    #[serde(default)]
    pub book_id: Option<i64>,
    #[serde(default)]
    pub reading_id: Option<i64>,
    #[serde(default)]
    pub highlight_id: Option<i64>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub location: Option<String>,
    /// Relative to the vault root. The absolute path is `note_path`, which is
    /// the engine's to derive — a client joining it itself would need the vault
    /// root, and would then be holding a second copy of a fact that can move.
    pub file_path: String,
    pub title: String,
    /// `note` | `session` | `reflection` | `review`.
    pub kind: String,
    #[serde(default)]
    pub created_at: Option<i64>,
}

impl From<NoteRecord> for NoteDto {
    fn from(n: NoteRecord) -> Self {
        NoteDto {
            id: n.id,
            book_id: n.book_id,
            reading_id: n.reading_id,
            highlight_id: n.highlight_id,
            page: n.page,
            location: n.location,
            file_path: n.file_path,
            title: n.title,
            kind: n.kind,
            created_at: n.created_at.map(|t| t.unix_timestamp()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewNoteDto {
    #[serde(default)]
    pub book_id: Option<i64>,
    #[serde(default)]
    pub reading_id: Option<i64>,
    #[serde(default)]
    pub highlight_id: Option<i64>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub kind: NoteKindDto,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: String,
}

impl From<NewNoteDto> for NewNoteInput {
    fn from(d: NewNoteDto) -> Self {
        NewNoteInput {
            book_id: d.book_id,
            reading_id: d.reading_id,
            highlight_id: d.highlight_id,
            page: d.page,
            location: d.location,
            kind: d.kind.into(),
            title: d.title,
            body: d.body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedNoteDto {
    pub id: i64,
    pub title: String,
    /// Absolute path of the markdown file.
    pub file: String,
    pub links: Vec<String>,
}

impl From<CreatedNote> for CreatedNoteDto {
    fn from(n: CreatedNote) -> Self {
        CreatedNoteDto {
            id: n.id,
            title: n.title,
            file: path_str(&n.file),
            links: n.links,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSearchHitDto {
    pub note: NoteDto,
    pub snippet: String,
}

impl From<NoteSearchHit> for NoteSearchHitDto {
    fn from(h: NoteSearchHit) -> Self {
        NoteSearchHitDto {
            note: h.note.into(),
            snippet: h.snippet,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingLinkDto {
    /// The `[[wikilink]]` as written.
    pub target_title: String,
    /// The note it resolves to, when one exists. `null` is a **forward
    /// reference**, not an error: it resolves itself the moment that note is
    /// written, and a client shows it as text rather than dropping it.
    #[serde(default)]
    pub note: Option<NoteDto>,
}

impl From<OutgoingLink> for OutgoingLinkDto {
    fn from(l: OutgoingLink) -> Self {
        OutgoingLinkDto {
            target_title: l.target_title,
            note: l.to.map(Into::into),
        }
    }
}

/// One row of the currently-reading shelf.
///
/// The [`ReadingDto`] rides beside the [`BookDto`] rather than being folded
/// into it, because the two are not interchangeable: a book's progress fields
/// are projections of the *current* reading, which for a finished book is a
/// closed one, while this row is specifically the open one and carries its own
/// status, source and device mirror. A sidecar-seeded book has `ko_percent` and
/// no `current_page` at all, so the book alone would render blank.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenReadingDto {
    pub book: BookDto,
    pub reading: ReadingDto,
}

impl From<(Book, Reading)> for OpenReadingDto {
    fn from((book, reading): (Book, Reading)) -> Self {
        OpenReadingDto {
            book: book.into(),
            reading: reading.into(),
        }
    }
}

// ---- ratings ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingScaleDto {
    pub id: i64,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl From<RatingScale> for RatingScaleDto {
    fn from(s: RatingScale) -> Self {
        RatingScaleDto {
            id: s.id,
            name: s.name,
            min: s.min,
            max: s.max,
            step: s.step,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingDto {
    /// The scale travels **with** the value, never without it: the Goodreads
    /// map is user-editable, so a bare number is not re-derivable into anything.
    pub scale: RatingScaleDto,
    pub value: f64,
}

impl From<Rating> for RatingDto {
    fn from(r: Rating) -> Self {
        RatingDto {
            scale: r.scale.into(),
            value: r.value,
        }
    }
}

/// One entry of the Goodreads lookup table.
///
/// A named pair rather than a JSON array, because a two-element array is
/// positional and this table is **explicit by design** — the whole reason it is
/// a lookup and not a formula is that the ends must be readable at a glance.
/// A scale point with no entry simply is not in this list, which is what a
/// review on that value will report instead of a number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingMapEntryDto {
    pub value: f64,
    pub goodreads: u8,
}

// ---- files and flashcards --------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookFileDto {
    pub sha256: String,
    pub book_id: i64,
    pub format: String,
    #[serde(default)]
    pub original_name: Option<String>,
    pub size: i64,
    pub added_at: i64,
}

impl From<BookFile> for BookFileDto {
    fn from(f: BookFile) -> Self {
        BookFileDto {
            sha256: f.sha256,
            book_id: f.book_id,
            format: f.format,
            original_name: f.original_name,
            size: f.size,
            added_at: f.added_at,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMatchDto {
    Sha256,
    Isbn,
    Md5,
    Title,
}

impl From<FileMatch> for FileMatchDto {
    fn from(m: FileMatch) -> Self {
        match m {
            FileMatch::Sha256 => FileMatchDto::Sha256,
            FileMatch::Isbn => FileMatchDto::Isbn,
            FileMatch::Md5 => FileMatchDto::Md5,
            FileMatch::Title => FileMatchDto::Title,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOutcomeDto {
    Stored,
    AlreadyOwned,
    Unmatched,
}

impl From<FileOutcome> for FileOutcomeDto {
    fn from(o: FileOutcome) -> Self {
        match o {
            FileOutcome::Stored => FileOutcomeDto::Stored,
            FileOutcome::AlreadyOwned => FileOutcomeDto::AlreadyOwned,
            FileOutcome::Unmatched => FileOutcomeDto::Unmatched,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileIdentityDto {
    pub path: String,
    pub sha256: String,
    pub partial_md5: String,
    pub format: String,
    pub size: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub matched_book_id: Option<i64>,
    #[serde(default)]
    pub matched_by: Option<FileMatchDto>,
    #[serde(default)]
    pub candidates: Vec<MatchCandidateDto>,
}

impl From<FileIdentity> for FileIdentityDto {
    /// The domain's `matched: Option<(i64, FileMatch)>` becomes two nullable
    /// fields. A JSON tuple is positional, so adding a third element later
    /// would silently change what index 1 means; two named fields cannot.
    fn from(i: FileIdentity) -> Self {
        FileIdentityDto {
            path: path_str(&i.path),
            sha256: i.sha256,
            partial_md5: i.partial_md5,
            format: i.format,
            size: i.size,
            title: i.title,
            matched_book_id: i.matched.map(|(id, _)| id),
            matched_by: i.matched.map(|(_, m)| m.into()),
            candidates: i.candidates.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileImportReportDto {
    pub outcome: FileOutcomeDto,
    #[serde(default)]
    pub book_id: Option<i64>,
    #[serde(default)]
    pub matched_by: Option<FileMatchDto>,
    pub created_book: bool,
    pub sha256: String,
    #[serde(default)]
    pub stored_path: Option<String>,
    #[serde(default)]
    pub candidates: Vec<MatchCandidateDto>,
}

impl From<FileImportReport> for FileImportReportDto {
    fn from(r: FileImportReport) -> Self {
        FileImportReportDto {
            outcome: r.outcome.into(),
            book_id: r.book_id,
            matched_by: r.matched_by.map(Into::into),
            created_book: r.created_book,
            sha256: r.sha256,
            stored_path: opt_path(&r.stored_path),
            candidates: r.candidates.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashcardDto {
    pub id: i64,
    pub word: String,
    #[serde(default)]
    pub context: Option<String>,
    pub book_title: String,
    pub exported: bool,
}

impl From<FlashcardRow> for FlashcardDto {
    fn from(c: FlashcardRow) -> Self {
        FlashcardDto {
            id: c.id,
            word: c.word,
            context: c.context,
            book_title: c.book_title,
            exported: c.exported,
        }
    }
}

// ---- search ----------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequestDto {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub translator: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub isbn: Option<String>,
    #[serde(default)]
    pub limit: u32,
}

impl From<SearchRequestDto> for SearchRequest {
    fn from(d: SearchRequestDto) -> Self {
        SearchRequest {
            query: d.query,
            title: d.title,
            author: d.author,
            publisher: d.publisher,
            translator: d.translator,
            language: d.language,
            year: d.year,
            isbn: d.isbn,
            limit: d.limit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderIdDto {
    OpenLibrary,
    GoogleBooks,
}

impl From<ProviderId> for ProviderIdDto {
    fn from(p: ProviderId) -> Self {
        match p {
            ProviderId::OpenLibrary => ProviderIdDto::OpenLibrary,
            ProviderId::GoogleBooks => ProviderIdDto::GoogleBooks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedResultDto {
    pub book: BookDto,
    pub sources: Vec<ProviderIdDto>,
    pub score: f64,
}

impl From<RankedResult> for RankedResultDto {
    fn from(r: RankedResult) -> Self {
        RankedResultDto {
            book: r.book.into(),
            sources: r.sources.into_iter().map(Into::into).collect(),
            score: r.score,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchOutcomeDto {
    pub results: Vec<RankedResultDto>,
    /// A dead provider degrades the search, never kills it — so the warnings
    /// ride back **with** the results rather than replacing them.
    pub warnings: Vec<DiagnosticDto>,
}

impl From<SearchOutcome> for SearchOutcomeDto {
    fn from(o: SearchOutcome) -> Self {
        SearchOutcomeDto {
            results: o.results.into_iter().map(Into::into).collect(),
            warnings: o.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

// ---- diagnostics -----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityDto {
    Warning,
    Error,
}

impl From<Severity> for SeverityDto {
    fn from(s: Severity) -> Self {
        match s {
            Severity::Warning => SeverityDto::Warning,
            Severity::Error => SeverityDto::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClassDto {
    Network,
    Timeout,
    RateLimited,
    Decode,
    Parse,
    Io,
    #[serde(other)]
    Other,
}

impl From<ErrorClass> for ErrorClassDto {
    fn from(c: ErrorClass) -> Self {
        match c {
            ErrorClass::Network => ErrorClassDto::Network,
            ErrorClass::Timeout => ErrorClassDto::Timeout,
            ErrorClass::RateLimited => ErrorClassDto::RateLimited,
            ErrorClass::Decode => ErrorClassDto::Decode,
            ErrorClass::Parse => ErrorClassDto::Parse,
            ErrorClass::Io => ErrorClassDto::Io,
            ErrorClass::Other => ErrorClassDto::Other,
        }
    }
}

/// The full mirror of `DiagnosticKind`, and it is full on purpose.
///
/// The cheap version of this type is `{ kind: String, detail: String }`, and it
/// would have thrown away the whole reason `Diagnostic` stopped being a
/// `String` in the first place: a caller has to be able to tell a timeout from
/// a 500, and *which file* was unparsable, without scraping prose. Every
/// variant here has a different next move on the far side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiagnosticKindDto {
    ProviderFailed {
        provider: ProviderIdDto,
        class: ErrorClassDto,
    },
    ProviderTimedOut {
        provider: ProviderIdDto,
        /// Seconds. A `Duration`'s serde shape is a dependency's choice.
        after_secs: u64,
    },
    SidecarUnreadable {
        path: String,
        class: ErrorClassDto,
    },
    SidecarUnparsable {
        path: String,
    },
    NoSidecarsFound {
        path: String,
    },
    UnknownDeviceStatus {
        path: String,
        status: String,
    },
    SidecarNotIdentified {
        path: String,
    },
    GoodreadsRowSkipped {
        row: usize,
    },
    GoodreadsReviewDiverged {
        title: String,
    },
    GoodreadsRatingDiverged {
        title: String,
    },
    GoodreadsUnanchoredReview {
        title: String,
    },
    GoodreadsUndatableRereads {
        title: String,
        dropped: usize,
    },
    GoodreadsRatingUnmapped {
        title: String,
    },
    GoodreadsRereadsDropped {
        title: String,
        dropped: usize,
    },
    CalibreRowSkipped {
        calibre_id: i64,
    },
    CalibreRowNotIdentified {
        calibre_id: i64,
    },
    CalibreCoverUnreadable {
        path: String,
    },
}

impl From<DiagnosticKind> for DiagnosticKindDto {
    fn from(k: DiagnosticKind) -> Self {
        use DiagnosticKind as K;
        match k {
            K::ProviderFailed { provider, class } => DiagnosticKindDto::ProviderFailed {
                provider: provider.into(),
                class: class.into(),
            },
            K::ProviderTimedOut { provider, after } => DiagnosticKindDto::ProviderTimedOut {
                provider: provider.into(),
                after_secs: after.as_secs(),
            },
            K::SidecarUnreadable { path, class } => DiagnosticKindDto::SidecarUnreadable {
                path: path_str(&path),
                class: class.into(),
            },
            K::SidecarUnparsable { path } => DiagnosticKindDto::SidecarUnparsable {
                path: path_str(&path),
            },
            K::NoSidecarsFound { path } => DiagnosticKindDto::NoSidecarsFound {
                path: path_str(&path),
            },
            K::UnknownDeviceStatus { path, status } => DiagnosticKindDto::UnknownDeviceStatus {
                path: path_str(&path),
                status,
            },
            K::SidecarNotIdentified { path } => DiagnosticKindDto::SidecarNotIdentified {
                path: path_str(&path),
            },
            K::GoodreadsRowSkipped { row } => DiagnosticKindDto::GoodreadsRowSkipped { row },
            K::GoodreadsReviewDiverged { title } => {
                DiagnosticKindDto::GoodreadsReviewDiverged { title }
            }
            K::GoodreadsRatingDiverged { title } => {
                DiagnosticKindDto::GoodreadsRatingDiverged { title }
            }
            K::GoodreadsUnanchoredReview { title } => {
                DiagnosticKindDto::GoodreadsUnanchoredReview { title }
            }
            K::GoodreadsUndatableRereads { title, dropped } => {
                DiagnosticKindDto::GoodreadsUndatableRereads { title, dropped }
            }
            K::GoodreadsRatingUnmapped { title } => {
                DiagnosticKindDto::GoodreadsRatingUnmapped { title }
            }
            K::GoodreadsRereadsDropped { title, dropped } => {
                DiagnosticKindDto::GoodreadsRereadsDropped { title, dropped }
            }
            K::CalibreRowSkipped { calibre_id } => {
                DiagnosticKindDto::CalibreRowSkipped { calibre_id }
            }
            K::CalibreRowNotIdentified { calibre_id } => {
                DiagnosticKindDto::CalibreRowNotIdentified { calibre_id }
            }
            K::CalibreCoverUnreadable { path } => DiagnosticKindDto::CalibreCoverUnreadable {
                path: path_str(&path),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticDto {
    #[serde(flatten)]
    pub kind: DiagnosticKindDto,
    pub severity: SeverityDto,
    pub detail: String,
    /// `Diagnostic`'s own `Display`, which is byte-for-byte what the CLI
    /// prints. Carried rather than left to each client to re-derive: the
    /// formatting rule lives in the engine, and three clients re-implementing it
    /// is three chances to disagree with the CLI about the same warning.
    pub display: String,
}

impl From<Diagnostic> for DiagnosticDto {
    fn from(d: Diagnostic) -> Self {
        DiagnosticDto {
            display: d.to_string(),
            kind: d.kind.into(),
            severity: d.severity.into(),
            detail: d.detail,
        }
    }
}

// ---- koreader and the device ----------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCandidateDto {
    pub book_id: i64,
    pub title: String,
    pub score: f64,
}

impl From<MatchCandidate> for MatchCandidateDto {
    fn from(c: MatchCandidate) -> Self {
        MatchCandidateDto {
            book_id: c.book_id,
            title: c.title,
            score: c.score,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMethodDto {
    Md5,
    Isbn,
    Title,
    New,
}

impl From<MatchMethod> for MatchMethodDto {
    fn from(m: MatchMethod) -> Self {
        match m {
            MatchMethod::Md5 => MatchMethodDto::Md5,
            MatchMethod::Isbn => MatchMethodDto::Isbn,
            MatchMethod::Title => MatchMethodDto::Title,
            MatchMethod::New => MatchMethodDto::New,
        }
    }
}

/// The device's own status. `Other` keeps the raw string rather than collapsing
/// to a known one — a status KOReader grew and we do not model is exactly the
/// thing worth reporting, and guessing at it would be silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum KoStatusDto {
    Reading,
    Abandoned,
    Complete,
    Other { raw: String },
}

impl From<KoStatus> for KoStatusDto {
    fn from(s: KoStatus) -> Self {
        match s {
            KoStatus::Reading => KoStatusDto::Reading,
            KoStatus::Abandoned => KoStatusDto::Abandoned,
            KoStatus::Complete => KoStatusDto::Complete,
            KoStatus::Other(raw) => KoStatusDto::Other { raw },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookImportStatsDto {
    pub book_id: i64,
    pub book_title: String,
    pub inserted: usize,
    /// Rows whose **device-owned** fields the sidecar disagreed with. Never
    /// includes `annotation`, which is ours.
    pub updated: usize,
    /// Present and identical.
    pub skipped: usize,
    pub flashcards: usize,
    pub matched_by: MatchMethodDto,
    #[serde(default)]
    pub percent_finished: Option<f64>,
    #[serde(default)]
    pub status: Option<KoStatusDto>,
    #[serde(default)]
    pub rating: Option<i64>,
}

impl From<BookImportStats> for BookImportStatsDto {
    fn from(s: BookImportStats) -> Self {
        BookImportStatsDto {
            book_id: s.book_id,
            book_title: s.book_title,
            inserted: s.inserted,
            updated: s.updated,
            skipped: s.skipped,
            flashcards: s.flashcards,
            matched_by: s.matched_by.into(),
            percent_finished: s.percent_finished,
            status: s.status.map(Into::into),
            rating: s.rating,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmatchedSidecarDto {
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub partial_md5: Option<String>,
    #[serde(default)]
    pub candidates: Vec<MatchCandidateDto>,
}

impl From<UnmatchedSidecar> for UnmatchedSidecarDto {
    fn from(u: UnmatchedSidecar) -> Self {
        UnmatchedSidecarDto {
            path: path_str(&u.path),
            title: u.title,
            partial_md5: u.partial_md5,
            candidates: u.candidates.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportReportDto {
    pub imported: Vec<BookImportStatsDto>,
    pub unmatched: Vec<UnmatchedSidecarDto>,
    pub warnings: Vec<DiagnosticDto>,
}

impl From<ImportReport> for ImportReportDto {
    fn from(r: ImportReport) -> Self {
        ImportReportDto {
            imported: r.imported.into_iter().map(Into::into).collect(),
            unmatched: r.unmatched.into_iter().map(Into::into).collect(),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullReportDto {
    pub stats: BookImportStatsDto,
    pub warnings: Vec<DiagnosticDto>,
}

impl From<PullReport> for PullReportDto {
    fn from(r: PullReport) -> Self {
        PullReportDto {
            stats: r.stats.into(),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

/// The four states `docs/decisions.md` names, and no fifth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeviceStateDto {
    New {
        candidates: Vec<MatchCandidateDto>,
    },
    Unchanged,
    Updated {
        new_highlights: usize,
        refreshed: usize,
    },
    Unreadable {
        diagnostic: DiagnosticDto,
    },
}

impl From<DeviceState> for DeviceStateDto {
    fn from(s: DeviceState) -> Self {
        match s {
            DeviceState::New { candidates } => DeviceStateDto::New {
                candidates: candidates.into_iter().map(Into::into).collect(),
            },
            DeviceState::Unchanged => DeviceStateDto::Unchanged,
            DeviceState::Updated {
                new_highlights,
                refreshed,
            } => DeviceStateDto::Updated {
                new_highlights,
                refreshed,
            },
            DeviceState::Unreadable(d) => DeviceStateDto::Unreadable {
                diagnostic: d.into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBookDto {
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Option<String>,
    #[serde(default)]
    pub partial_md5: Option<String>,
    #[serde(default)]
    pub book_id: Option<i64>,
    #[serde(default)]
    pub matched_by: Option<MatchMethodDto>,
    pub state: DeviceStateDto,
    #[serde(default)]
    pub ko_percent: Option<f64>,
    #[serde(default)]
    pub ko_status: Option<KoStatusDto>,
}

impl From<DeviceBook> for DeviceBookDto {
    fn from(b: DeviceBook) -> Self {
        DeviceBookDto {
            path: path_str(&b.path),
            title: b.title,
            authors: b.authors,
            partial_md5: b.partial_md5,
            book_id: b.book_id,
            matched_by: b.matched_by.map(Into::into),
            state: b.state.into(),
            ko_percent: b.ko_percent,
            ko_status: b.ko_status.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceScanDto {
    pub root: String,
    pub books: Vec<DeviceBookDto>,
    pub warnings: Vec<DiagnosticDto>,
    /// Sidecars this scan actually evaluated, and sidecars answered from the
    /// `sidecar_seen` cache. Reported rather than logged because the
    /// pre-filter's whole claim is that the first is zero on a second scan of an
    /// unmodified tree, and a claim only a stopwatch can check is not one.
    pub parsed: usize,
    pub cached: usize,
}

impl From<DeviceScan> for DeviceScanDto {
    fn from(s: DeviceScan) -> Self {
        DeviceScanDto {
            root: path_str(&s.root),
            books: s.books.into_iter().map(Into::into).collect(),
            warnings: s.warnings.into_iter().map(Into::into).collect(),
            parsed: s.parsed,
            cached: s.cached,
        }
    }
}

// ---- goodreads -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodreadsMatchDto {
    ExternalId,
    Isbn,
    Title,
    New,
}

impl From<GoodreadsMatch> for GoodreadsMatchDto {
    fn from(m: GoodreadsMatch) -> Self {
        match m {
            GoodreadsMatch::ExternalId => GoodreadsMatchDto::ExternalId,
            GoodreadsMatch::Isbn => GoodreadsMatchDto::Isbn,
            GoodreadsMatch::Title => GoodreadsMatchDto::Title,
            GoodreadsMatch::New => GoodreadsMatchDto::New,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOutcomeDto {
    Absent,
    Written,
    Unchanged,
    /// Already here and different. **Ours was kept** and a diagnostic says so.
    KeptOurs,
}

impl From<TextOutcome> for TextOutcomeDto {
    fn from(o: TextOutcome) -> Self {
        match o {
            TextOutcome::Absent => TextOutcomeDto::Absent,
            TextOutcome::Written => TextOutcomeDto::Written,
            TextOutcome::Unchanged => TextOutcomeDto::Unchanged,
            TextOutcome::KeptOurs => TextOutcomeDto::KeptOurs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodreadsBookReportDto {
    #[serde(default)]
    pub book_id: Option<i64>,
    pub title: String,
    pub matched_by: GoodreadsMatchDto,
    pub readings_added: usize,
    pub tags_added: usize,
    pub review: TextOutcomeDto,
    pub private_notes: TextOutcomeDto,
    #[serde(default)]
    pub rating: Option<u8>,
}

impl From<GoodreadsBookReport> for GoodreadsBookReportDto {
    fn from(r: GoodreadsBookReport) -> Self {
        GoodreadsBookReportDto {
            book_id: r.book_id,
            title: r.title,
            matched_by: r.matched_by.into(),
            readings_added: r.readings_added,
            tags_added: r.tags_added,
            review: r.review.into(),
            private_notes: r.private_notes.into(),
            rating: r.rating,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmatchedRowDto {
    pub row: usize,
    pub title: String,
    pub authors: Vec<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub candidates: Vec<MatchCandidateDto>,
}

impl From<UnmatchedRow> for UnmatchedRowDto {
    fn from(u: UnmatchedRow) -> Self {
        UnmatchedRowDto {
            row: u.row,
            title: u.title,
            authors: u.authors,
            external_id: u.external_id,
            candidates: u.candidates.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GoodreadsReportDto {
    pub dry_run: bool,
    pub rows: usize,
    pub books: Vec<GoodreadsBookReportDto>,
    pub unmatched: Vec<UnmatchedRowDto>,
    pub warnings: Vec<DiagnosticDto>,
}

impl From<GoodreadsReport> for GoodreadsReportDto {
    fn from(r: GoodreadsReport) -> Self {
        GoodreadsReportDto {
            dry_run: r.dry_run,
            rows: r.rows,
            books: r.books.into_iter().map(Into::into).collect(),
            unmatched: r.unmatched.into_iter().map(Into::into).collect(),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

// ---- calibre ---------------------------------------------------------------

/// What calibre this machine has. **Two options, not one flag** — a half
/// install degrades to the half that works, and a client shows the feature it
/// has rather than refusing both.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibreStatusDto {
    /// Absolute path of `ebook-convert`, when it was found.
    #[serde(default)]
    pub ebook_convert: Option<String>,
    #[serde(default)]
    pub calibredb: Option<String>,
}

impl From<&readingbuddy::Calibre> for CalibreStatusDto {
    fn from(c: &readingbuddy::Calibre) -> Self {
        CalibreStatusDto {
            ebook_convert: c.convert_path().map(path_str),
            calibredb: c.calibredb_path().map(path_str),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibreBookDto {
    pub calibre_id: i64,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub publish_year: Option<i64>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub isbn_10: Option<String>,
    #[serde(default)]
    pub isbn_13: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub added: Option<i64>,
}

impl From<CalibreBook> for CalibreBookDto {
    fn from(b: CalibreBook) -> Self {
        CalibreBookDto {
            calibre_id: b.calibre_id,
            uuid: b.uuid,
            title: b.title,
            authors: b.authors,
            publisher: b.publisher,
            publish_year: b.publish_year,
            language: b.language,
            isbn_10: b.isbn_10,
            isbn_13: b.isbn_13,
            description: b.description,
            tags: b.tags,
            cover: opt_path(&b.cover),
            formats: b.formats.iter().map(|p| path_str(p)).collect(),
            series: b.series,
            added: b.added,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibreMatchDto {
    Uuid,
    Isbn,
    Md5,
    Title,
    New,
}

impl From<CalibreMatch> for CalibreMatchDto {
    fn from(m: CalibreMatch) -> Self {
        match m {
            CalibreMatch::Uuid => CalibreMatchDto::Uuid,
            CalibreMatch::Isbn => CalibreMatchDto::Isbn,
            CalibreMatch::Md5 => CalibreMatchDto::Md5,
            CalibreMatch::Title => CalibreMatchDto::Title,
            CalibreMatch::New => CalibreMatchDto::New,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibreBookReportDto {
    #[serde(default)]
    pub book_id: Option<i64>,
    pub title: String,
    pub matched_by: CalibreMatchDto,
    pub tags_added: usize,
    pub cover: bool,
    pub files_linked: usize,
}

impl From<CalibreBookReport> for CalibreBookReportDto {
    fn from(r: CalibreBookReport) -> Self {
        CalibreBookReportDto {
            book_id: r.book_id,
            title: r.title,
            matched_by: r.matched_by.into(),
            tags_added: r.tags_added,
            cover: r.cover,
            files_linked: r.files_linked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmatchedCalibreBookDto {
    pub calibre_id: i64,
    #[serde(default)]
    pub uuid: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<MatchCandidateDto>,
}

impl From<readingbuddy::UnmatchedCalibreBook> for UnmatchedCalibreBookDto {
    fn from(u: readingbuddy::UnmatchedCalibreBook) -> Self {
        UnmatchedCalibreBookDto {
            calibre_id: u.calibre_id,
            uuid: u.uuid,
            title: u.title,
            authors: u.authors,
            candidates: u.candidates.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibreReportDto {
    pub dry_run: bool,
    pub rows: usize,
    pub books: Vec<CalibreBookReportDto>,
    pub unmatched: Vec<UnmatchedCalibreBookDto>,
    pub warnings: Vec<DiagnosticDto>,
}

impl From<CalibreReport> for CalibreReportDto {
    fn from(r: CalibreReport) -> Self {
        CalibreReportDto {
            dry_run: r.dry_run,
            rows: r.rows,
            books: r.books.into_iter().map(Into::into).collect(),
            unmatched: r.unmatched.into_iter().map(Into::into).collect(),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

// ---- where things live -----------------------------------------------------

/// The paths a settings screen shows. Not handles — a client that is not on
/// this machine can read them and can do nothing with them, which is correct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathsDto {
    pub db_url: String,
    pub images_dir: String,
    pub vault_dir: String,
    pub files_dir: String,
    pub log_dir: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_book_round_trips_through_json() {
        let dto = BookDto {
            id: Some(7),
            title: Some("Station Eleven".into()),
            authors: vec!["Emily St. John Mandel".into()],
            isbn_13: Some("9781447268963".into()),
            page_count: Some(333),
            ..BookDto::default()
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert_eq!(serde_json::from_str::<BookDto>(&json).unwrap(), dto);
    }

    /// The `#[serde(default)]` promise: a payload written before a field
    /// existed still parses. Every optional field carries it, so the minimum a
    /// client must send is nothing at all.
    #[test]
    fn an_empty_object_is_a_valid_book() {
        let dto: BookDto = serde_json::from_str("{}").unwrap();
        assert_eq!(dto, BookDto::default());
    }

    /// `Book -> BookDto -> Book` must not quietly drop a field, which is the
    /// one bug a hand-written `From` invites. Timestamps are the deliberate
    /// exception and are asserted as such.
    #[test]
    fn the_trip_through_the_dto_keeps_every_field_but_the_stamps() {
        let book = Book {
            id: Some(3),
            title: Some("t".into()),
            sort_title: Some("st".into()),
            authors: vec!["a".into()],
            translators: vec!["tr".into()],
            publisher: Some("p".into()),
            publish_year: Some(1999),
            language: Some("en".into()),
            isbn_10: Some("0316769487".into()),
            isbn_13: Some("9780316769488".into()),
            openlibrary_key: Some("OL1M".into()),
            googlebooks_id: Some("g1".into()),
            cover_url: Some("http://x/y.jpg".into()),
            cover_path: Some("images/y.jpg".into()),
            page_count: Some(200),
            description: Some("d".into()),
            first_sentence: Some("f".into()),
            current_page: Some(12),
            finished: true,
            date_started: Some(1),
            date_finished: Some(2),
            created_at: Some(time::OffsetDateTime::from_unix_timestamp(1000).unwrap()),
            last_modified: Some(time::OffsetDateTime::from_unix_timestamp(2000).unwrap()),
        };
        let dto = BookDto::from(book.clone());
        assert_eq!(dto.created_at, Some(1000));
        assert_eq!(dto.last_modified, Some(2000));

        let back = Book::from(dto);
        assert_eq!(back.id, book.id);
        assert_eq!(back.title, book.title);
        assert_eq!(back.sort_title, book.sort_title);
        assert_eq!(back.authors, book.authors);
        assert_eq!(back.translators, book.translators);
        assert_eq!(back.publisher, book.publisher);
        assert_eq!(back.publish_year, book.publish_year);
        assert_eq!(back.language, book.language);
        assert_eq!(back.isbn_10, book.isbn_10);
        assert_eq!(back.isbn_13, book.isbn_13);
        assert_eq!(back.openlibrary_key, book.openlibrary_key);
        assert_eq!(back.googlebooks_id, book.googlebooks_id);
        assert_eq!(back.cover_url, book.cover_url);
        assert_eq!(back.cover_path, book.cover_path);
        assert_eq!(back.page_count, book.page_count);
        assert_eq!(back.description, book.description);
        assert_eq!(back.first_sentence, book.first_sentence);
        assert_eq!(back.current_page, book.current_page);
        assert_eq!(back.finished, book.finished);
        assert_eq!(back.date_started, book.date_started);
        assert_eq!(back.date_finished, book.date_finished);
        // Storage stamps these; a client must not be able to backdate a row.
        assert_eq!(back.created_at, None);
        assert_eq!(back.last_modified, None);
    }

    /// The whole argument for mirroring `DiagnosticKind` in full rather than
    /// flattening it to a string: the far side can still tell *which* file, and
    /// still tell a timeout from a 500.
    #[test]
    fn a_diagnostic_keeps_its_structure_across_json() {
        let d = Diagnostic::sidecar_unparsable(
            PathBuf::from("/books/x.sdr/metadata.epub.lua"),
            &readingbuddy::EngineError::Sidecar("lua eval: nope".into()),
        );
        let display = d.to_string();
        let dto = DiagnosticDto::from(d);
        let json = serde_json::to_string(&dto).unwrap();
        let back: DiagnosticDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dto);
        assert_eq!(
            back.kind,
            DiagnosticKindDto::SidecarUnparsable {
                path: "/books/x.sdr/metadata.epub.lua".into()
            }
        );
        // And the CLI's exact wording survives, so no client re-derives it.
        assert_eq!(back.display, display);
    }

    #[test]
    fn a_timeout_carries_its_provider_and_its_seconds() {
        let d = Diagnostic::provider_timed_out(
            ProviderId::OpenLibrary,
            std::time::Duration::from_secs(5),
        );
        let dto = DiagnosticDto::from(d);
        assert_eq!(
            dto.kind,
            DiagnosticKindDto::ProviderTimedOut {
                provider: ProviderIdDto::OpenLibrary,
                after_secs: 5
            }
        );
        assert_eq!(dto.severity, SeverityDto::Warning);
    }

    #[test]
    fn a_device_state_names_itself_on_the_wire() {
        let dto = DeviceStateDto::from(DeviceState::Updated {
            new_highlights: 3,
            refreshed: 1,
        });
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["state"], "updated");
        assert_eq!(json["new_highlights"], 3);
    }

    /// An unmodelled KOReader status must arrive as itself. Collapsing it would
    /// be exactly the silence `UnknownDeviceStatus` exists to break.
    #[test]
    fn an_unknown_device_status_keeps_its_raw_string() {
        let dto = KoStatusDto::from(KoStatus::Other("tsundoku".into()));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["status"], "other");
        assert_eq!(json["raw"], "tsundoku");
    }
}
