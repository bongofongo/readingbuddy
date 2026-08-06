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
    ActivitySummary, Book, BookFile, BookFilter, BookImportStats, BookQuery, BookSort, BookSummary,
    BookTag, CalibreBook, CalibreBookReport, CalibreMatch, CalibreReport, Confidence, CreatedNote,
    DayActivity, DayRange, DeviceBook, DeviceScan, DeviceState, Diagnostic, DiagnosticKind,
    EditionShape, EnrichCandidate, EnrichMatch, EnrichOutcome, EnrichReport, ErrorClass,
    FieldChange, FieldSource, FileIdentity, FileImportReport, FileMatch, FileOutcome, FillStats,
    FlashcardRow, FractionSource, GoodreadsBookReport, GoodreadsReport, HeldField, Highlight,
    ImportReport, KoStatus, MatchCandidate, MatchMethod, MergeReport, Moment, MomentKind,
    MonthActivity, NewNoteInput, NoteKind, NoteRecord, OutgoingLink, Progress, PullReport,
    RankedResult, Rating, RatingScale, Reading, ReadingEvent, ReadingState, RefillReport,
    SearchHit, SearchOutcome, SearchRequest, SearchSource, Severity, ShapeSource, Source,
    StatsImportReport, StatusFilter, TableOfContents, TextOutcome, TocEntry, UnmatchedRow,
};

/// A path, as far as JSON can carry one. See the module doc.
fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn opt_path(p: &Option<PathBuf>) -> Option<String> {
    p.as_deref().map(path_str)
}

// ---- books ----------------------------------------------------------------

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
    /// What a provider says this book is about (migration `0013`). **Not
    /// `book_tags`**, which are minted shelves — see `Book::subjects`.
    ///
    /// Merges as a set, whole or not at all: `[]` on the way in means "this
    /// record does not say", never "this book has no subjects", so a client
    /// cannot clear them by omission and `save_book` has no way to clear them at
    /// all. That is `Engine::set_book_fields`' rule, not a wire limitation.
    #[serde(default)]
    pub subjects: Vec<String>,
    /// The series, and the place in it. **A pair**: `series_index` is only
    /// meaningful beside `series`, and sending an index alone writes a number
    /// under whatever name the row already had. The engine's own merge refuses
    /// that (`Rule::pair`), but this struct is a record rather than a merge, and
    /// a rule enforced here would be a rule the in-process caller never meets —
    /// which is the argument `crates/api/CLAUDE.md` makes about `dispatch`.
    #[serde(default)]
    pub series: Option<String>,
    /// Fractional on purpose: novellas are 0.5.
    #[serde(default)]
    pub series_index: Option<f64>,
    /// The **shelf tier** of the cover: a downscaled sibling written when the
    /// stored jacket is bigger than a grid cell can use, else the jacket
    /// itself, else absent (item 20c).
    ///
    /// **Derived, read-only, and the field a grid should load.** Providers are
    /// now asked for the largest cover they publish, which is right for a hero
    /// shot and wrong for sixty tiles; this is the one place "thumb, else the
    /// original" is decided, because a frontend reading `cover_thumb_path`
    /// directly shows nothing for every cover small enough not to have one.
    /// `cover_path` stays exactly what it was — a **whole path** — and is still
    /// the field a detail view wants.
    #[serde(default)]
    pub cover_shelf_path: Option<String>,
    /// Width over height of the stored cover — the box a tile reserves before
    /// the image loads.
    ///
    /// **Derived, read-only.** `None` means the cover has not been measured
    /// (a library predating migration `0014`, until
    /// `Engine::measure_stored_covers` runs), never that there is no cover; a
    /// frontend meeting `None` falls back to whatever it did before, which for
    /// the GUI is a fixed box and for the TUI is decoding the file.
    #[serde(default)]
    pub cover_aspect: Option<f64>,
    /// The cover's border colour, which is what a spine or a placeholder gets
    /// when there is no spine art — and there never is, since providers ship
    /// front covers only.
    ///
    /// **Derived, read-only, and a value rather than a name.** Channels, not a
    /// hex string: `#a02020` is CSS's spelling and a terminal wants floats, so
    /// the wire carries the measurement and each frontend writes it its own
    /// way. Unclamped — the TUI's legibility band is its own policy.
    #[serde(default)]
    pub cover_accent: Option<AccentDto>,
    /// Read-only projections of the **current** reading. Sending them back in a
    /// `save_book` changes nothing: `upsert_book` has ignored them since
    /// migration `0005`, and `update_progress` is the writer.
    #[serde(default)]
    pub current_page: Option<i64>,
    #[serde(default)]
    pub finished: bool,
    /// The current reading's state, or absent when the book has no reading at
    /// all — which is the commonest state in a real library and the one
    /// `finished: false` cannot be told apart from *abandoned*.
    ///
    /// **Typed since item 17**, where it crossed as a bare `String`. It is
    /// stored as one and always will be (an importer can write a status this
    /// build does not know, and a parse that refused one would turn a foreign
    /// device's vocabulary into an error on the read path) — but a *wire* that
    /// hands a frontend a bare string hands every frontend the same `switch`
    /// with the same three magic words in it, and the one that misspells
    /// `abandoned` styles a put-down book as an active read. `Other { raw }`
    /// carries the unknown value whole, so nothing is lost. See
    /// [`readingbuddy::ReadingState`], and note there is deliberately **no
    /// variant for "no reading"** — that is this field's `None`.
    #[serde(default)]
    pub reading_state: Option<ReadingStateDto>,
    #[serde(default)]
    pub date_started: Option<i64>,
    #[serde(default)]
    pub date_finished: Option<i64>,
    /// How far into the current reading this is — item 17b.
    ///
    /// **Derived, read-only, and ignored on the way in.** Here rather than left
    /// to each frontend because the arithmetic has three hazards that a
    /// frontend reading `current_page` and `page_count` cannot see: a real book
    /// with `page_count = 0` (a false denominator, not a length), a real book
    /// with `page_count = NULL` (absence, not zero), and the device's own
    /// `ko_percent`, which is the better answer for a book whose length we do
    /// not know and which no `BookDto` field exposes. See
    /// [`readingbuddy::Progress`].
    #[serde(default)]
    pub progress: ProgressDto,
    /// The series and its place in it, written the way a person writes it:
    /// `Dune #2`, never `Dune #2.0`.
    ///
    /// **Derived, read-only.** `series_index` is a `REAL`, so a frontend
    /// formatting the pair itself will eventually print `#2.5` one way and
    /// `#2.50` another — `readingbuddy::Book::series_label` exists precisely so
    /// two frontends cannot disagree, and until item 17 it was not on this
    /// struct at all, so the GUI reconstructed the pair and happened to agree
    /// because JS prints whole floats without a decimal point. That is
    /// agreement by coincidence.
    #[serde(default)]
    pub series_label: Option<String>,
    /// The authors, each written the way a person says it — `Jorge Luis
    /// Borges`, never `Borges, Jorge Luis`.
    ///
    /// **Derived, read-only.** `authors` stays exactly as the origin spelled
    /// it, because that is the record; this is the parse. The **join** between
    /// names is still phrasing and still the frontend's, which is why this is a
    /// list and not a sentence.
    #[serde(default)]
    pub authors_display: Vec<String>,
    /// What shape this edition is, in multiples of its own height — item 19,
    /// carried across the seam by item 18.
    ///
    /// **Derived, read-only.** A shelf needs it *per row* and the GUI links this
    /// crate and not the engine, so without it the only way to draw three
    /// hundred spines is to re-derive the arithmetic in TypeScript — where it
    /// would eventually disagree with the TUI's Unicode book about how fat
    /// *Infinite Jest* is, with nothing on either screen looking wrong. That is
    /// the failure item 19 exists to prevent, and it is prevented here or not at
    /// all.
    ///
    /// Never `None`: a book always has *a* shape, and
    /// [`EditionShapeDto::width_source`]/`thickness_source` say which of its two
    /// numbers were assumed.
    #[serde(default)]
    pub shape: EditionShapeDto,
    /// Unix seconds. `OffsetDateTime`'s own serde format is a dependency's
    /// choice; an integer is ours.
    #[serde(default)]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub last_modified: Option<i64>,
}

impl From<Book> for BookDto {
    fn from(b: Book) -> Self {
        // Derived first, because the literal below moves the fields they read.
        // Every one of these is a value the engine decided; none of them is a
        // word a user sees.
        let reading_state = b.reading_state().map(ReadingStateDto::from);
        let progress = ProgressDto::from(Progress::of_book(&b));
        let series_label = b.series_label();
        let authors_display = b.authors_in_display_order();
        let cover_shelf_path = b.shelf_cover_path().map(str::to_string);
        let cover_aspect = b.cover_aspect();
        let cover_accent = b.cover_accent_rgb().map(AccentDto::from);
        // Item 20's stored dimensions are what makes this a division of two
        // columns rather than three hundred image decodes — the rewire item 19
        // said would need no signature change, and did not.
        let shape =
            EditionShapeDto::from(EditionShape::of_book(&b, cover_aspect.map(|a| a as f32)));
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
            subjects: b.subjects,
            series: b.series,
            series_index: b.series_index,
            cover_shelf_path,
            cover_aspect,
            cover_accent,
            current_page: b.current_page,
            finished: b.finished,
            reading_state,
            date_started: b.date_started,
            date_finished: b.date_finished,
            progress,
            series_label,
            authors_display,
            shape,
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
    /// backdate a row by mistake. The reading projections are carried
    /// through because the struct has the fields, and are ignored downstream by
    /// `upsert_book` — which is where that rule already lives, and where it
    /// should stay rather than being re-implemented here.
    ///
    /// The **derived** fields — `progress`, `series_label`, `authors_display`,
    /// `shape`, and item 20's three cover readings — are dropped outright. They
    /// are
    /// readings of other columns and there is nothing to write them to; a client
    /// that sends a `series_label` of "Dune #2" and a `series_index` of 3 has
    /// not asked for anything, and taking the label would be inventing a second
    /// writer for a pair that already has one. The cover measurements are
    /// stronger than that: `Storage::set_cover` is their only writer *by
    /// design*, since a width arriving without the file it measures is a row
    /// describing an image it is not pointing at. `ko_percent` is dropped for
    /// the same reason it is not on this struct: it is device-owned and
    /// `update_progress` is its writer.
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
            reading_status: d.reading_state.map(|s| ReadingState::from(s).to_string()),
            ko_percent: None,
            date_started: d.date_started,
            date_finished: d.date_finished,
            subjects: d.subjects,
            series: d.series,
            series_index: d.series_index,
            // Not on this struct at all, and that is the point: the wire carries
            // the three *readings* above and no way to write the columns behind
            // them. `Storage::set_cover` writes all four beside the file it
            // measured, or none of them.
            cover_width: None,
            cover_height: None,
            cover_accent: None,
            cover_thumb_path: None,
            created_at: None,
            last_modified: None,
        }
    }
}

/// A colour as its three channels, 0–255.
///
/// The wire shape of a cover's border accent. **Channels rather than a hex
/// string**: `#a02020` is CSS's spelling and the TUI wants floats in 0..1, so a
/// hex string would be one frontend's dialect crossing the seam and the other
/// one parsing it back out. This is the measurement; the spelling stays where
/// the spelling belongs.
///
/// It is not a colour *name* and never will be — `docs/decisions.md`'s "no
/// prose" applies to a value a user never reads as words.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccentDto {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<[u8; 3]> for AccentDto {
    fn from([r, g, b]: [u8; 3]) -> Self {
        AccentDto { r, g, b }
    }
}

/// A reading's state, typed. The mirror of [`readingbuddy::ReadingState`], and
/// deliberately the same shape as [`KoStatusDto`] — one is our vocabulary, the
/// other is the device's, and both have to survive a word neither build knows.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ReadingStateDto {
    Reading,
    Finished,
    /// Put down. Never *failed*, never *did not finish* — the word a frontend
    /// chooses is its own, but this is the state it must be able to see, and
    /// before item 17 it could not without a stringly comparison.
    Abandoned,
    Other {
        raw: String,
    },
}

impl From<ReadingState> for ReadingStateDto {
    fn from(s: ReadingState) -> Self {
        match s {
            ReadingState::Reading => ReadingStateDto::Reading,
            ReadingState::Finished => ReadingStateDto::Finished,
            ReadingState::Abandoned => ReadingStateDto::Abandoned,
            ReadingState::Other(raw) => ReadingStateDto::Other { raw },
        }
    }
}

impl From<ReadingStateDto> for ReadingState {
    fn from(s: ReadingStateDto) -> Self {
        match s {
            ReadingStateDto::Reading => ReadingState::Reading,
            ReadingStateDto::Finished => ReadingState::Finished,
            ReadingStateDto::Abandoned => ReadingState::Abandoned,
            ReadingStateDto::Other { raw } => ReadingState::Other(raw),
        }
    }
}

/// Where a fraction came from. A frontend may say so or not; what it may not do
/// is compute one and forget which it was.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FractionSourceDto {
    /// `current_page / page_count`, with an honest denominator.
    Pages,
    /// The device's own percentage, for a book whose length we do not know.
    Device,
}

impl From<FractionSource> for FractionSourceDto {
    fn from(s: FractionSource) -> Self {
        match s {
            FractionSource::Pages => FractionSourceDto::Pages,
            FractionSource::Device => FractionSourceDto::Device,
        }
    }
}

/// How far into a book a reading is. The mirror of [`readingbuddy::Progress`],
/// and item 17b's whole point: the arithmetic crosses the wire as a value so
/// that neither frontend does it.
///
/// `percent` is carried beside `fraction` rather than left to
/// `Math.round(fraction * 100)` on the other side, and it is not redundant: the
/// page-based percentage is an **integer division**, and `29/100` is
/// `0.28999999999999998` in binary, so flooring the float gives 28 where the
/// division gives 29. Two frontends already did it in integers and were right
/// to; this is how the third gets the same number.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "progress", rename_all = "snake_case")]
pub enum ProgressDto {
    /// Nothing recorded. **Not zero**, and not *unread* — a book nobody opened
    /// and a book opened at page one are the same here, and the difference is
    /// `reading_state`.
    #[default]
    Untouched,
    Started {
        #[serde(default)]
        page: Option<i64>,
        /// The length. **Never `Some(0)`**: a zero page count is a false
        /// denominator and is absence by the time it reaches here.
        #[serde(default)]
        of: Option<i64>,
        /// `0.0..=1.0`, or absent — a page with no length has no percentage,
        /// and a bar must not draw an empty track over one.
        #[serde(default)]
        fraction: Option<f64>,
        #[serde(default)]
        percent: Option<i64>,
        #[serde(default)]
        source: Option<FractionSourceDto>,
    },
    /// Closed as read. Carries no page: every frontend that had this case
    /// dropped the numbers for a word.
    Finished,
}

impl From<Progress> for ProgressDto {
    fn from(p: Progress) -> Self {
        match p {
            Progress::Untouched => ProgressDto::Untouched,
            Progress::Finished => ProgressDto::Finished,
            Progress::Started { page, of, fraction } => ProgressDto::Started {
                page,
                of,
                fraction: fraction.map(|f| f.value),
                percent: p.percent(),
                source: fraction.map(|f| f.source.into()),
            },
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookSortDto {
    #[default]
    LastModified,
    Title,
    Progress,
    /// Alphabetically by the first author's last name, authorless books last —
    /// item 17. The one key SQL cannot express, since it is a parse of a human
    /// name; see [`BookSort::Author`].
    Author,
    /// Newest first, undated last.
    Year,
}

impl From<BookSortDto> for BookSort {
    fn from(s: BookSortDto) -> Self {
        match s {
            BookSortDto::LastModified => BookSort::LastModified,
            BookSortDto::Title => BookSort::Title,
            BookSortDto::Progress => BookSort::Progress,
            BookSortDto::Author => BookSort::Author,
            BookSortDto::Year => BookSort::Year,
        }
    }
}

/// Which books a list is about, before it is ordered or paged (item 18).
///
/// Every field absent is *every book*, so a client with no opinion sends
/// nothing — and `Request::ListBooks` carries this as an `Option`, which is why
/// the default and the absence are the same answer twice over rather than a
/// distinction nobody wanted.
///
/// The mirror of [`readingbuddy::BookFilter`]; the predicates and their
/// case-sensitivity are documented there, because they are properties of the
/// SQL and not of the wire.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookFilterDto {
    #[serde(default)]
    pub status: Option<StatusFilterDto>,
    /// A substring of the title, case-insensitive — the same rule `author`
    /// states below it, and **not** the full-text index `SearchMarks` uses.
    ///
    /// A title search is a predicate here rather than a seventh endpoint
    /// because item 18 already made every list a filter + sort + page over one
    /// shared `WHERE`, and this joins the five that were already there: it
    /// composes with `CountBooks`, with the offset paging and with every sort,
    /// none of which a separate method would get. The engine's `BookFilter`
    /// carries the argument for `LIKE` over fts5 in full.
    #[serde(default)]
    pub title: Option<String>,
    /// A substring of the stored author list, case-insensitive.
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub language: Option<String>,
    /// A minted shelf name out of `book_tags` — **not** `subjects`.
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub has_cover: Option<bool>,
}

impl From<BookFilterDto> for BookFilter {
    fn from(f: BookFilterDto) -> Self {
        BookFilter {
            status: f.status.map(Into::into),
            title: f.title,
            author: f.author,
            year: f.year,
            language: f.language,
            tag: f.tag,
            has_cover: f.has_cover,
        }
    }
}

/// What the current reading's state has to be for a book to be in the answer.
///
/// Four reachable cases: the three states this build writes, an importer's own
/// word through [`ReadingStateDto::Other`], and **absence**.
///
/// [`StatusFilterDto::NoReading`] is here rather than as a fifth
/// `ReadingStateDto` variant, and that placement is the ruling: absence is a
/// question somebody asked once, not a permanent claim about every book. A
/// variant is a thing a UI filters on, counts, and eventually puts a badge
/// beside — which is the completion framing `docs/decisions.md` bans, and which
/// `no_reading_is_absence_rather_than_a_variant` exists to keep out.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "match", rename_all = "snake_case")]
pub enum StatusFilterDto {
    /// The book's current reading is in this state.
    State { is: ReadingStateDto },
    /// The book has no reading at all.
    NoReading,
}

impl From<StatusFilterDto> for StatusFilter {
    fn from(s: StatusFilterDto) -> Self {
        match s {
            StatusFilterDto::State { is } => StatusFilter::Is(is.into()),
            StatusFilterDto::NoReading => StatusFilter::NoReading,
        }
    }
}

/// One page of the library: which books, in what order, and which slice.
///
/// The typed argument to [`crate::Api::list_books`]. `Request::ListBooks`
/// carries the same four values **flat**, so a payload of `{"limit":20,
/// "sort":"title"}` still means what it always did and no client had to change
/// for this item — the offset and the filter are additive, and an omitted one is
/// the old behaviour exactly.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookQueryDto {
    #[serde(default)]
    pub sort: BookSortDto,
    #[serde(default)]
    pub filter: Option<BookFilterDto>,
    /// **Negative means no limit.** The whole-library read, and the same reading
    /// SQLite gives `LIMIT -1`.
    pub limit: i64,
    /// Rows to skip. **Pagination here is an offset and not a cursor** — see
    /// `docs/decisions.md` entry 18 for the argument, which turns on two of the
    /// five sorts having no cursor key that exists in the database.
    #[serde(default)]
    pub offset: i64,
}

impl From<BookQueryDto> for BookQuery {
    fn from(q: BookQueryDto) -> Self {
        BookQuery {
            sort: q.sort.into(),
            filter: q.filter.map(Into::into).unwrap_or_default(),
            limit: q.limit,
            offset: q.offset,
        }
    }
}

/// What is behind one book — item 18's answer to the four-calls-per-row problem.
///
/// **Counts, not marks.** A tile saying *3 highlights* and a tile showing a dot
/// are different products, and this is the more informative one at the same
/// cost; a frontend that wants the dot reads `> 0`, which
/// [`readingbuddy::BookSummary::has`] spells once so twelve components do not
/// each spell it.
///
/// Every number here is **past tense** — highlights taken, notes written, files
/// owned. Nothing in this struct counts what has not been done, and nothing
/// derived from it may.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookSummaryDto {
    pub book_id: i64,
    pub highlights: i64,
    pub notes: i64,
    pub files: i64,
}

impl From<BookSummary> for BookSummaryDto {
    fn from(s: BookSummary) -> Self {
        BookSummaryDto {
            book_id: s.book_id,
            highlights: s.highlights,
            notes: s.notes,
            files: s.files,
        }
    }
}

/// The physical shape of one edition, in multiples of its own height (item 19).
///
/// **Derived, read-only**, and here because item 19 landed `EditionShape` in the
/// engine while the GUI links this crate and not the engine — so a WebGL shelf
/// either gets this field or re-derives the arithmetic in TypeScript, which is
/// the exact failure item 19 was written to prevent. Height is `1.0` and is not
/// a field; see [`readingbuddy::EditionShape`] for why millimetres were refused.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EditionShapeDto {
    pub width_over_height: f32,
    pub width_source: ShapeSourceDto,
    pub thickness_over_height: f32,
    pub thickness_source: ShapeSourceDto,
}

/// Whether one of [`EditionShapeDto`]'s numbers came from anything recorded.
///
/// Mirrored as an enum rather than flattened to `width_assumed: bool`, for
/// `DiagnosticKind`'s reason: the vocabulary is the information, and a boolean
/// named after one of two states is a coin flip about which way it reads.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeSourceDto {
    /// Derived from something recorded about this edition.
    Recorded,
    /// Nobody recorded it, so a trade paperback stood in.
    Assumed,
}

impl From<ShapeSource> for ShapeSourceDto {
    fn from(s: ShapeSource) -> Self {
        match s {
            ShapeSource::Recorded => ShapeSourceDto::Recorded,
            ShapeSource::Assumed => ShapeSourceDto::Assumed,
        }
    }
}

impl Default for EditionShapeDto {
    /// The trade paperback the engine stands in with when nothing was recorded,
    /// **through the engine's own function** — a literal here would be a second
    /// copy of the fallback, and the two would part company the day the curve
    /// moves.
    fn default() -> Self {
        EditionShape::new(None, None).into()
    }
}

impl From<EditionShape> for EditionShapeDto {
    fn from(s: EditionShape) -> Self {
        EditionShapeDto {
            width_over_height: s.width_over_height,
            width_source: s.width_source.into(),
            thickness_over_height: s.thickness_over_height,
            thickness_source: s.thickness_source.into(),
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingDto {
    pub id: i64,
    pub book_id: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    /// `null` means open, and at most one reading per book may be.
    #[serde(default)]
    pub finished_at: Option<i64>,
    /// Typed since item 17 — see [`BookDto::reading_state`] for the argument.
    /// The column stays a `String`; the wire does not.
    pub status: ReadingStateDto,
    /// `manual` | `koreader` | `migrated` | `goodreads` | `calibre`, and
    /// deliberately still a `String`.
    ///
    /// Item 17 typed `status` and stopped here on purpose. `status` had an
    /// engine type to mirror, a fixed three-word vocabulary, and frontends
    /// branching on it. `source` has none of the three: it is the *name of a
    /// writer*, it grows by one every time an importer is added, and nothing
    /// branches on it — it is shown. An enum here would be a second list of
    /// importers to keep in step with the first, which is the shape this repo
    /// spends `MERGE_RULES` avoiding.
    pub source: String,
    #[serde(default)]
    pub current_page: Option<i64>,
    /// The device's own status, typed — [`KoStatusDto`] already existed and was
    /// not being used here.
    #[serde(default)]
    pub ko_status: Option<KoStatusDto>,
    #[serde(default)]
    pub ko_percent: Option<f64>,
    #[serde(default)]
    pub ko_rating: Option<i64>,
    pub created_at: i64,
    pub last_modified: i64,
    /// **This** reading's progress, not the book's (item 22).
    ///
    /// `BookDto::progress` is the *current* read's, which on a reread would
    /// print today's numbers under an older read's heading —
    /// `readingbuddy::Progress::of_book` says so in as many words. The pairing
    /// with the book's length is done in the engine
    /// (`Engine::readings_with_progress`), because `readings` has no page count
    /// and deciding which length goes with which read is a derivation, not a
    /// projection.
    pub progress: ProgressDto,
}

impl ReadingDto {
    /// A reading beside the progress the engine computed for it.
    ///
    /// There is deliberately **no `From<Reading>`**: a `Reading` alone cannot
    /// answer `progress`, and an impl that filled it with
    /// `Progress::of_reading(&r, None)` would quietly report "no percentage"
    /// for every book whose length we know perfectly well.
    pub fn new(r: Reading, progress: Progress) -> ReadingDto {
        ReadingDto {
            progress: progress.into(),
            ..ReadingDto::bare(r)
        }
    }

    fn bare(r: Reading) -> ReadingDto {
        ReadingDto {
            progress: ProgressDto::Untouched,
            id: r.id,
            book_id: r.book_id,
            started_at: r.started_at,
            finished_at: r.finished_at,
            status: ReadingState::from(r.status.as_str()).into(),
            source: r.source,
            current_page: r.current_page,
            ko_status: r.ko_status.map(|s| KoStatus::from(s.as_str()).into()),
            ko_percent: r.ko_percent,
            ko_rating: r.ko_rating,
            created_at: r.created_at,
            last_modified: r.last_modified,
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
    /// Which reading this was captured during, when the dates could place it.
    ///
    /// `#[serde(default)]` like every other optional field here, and `null` is
    /// an ordinary answer rather than a missing one: a highlight captured
    /// between two readings belongs to neither, and the device cannot tell us
    /// otherwise.
    #[serde(default)]
    pub reading_id: Option<i64>,
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
            reading_id: h.reading_id,
            source: h.source,
            created_at: h.created_at,
        }
    }
}

// ---- notes ----------------------------------------------------------------

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

/// Which index a search is about, or `null` for both.
///
/// A **filter**, in `BookFilterDto`'s idiom rather than a second method:
/// `null` is not asking, and naming one narrows what is in the list without
/// touching how the list is ordered. It exists because "search my notes" is a
/// real question — `rb notes --search` has asked it since item 7 — while the
/// ordering never belongs to the caller.
///
/// It is emphatically **not** a way to fetch the two halves separately and
/// interleave them above the seam. Two calls give a client two rankings and no
/// rule for merging them, which is the thing [`Request::SearchMarks`] exists to
/// make unnecessary.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSourceDto {
    Note,
    Highlight,
}

impl From<SearchSourceDto> for SearchSource {
    fn from(s: SearchSourceDto) -> Self {
        match s {
            SearchSourceDto::Note => SearchSource::Note,
            SearchSourceDto::Highlight => SearchSource::Highlight,
        }
    }
}

/// One hit in the one ranked list, carrying whichever row matched.
///
/// Internally tagged on `kind`, the shape [`StatusFilterDto`] already uses, so
/// a client switches on one string rather than probing for which field is
/// present. **Not two nullable fields**: exactly one of the two rows exists,
/// and a shape that can represent neither or both is a shape somebody
/// eventually reads wrong.
///
/// The order these arrive in is the engine's and is the whole point of the
/// method — see [`Request::SearchMarks`]. Nothing above this layer re-sorts it.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchHitDto {
    Note {
        note: NoteDto,
        /// The matching text with `>>` and `<<` around the terms.
        snippet: String,
    },
    Highlight {
        highlight: HighlightDto,
        snippet: String,
    },
}

impl From<SearchHit> for SearchHitDto {
    fn from(h: SearchHit) -> Self {
        match h {
            SearchHit::Note { note, snippet } => SearchHitDto::Note {
                note: note.into(),
                snippet,
            },
            SearchHit::Highlight { highlight, snippet } => SearchHitDto::Highlight {
                highlight: highlight.into(),
                snippet,
            },
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenReadingDto {
    pub book: BookDto,
    pub reading: ReadingDto,
}

impl From<(Book, Reading)> for OpenReadingDto {
    fn from((book, reading): (Book, Reading)) -> Self {
        // The reading's own progress, through the engine's own function. The
        // length is right here on the book, so this is the one place the pair
        // needs no round trip — and calling `Progress::of_reading` is not the
        // derivation `ReadingDto::progress` warns about, it *is* the one
        // implementation of it.
        let progress = Progress::of_reading(&reading, book.page_count);
        OpenReadingDto {
            book: book.into(),
            reading: ReadingDto::new(reading, progress),
        }
    }
}

// ---- ratings ---------------------------------------------------------------

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingMapEntryDto {
    pub value: f64,
    pub goodreads: u8,
}

// ---- files and flashcards --------------------------------------------------

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileIdentityDto {
    pub path: String,
    pub sha256: String,
    pub partial_md5: String,
    pub format: String,
    pub size: i64,
    #[serde(default)]
    pub title: Option<String>,
    /// How long the file says it is — pdf only, and absent far more often than
    /// present (item 22).
    ///
    /// **Absent is not zero**, and a client must not render it as one: a `0`
    /// here would be a false denominator, which is the exact thing
    /// `ProgressDto` was built to make unrepresentable.
    #[serde(default)]
    pub page_count: Option<i64>,
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
            page_count: i.page_count,
            matched_book_id: i.matched.map(|(id, _)| id),
            matched_by: i.matched.map(|(_, m)| m.into()),
            candidates: i.candidates.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
    /// The count is the field, not a list: the engine reports this once per
    /// file, so a frontend renders one line however many highlights it names.
    SidecarAnchorsUnsupported {
        path: String,
        entries: usize,
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
    // ---- koreader statistics (item 31) -------------------------------------
    //
    // These cross the seam as five variants rather than folding into one
    // "statistics unavailable", because *absence is the ordinary path* here and
    // a frontend's next move differs for each: no database is a device whose
    // owner never enabled the plugin (say nothing, or offer to explain), an
    // unknown schema is a readingbuddy that needs updating, and an unmatched
    // book is an invitation to link it.
    StatisticsDbAbsent {
        path: String,
    },
    StatisticsDbUnreadable {
        path: String,
        class: ErrorClassDto,
    },
    StatisticsSchemaUnknown {
        path: String,
        version: i64,
    },
    StatisticsBookUnmatched {
        md5: String,
    },
    StatisticsBookNotIdentified {
        title: String,
    },
    /// A provider's cover URL would not download. The metadata it came with
    /// landed; the URL itself is deliberately not carried across the wire.
    CoverUnavailable {
        class: ErrorClassDto,
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
            K::SidecarAnchorsUnsupported { path, entries } => {
                DiagnosticKindDto::SidecarAnchorsUnsupported {
                    path: path_str(&path),
                    entries,
                }
            }
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
            K::StatisticsDbAbsent { path } => DiagnosticKindDto::StatisticsDbAbsent {
                path: path_str(&path),
            },
            K::StatisticsDbUnreadable { path, class } => {
                DiagnosticKindDto::StatisticsDbUnreadable {
                    path: path_str(&path),
                    class: class.into(),
                }
            }
            K::StatisticsSchemaUnknown { path, version } => {
                DiagnosticKindDto::StatisticsSchemaUnknown {
                    path: path_str(&path),
                    version,
                }
            }
            K::StatisticsBookUnmatched { md5 } => {
                DiagnosticKindDto::StatisticsBookUnmatched { md5 }
            }
            K::StatisticsBookNotIdentified { title } => {
                DiagnosticKindDto::StatisticsBookNotIdentified { title }
            }
            K::CoverUnavailable { class } => DiagnosticKindDto::CoverUnavailable {
                class: class.into(),
            },
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
/// A library book that might be the thing being imported, offered as a choice.
///
/// **A row to show, not a record to write**, and that is what decides its
/// fields. The only thing a client does with one is send `book_id` back to
/// `link_sidecar`/`link_calibre_book`/`link_goodreads_row`, so nothing here
/// round-trips; what it has to do is answer *which of these is it* in the one
/// screen a refusal leads to.
///
/// Item 36 added `authors_display` and `publish_year`. Before them a chooser
/// could not answer the question it was asking — two Dunes with one title and
/// two authors read identically — and the only way to recover the author was a
/// `get_book` per row, which is an N+1 against a list that is per *row* of a
/// calibre or Goodreads import. Additive, so `API_VERSION` did not move.
///
/// **`cover_path` is deliberately absent.** No chooser draws a jacket: the TUI's
/// picker is a text list, the CLI prints lines, and the GUI has no chooser yet.
/// And when one does, the field it needs is not this one — item 20 made
/// `cover_shelf_path` the thing a grid loads, beside `cover_aspect` and
/// `cover_accent`, because a frontend reading `cover_path` directly shows
/// nothing for every cover small enough to have no thumb. So the honest addition
/// is a *cluster*, decided by the screen that draws it, and it stays additive
/// and cheap; a path shipped now is a string per row on a list that can be one
/// per book in the library.
///
/// **The whole `BookDto` was considered and refused.** `band` is holding the
/// entire `Book`, and picking fields off it is what created this bug — but a
/// candidate list is a question about *identity*, and a `BookDto` answers a
/// different one: it carries `description` (kilobytes), `subjects`,
/// `first_sentence`, and `progress`/`reading_status`, which would invite a
/// chooser to draw a progress bar on a book it is asking you to identify.
/// Measured rather than asserted: a `BookDto` for a book with a real
/// description serializes to roughly twenty times a candidate, and a calibre
/// import reports candidates *per row*. Getting the picking wrong again costs
/// one more additive field; `BookDto` costs every import report, for ever.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCandidateDto {
    pub book_id: i64,
    pub title: String,
    /// The authors, each in reading order — `Frank Herbert`, never
    /// `Herbert, Frank`.
    ///
    /// **Derived and read-only**, the same fact `BookDto::authors_display`
    /// carries and under the same name. There is no raw `authors` beside it:
    /// the order within a name is a parse and is the engine's
    /// (`readingbuddy::names`), and a client of this crate does not link the
    /// engine — a raw list would be that parse re-implemented above the seam.
    #[serde(default)]
    pub authors_display: Vec<String>,
    /// The year, where the record has one. What is left to choose by when the
    /// band's ordinary case — a variant title of a book already on the shelf —
    /// puts the same author on both rows.
    #[serde(default)]
    pub publish_year: Option<i64>,
    pub score: f64,
}

impl From<MatchCandidate> for MatchCandidateDto {
    fn from(c: MatchCandidate) -> Self {
        MatchCandidateDto {
            book_id: c.book_id,
            title: c.title,
            authors_display: c.authors_display,
            publish_year: c.publish_year,
            score: c.score,
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibreBookReportDto {
    /// The calibre row this line is about — the only thing tying a report line
    /// back to the shelf row a client is showing.
    pub calibre_id: i64,
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
            calibre_id: r.calibre_id,
            book_id: r.book_id,
            title: r.title,
            matched_by: r.matched_by.into(),
            tags_added: r.tags_added,
            cover: r.cover,
            files_linked: r.files_linked,
        }
    }
}

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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

// ---- the chapter list (item 32) --------------------------------------------

/// One line of a book's table of contents.
///
/// Flat with a `depth`, exactly as the engine has it — a tree here would make
/// "the entries, in order" the awkward shape on the wire too, and every consumer
/// walks them in reading order. The nesting is a column, not a loss.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocEntryDto {
    pub label: String,
    /// 0 for a top-level entry, 1 for a section inside one.
    pub depth: usize,
    /// The book's own name for the place (`OEBPS/ch2.xhtml#part-two`), not a
    /// path on this machine — a client on another machine can still match it
    /// against the file it is rendering.
    pub target: String,
    /// Where `target` sits in the spine, where it names a spine item at all.
    /// Absent is ordinary and is **not** a page number.
    #[serde(default)]
    pub spine_index: Option<usize>,
}

impl From<TocEntry> for TocEntryDto {
    fn from(e: TocEntry) -> Self {
        TocEntryDto {
            label: e.label,
            depth: e.depth,
            target: e.target,
            spine_index: e.spine_index,
        }
    }
}

/// A book's chapter list, and the file it was read from.
///
/// **Derived on every call and stored nowhere** — see `epub::table_of_contents`
/// for the argument. What that means on the wire is that `sha256` is not
/// decoration: it is how a client knows *which* file answered, since a book may
/// own several and a better one may be attached tomorrow.
///
/// The method returns `null` for "no file here we can read" and this struct with
/// an empty `entries` for "this epub carries no TOC". Two different answers; a
/// client that collapses them tells the user the same thing about a missing file
/// and an ordinary EPUB3 book.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableOfContentsDto {
    pub sha256: String,
    pub entries: Vec<TocEntryDto>,
}

impl From<TableOfContents> for TableOfContentsDto {
    fn from(t: TableOfContents) -> Self {
        TableOfContentsDto {
            sha256: t.sha256,
            entries: t.entries.into_iter().map(Into::into).collect(),
        }
    }
}

// ---- the activity log (items 21 and 31) ------------------------------------

/// How much a row is willing to claim.
///
/// Mirrored as an enum rather than crossing as the `"measured"`/`"inferred"`
/// text the column holds: the engine reads an unrecognised token *back* as
/// `inferred` on purpose, and a client doing its own string comparison would
/// get the opposite default — claiming a measurement nobody made.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceDto {
    Measured,
    Inferred,
}

impl From<Confidence> for ConfidenceDto {
    fn from(c: Confidence) -> Self {
        match c {
            Confidence::Measured => ConfidenceDto::Measured,
            Confidence::Inferred => ConfidenceDto::Inferred,
        }
    }
}

/// One day of one book, as one source saw it.
///
/// **`minutes` and `pages` absent means "not known", never zero**, and that is
/// the whole reason this table exists rather than a KOReader-shaped one. A
/// library that arrived as a Goodreads CSV has no minutes at all; a screen that
/// renders `null` as `0` has told its reader something false about their own
/// reading. A measured twenty-second session, by contrast, really does record
/// `0`.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingEventDto {
    pub book_id: i64,
    /// Which read this day belongs to, where the evidence agrees on one.
    /// Absent when no reading's window holds it, or when two do.
    #[serde(default)]
    pub reading_id: Option<i64>,
    /// `YYYY-MM-DD`, UTC.
    pub day: String,
    #[serde(default)]
    pub minutes: Option<i64>,
    #[serde(default)]
    pub pages: Option<i64>,
    /// Free text (`koreader`, `vault`, a reading's own source). Not an enum:
    /// the vocabulary lives in a comment rather than a `CHECK` so it can grow,
    /// and a closed enum here would make it a wire-breaking change to add one.
    pub source: String,
    pub confidence: ConfidenceDto,
    pub created_at: i64,
}

impl From<ReadingEvent> for ReadingEventDto {
    fn from(e: ReadingEvent) -> Self {
        ReadingEventDto {
            book_id: e.book_id,
            reading_id: e.reading_id,
            day: e.day,
            minutes: e.minutes,
            pages: e.pages,
            source: e.source,
            confidence: e.confidence.into(),
            created_at: e.created_at,
        }
    }
}

/// What one filler pass did. `updated` counts rows that **actually changed**.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillStatsDto {
    pub inserted: u64,
    pub updated: u64,
}

impl From<FillStats> for FillStatsDto {
    fn from(s: FillStats) -> Self {
        FillStatsDto {
            inserted: s.inserted,
            updated: s.updated,
        }
    }
}

/// One pass of every filler that needs no device.
///
/// Broken out per filler rather than totalled, because a refill reporting `0`
/// overall and `0` for the vault are different facts — the second says the
/// notes filler ran and found nothing, which is what a second identical run is
/// supposed to say.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefillReportDto {
    pub highlights: FillStatsDto,
    pub notes: FillStatsDto,
    pub readings: FillStatsDto,
}

impl From<RefillReport> for RefillReportDto {
    fn from(r: RefillReport) -> Self {
        RefillReportDto {
            highlights: r.highlights.into(),
            notes: r.notes.into(),
            readings: r.readings.into(),
        }
    }
}

/// The period a summary is about, echoed back.
///
/// Echoed rather than assumed: the caller sent two strings and the engine
/// validated them, so a reply carrying the range it actually used is what lets a
/// client label a chart without re-deriving what it asked for.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayRangeDto {
    pub from: String,
    pub to: String,
}

impl From<&DayRange> for DayRangeDto {
    fn from(r: &DayRange) -> Self {
        DayRangeDto {
            from: r.from().to_string(),
            to: r.to().to_string(),
        }
    }
}

/// What is known about a period.
///
/// The counts are `i64` because the engine fully originates the evidence behind
/// them — a zero there is knowable. `minutes` and `pages` are not: read them as
/// "we have no data", and see [`ReadingEventDto`] for why that distinction is
/// carried this far.
///
/// There is deliberately nothing here counting what the user has *not* done.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivitySummaryDto {
    pub range: DayRangeDto,
    pub books_finished: i64,
    pub activity_days: i64,
    pub notes_created: i64,
    pub links_created: i64,
    #[serde(default)]
    pub minutes: Option<i64>,
    #[serde(default)]
    pub pages: Option<i64>,
}

impl From<ActivitySummary> for ActivitySummaryDto {
    fn from(s: ActivitySummary) -> Self {
        ActivitySummaryDto {
            range: DayRangeDto::from(&s.range),
            books_finished: s.books_finished,
            activity_days: s.activity_days,
            notes_created: s.notes_created,
            links_created: s.links_created,
            minutes: s.minutes,
            pages: s.pages,
        }
    }
}

/// One day of a period. **Only days carrying an event appear** — the gaps are
/// the client's to draw, and filling them with zero rows here would be the same
/// lie `minutes: null` exists to avoid, spread across a calendar.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayActivityDto {
    pub day: String,
    /// Distinct books with an event that day.
    pub books: i64,
    #[serde(default)]
    pub minutes: Option<i64>,
    #[serde(default)]
    pub pages: Option<i64>,
}

impl From<DayActivity> for DayActivityDto {
    fn from(d: DayActivity) -> Self {
        DayActivityDto {
            day: d.day,
            books: d.books,
            minutes: d.minutes,
            pages: d.pages,
        }
    }
}

/// One month of a period (item 42). **Only months carrying an event appear**,
/// the same rule and the same reason as [`DayActivityDto`].
///
/// This is not a client's bucketing of [`DayActivityDto`] and could not be.
/// `minutes: null` collapses to `0` the first time a client reduces over it,
/// which is precisely the lie the nullability exists to prevent — spread over a
/// calendar, where a month the device never measured becomes a month you read
/// for zero minutes. And `books` is **distinct over the whole month**, so it
/// cannot be recovered from the days at all: a reader who opened the same two
/// books on twelve days read two books that month, not twenty-four.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthActivityDto {
    /// `YYYY-MM`.
    pub month: String,
    /// Distinct books with an event anywhere in the month.
    pub books: i64,
    /// Days of this month that carry an event.
    pub activity_days: i64,
    #[serde(default)]
    pub minutes: Option<i64>,
    #[serde(default)]
    pub pages: Option<i64>,
}

impl From<MonthActivity> for MonthActivityDto {
    fn from(m: MonthActivity) -> Self {
        MonthActivityDto {
            month: m.month,
            books: m.books,
            activity_days: m.activity_days,
            minutes: m.minutes,
            pages: m.pages,
        }
    }
}

// ---- moments (item 23) -----------------------------------------------------

/// What happened, and whatever the phrasing needs beyond the ids.
///
/// Adjacently tagged on `kind`, the shape `DeviceStateDto` already uses, so a
/// client switches on one field and a payload that grows a member does not
/// change how the others read.
///
/// **The wording is a frontend's and the fact is the engine's**, which is item
/// 17's line applied here: `run_ended` carries a span and a number of days, and
/// the sentence built out of them — including whether "3" is spelled *three* —
/// belongs to whoever is drawing.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MomentKindDto {
    /// A read ended. `MomentDto::reading_id` is that read.
    ReadingClosed,
    /// The first mark of the reader's own on a book that carried none.
    FirstAnnotation,
    /// A reflection reached a book it had not reached before.
    /// `MomentDto::book_id` is the reflection's own book; `reached_book_id` is
    /// the far side.
    ReflectionReached { note_id: i64, reached_book_id: i64 },
    /// A run of consecutive days with activity, **which has already ended**.
    /// There is no variant for a run in progress, and that is deliberate — a
    /// run visible while it runs is a streak.
    RunEnded {
        from: String,
        to: String,
        /// How many days it held. Past tense, like a count of your own
        /// highlights; never a target and never compared with anything.
        days: i64,
    },
}

impl From<MomentKind> for MomentKindDto {
    fn from(k: MomentKind) -> Self {
        match k {
            MomentKind::ReadingClosed => MomentKindDto::ReadingClosed,
            MomentKind::FirstAnnotation => MomentKindDto::FirstAnnotation,
            MomentKind::ReflectionReached {
                note_id,
                reached_book_id,
            } => MomentKindDto::ReflectionReached {
                note_id,
                reached_book_id,
            },
            MomentKind::RunEnded { from, to, days } => MomentKindDto::RunEnded { from, to, days },
        }
    }
}

/// One thing worth noticing, derived on every ask and stored nowhere.
///
/// **`reading_id` is here beside `book_id` and is not decoration.** A reread is
/// a second read of one book, so anything minted per reading — item 28's cards
/// — cannot be selected by the book alone. It is absent where the evidence does
/// not settle on one read, which is the same call `ReadingEventDto::reading_id`
/// makes and for the same reason.
///
/// **`id` is opaque.** It is built by the engine out of the rows the moment is
/// made of; hand it back to `acknowledge_moment` and never parse it. Its format
/// is not part of this vocabulary and may gain a kind without a version bump.
///
/// **There is no count anywhere on this surface** — no total, no unread, no
/// remaining. The length of the list is the frontend's business and nothing
/// else's; `docs/decisions.md` forbids a badge counting what you have not done,
/// and a `pending: 3` on a reply is exactly that with a different name.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MomentDto {
    /// Opaque. Give it back to acknowledge; do not read it.
    pub id: String,
    pub kind: MomentKindDto,
    /// Absent only for `run_ended`, which is about the library rather than a
    /// book.
    #[serde(default)]
    pub book_id: Option<i64>,
    /// Which read this belongs to, where the evidence settles on one.
    #[serde(default)]
    pub reading_id: Option<i64>,
    /// `YYYY-MM-DD`, UTC — the same day convention as the activity log.
    pub day: String,
    /// Unix seconds. The list arrives newest first by this.
    pub occurred_at: i64,
}

impl From<Moment> for MomentDto {
    fn from(m: Moment) -> Self {
        MomentDto {
            id: m.id,
            kind: m.kind.into(),
            book_id: m.book_id,
            reading_id: m.reading_id,
            day: m.day,
            occurred_at: m.occurred_at,
        }
    }
}

/// What an import of the device's `statistics.sqlite3` did (item 31).
///
/// Every field is a number a user could check against their device, which is
/// the point: `books_in_db` against `books_matched` is the whole of "why did
/// this import so little", and answering it without a round trip is why the
/// counts cross rather than just the `FillStats`.
///
/// A device whose owner never enabled the statistics plugin comes back here with
/// a `warnings` entry and every count zero — **not an error**. Absence is
/// ordinary; `schema_version` absent is how a client tells "no database" from
/// "a database with nothing in it".
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsImportReportDto {
    #[serde(default)]
    pub schema_version: Option<i64>,
    pub books_in_db: usize,
    pub books_matched: usize,
    pub days: usize,
    pub events: FillStatsDto,
    pub warnings: Vec<DiagnosticDto>,
}

impl From<StatsImportReport> for StatsImportReportDto {
    fn from(r: StatsImportReport) -> Self {
        StatsImportReportDto {
            schema_version: r.schema_version,
            books_in_db: r.books_in_db,
            books_matched: r.books_matched,
            days: r.days,
            events: r.events.into(),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

// ---- enrichment and provenance (items 29 and 30) ---------------------------

/// Who supplied a field. Mirrored in full, for `DiagnosticKind`'s reason: a
/// client branches on `user` — the rank that outranks every provider — and
/// flattening this to a string would make that branch a string comparison
/// against a vocabulary nothing on the wire pins.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDto {
    OpenLibrary,
    GoogleBooks,
    Calibre,
    Epub,
    /// A PDF we own said so — item 22. Beside `Epub` because the two answer
    /// different questions: an epub supplies a title and an ISBN and never a
    /// length, a pdf supplies a length and nothing else.
    Pdf,
    Koreader,
    Goodreads,
    User,
}

impl From<Source> for SourceDto {
    fn from(s: Source) -> Self {
        match s {
            Source::OpenLibrary => SourceDto::OpenLibrary,
            Source::GoogleBooks => SourceDto::GoogleBooks,
            Source::Calibre => SourceDto::Calibre,
            Source::Epub => SourceDto::Epub,
            Source::Pdf => SourceDto::Pdf,
            Source::KOReader => SourceDto::Koreader,
            Source::Goodreads => SourceDto::Goodreads,
            Source::User => SourceDto::User,
        }
    }
}

/// Where one field came from, and when (item 29).
///
/// **An absent entry means nobody has claimed the field** — every book predating
/// migration `0012` reports an empty list however well-populated it is. So a
/// client must render "unattributed", never "unknown provider".
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSourceDto {
    pub field: String,
    /// The stored token. Text rather than [`SourceDto`] because this is read
    /// straight off the column, whose vocabulary deliberately lives in a comment
    /// rather than a `CHECK` — a closed enum here would fail to parse a row a
    /// newer engine wrote.
    pub source: String,
    pub fetched_at: i64,
}

impl From<FieldSource> for FieldSourceDto {
    fn from(f: FieldSource) -> Self {
        FieldSourceDto {
            field: f.field,
            source: f.source,
            fetched_at: f.fetched_at,
        }
    }
}

/// How a provider record was tied to the book.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichMatchDto {
    /// By ISBN, which is an identity — this is about *this edition*.
    Isbn,
    /// By title and author, at this score.
    Title { score: f64 },
}

impl From<EnrichMatch> for EnrichMatchDto {
    fn from(m: EnrichMatch) -> Self {
        match m {
            EnrichMatch::Isbn => EnrichMatchDto::Isbn,
            EnrichMatch::Title(score) => EnrichMatchDto::Title { score },
        }
    }
}

/// A record that looked like this book but not enough to write unasked.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnrichCandidateDto {
    pub book: BookDto,
    pub score: f64,
    pub sources: Vec<ProviderIdDto>,
}

impl From<EnrichCandidate> for EnrichCandidateDto {
    fn from(c: EnrichCandidate) -> Self {
        EnrichCandidateDto {
            book: c.book.into(),
            score: c.score,
            sources: c.sources.into_iter().map(Into::into).collect(),
        }
    }
}

/// What happened, at the granularity a frontend branches on.
///
/// The five are not interchangeable and collapsing any two of them is the bug
/// this enum was carved to prevent: `nothing_found` is a fact about the book,
/// `no_answer` is a fact about the network wearing the same shape, `refused`
/// means results came back and none was certainly this book, and `unaskable`
/// means there was nothing to ask with.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichOutcomeDto {
    Enriched { matched_by: EnrichMatchDto },
    Refused { candidates: Vec<EnrichCandidateDto> },
    NothingFound,
    NoAnswer,
    Unaskable,
}

impl From<EnrichOutcome> for EnrichOutcomeDto {
    fn from(o: EnrichOutcome) -> Self {
        match o {
            EnrichOutcome::Enriched(how) => EnrichOutcomeDto::Enriched {
                matched_by: how.into(),
            },
            EnrichOutcome::Refused { candidates } => EnrichOutcomeDto::Refused {
                candidates: candidates.into_iter().map(Into::into).collect(),
            },
            EnrichOutcome::NothingFound => EnrichOutcomeDto::NothingFound,
            EnrichOutcome::NoAnswer => EnrichOutcomeDto::NoAnswer,
            EnrichOutcome::Unaskable => EnrichOutcomeDto::Unaskable,
        }
    }
}

/// One field that changed, and who is now answerable for it.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChangeDto {
    pub field: String,
    /// Absent where the write could not name an origin, which is not the same
    /// as an origin nobody recognises.
    #[serde(default)]
    pub source: Option<SourceDto>,
    #[serde(default)]
    pub before: Option<String>,
    pub after: String,
}

impl From<FieldChange> for FieldChangeDto {
    fn from(c: FieldChange) -> Self {
        FieldChangeDto {
            field: c.field.to_string(),
            source: c.source.map(Into::into),
            before: c.before,
            after: c.after,
        }
    }
}

/// A field a provider offered and was not allowed to write.
///
/// **The value the provider offered is carried, not just the field name.** A
/// held-back field reported by name alone is indistinguishable from a field the
/// provider had nothing for, which is the "silently not updated" state the whole
/// report exists to remove.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeldFieldDto {
    pub field: String,
    pub offered: String,
    #[serde(default)]
    pub offered_by: Option<SourceDto>,
    /// What was kept instead. Absent where the user owns the field's *pair*
    /// rather than the field, so there is nothing of theirs in this column.
    #[serde(default)]
    pub kept: Option<String>,
}

impl From<HeldField> for HeldFieldDto {
    fn from(h: HeldField) -> Self {
        HeldFieldDto {
            field: h.field.to_string(),
            offered: h.offered,
            offered_by: h.offered_by.map(Into::into),
            kept: h.kept,
        }
    }
}

/// What one run of `enrich_book_from_providers` did (item 30).
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnrichReportDto {
    pub book_id: i64,
    pub outcome: EnrichOutcomeDto,
    pub filled: Vec<FieldChangeDto>,
    pub held: Vec<HeldFieldDto>,
    /// Where the cover landed, if one was fetched.
    #[serde(default)]
    pub cover: Option<String>,
    pub warnings: Vec<DiagnosticDto>,
}

impl From<EnrichReport> for EnrichReportDto {
    fn from(r: EnrichReport) -> Self {
        EnrichReportDto {
            book_id: r.book_id,
            outcome: r.outcome.into(),
            filled: r.filled.into_iter().map(Into::into).collect(),
            held: r.held.into_iter().map(Into::into).collect(),
            cover: opt_path(&r.cover),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

// ---- where things live -----------------------------------------------------

/// The paths a settings screen shows. Not handles — a client that is not on
/// this machine can read them and can do nothing with them, which is correct.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
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
    /// one bug a hand-written `From` invites. **Timestamps are now the only
    /// deliberate exception**, and are asserted as such —
    /// `subjects`/`series`/`series_index` were the other three until this
    /// surfacing item, and their assertions were what made the gap an API gap
    /// with a name rather than a silent drop.
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
            reading_status: Some("finished".into()),
            ko_percent: Some(0.5),
            date_started: Some(1),
            date_finished: Some(2),
            subjects: vec!["Fiction / Literary".into()],
            series: Some("Dune".into()),
            series_index: Some(2.0),
            cover_width: Some(600),
            cover_height: Some(900),
            cover_accent: Some(0x00a0_2020),
            cover_thumb_path: Some("images/y.thumb.jpg".into()),
            created_at: Some(time::OffsetDateTime::from_unix_timestamp(1000).unwrap()),
            last_modified: Some(time::OffsetDateTime::from_unix_timestamp(2000).unwrap()),
        };
        let dto = BookDto::from(book.clone());
        assert_eq!(dto.created_at, Some(1000));
        assert_eq!(dto.last_modified, Some(2000));

        let back = Book::from(dto.clone());
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
        assert_eq!(back.reading_status, book.reading_status);
        assert_eq!(back.date_started, book.date_started);
        assert_eq!(back.date_finished, book.date_finished);
        assert_eq!(back.subjects, book.subjects);
        assert_eq!(back.series, book.series);
        assert_eq!(back.series_index, book.series_index);
        // Storage stamps these; a client must not be able to backdate a row.
        assert_eq!(back.created_at, None);
        assert_eq!(back.last_modified, None);
        // **And `ko_percent` does not come back**, which is deliberate rather
        // than a drop: it is a read-only projection of a device-owned column
        // whose writer is `update_progress`. The four fields beside it survive
        // only because `upsert_book` ignores them, which is where that rule
        // lives; carrying a value nothing can write would be the worse of the
        // two.
        assert_eq!(back.ko_percent, None);
        // **And neither do the four cover columns**, for a stronger version of
        // `ko_percent`'s reason: `Storage::set_cover` is their only writer *by
        // design*, since a width arriving without the file it measures is a row
        // describing an image it is not pointing at. The wire carries the three
        // readings of them (`cover_aspect`, `cover_accent`,
        // `cover_shelf_path`) and no way to write them back.
        assert_eq!(dto.cover_aspect, Some(600.0 / 900.0));
        assert_eq!(
            dto.cover_accent,
            Some(AccentDto {
                r: 0xa0,
                g: 0x20,
                b: 0x20
            })
        );
        assert_eq!(dto.cover_shelf_path.as_deref(), Some("images/y.thumb.jpg"));
        assert_eq!(back.cover_width, None);
        assert_eq!(back.cover_height, None);
        assert_eq!(back.cover_accent, None);
        assert_eq!(back.cover_thumb_path, None);
    }

    /// The derived fields are **readings, not columns**: they arrive on the way
    /// out and are dropped on the way in, because there is nothing to write
    /// them to. A `series_label` of "Dune #2" beside a `series_index` of 3 is
    /// not a request.
    #[test]
    fn the_derived_fields_cross_outward_only() {
        let book = Book {
            authors: vec!["Borges, Jorge Luis".into()],
            series: Some("Dune".into()),
            // A REAL that must not print as `#2.0` — the drift `series_label`
            // exists to prevent.
            series_index: Some(2.0),
            current_page: Some(50),
            page_count: Some(200),
            ..Book::default()
        };
        let dto = BookDto::from(book);
        assert_eq!(dto.series_label.as_deref(), Some("Dune #2"));
        assert_eq!(dto.authors_display, vec!["Jorge Luis Borges".to_string()]);
        // The record itself is untouched: `authors` is what the origin spelled.
        assert_eq!(dto.authors, vec!["Borges, Jorge Luis".to_string()]);
        assert_eq!(
            dto.progress,
            ProgressDto::Started {
                page: Some(50),
                of: Some(200),
                fraction: Some(0.25),
                percent: Some(25),
                source: Some(FractionSourceDto::Pages),
            }
        );

        let back = Book::from(dto);
        assert_eq!(back.series, Some("Dune".to_string()));
        assert_eq!(back.series_index, Some(2.0));
        assert_eq!(back.authors, vec!["Borges, Jorge Luis".to_string()]);
    }

    /// A candidate crosses the seam knowing who wrote it (item 36).
    ///
    /// The wire is where this matters most: the GUI links this crate and **not**
    /// the engine, so before the field existed the only way to put an author in
    /// a chooser row was a `get_book` per candidate — one request per row, on a
    /// list a calibre import produces per row of its own. The names arrive
    /// already parsed for the same reason: `readingbuddy::names` is not
    /// reachable from TypeScript.
    #[test]
    fn a_candidate_crosses_the_seam_knowing_who_wrote_it() {
        let dto = MatchCandidateDto::from(MatchCandidate {
            book_id: 7,
            title: "Dune".into(),
            authors_display: vec!["Frank Herbert".into()],
            publish_year: Some(1965),
            score: 0.71,
        });
        let json = serde_json::to_string(&dto).unwrap();
        let back: MatchCandidateDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dto);
        assert_eq!(back.authors_display, ["Frank Herbert"]);
        assert_eq!(back.publish_year, Some(1965));

        // Both fields are `#[serde(default)]`, so a payload written before they
        // existed still parses — which is what let them land without moving
        // `API_VERSION`.
        let old: MatchCandidateDto =
            serde_json::from_str(r#"{"book_id":7,"title":"Dune","score":0.71}"#).unwrap();
        assert!(old.authors_display.is_empty());
        assert_eq!(old.publish_year, None);
    }

    /// The false denominator, at the seam. A real book in `make dev-db` has
    /// `page_count = 0`; before item 17b the CLI printed `[12/0]` over it.
    #[test]
    fn a_zero_page_count_never_reaches_a_client_as_a_denominator() {
        let dto = BookDto::from(Book {
            current_page: Some(12),
            page_count: Some(0),
            ..Book::default()
        });
        assert_eq!(
            dto.progress,
            ProgressDto::Started {
                page: Some(12),
                of: None,
                fraction: None,
                percent: None,
                source: None,
            }
        );
    }

    /// An importer can write a status this build does not know, and it must
    /// cross whole rather than collapsing to a placeholder or failing to parse.
    #[test]
    fn an_unknown_reading_state_survives_the_wire() {
        let dto = BookDto::from(Book {
            reading_status: Some("tsundoku".into()),
            ..Book::default()
        });
        assert_eq!(
            dto.reading_state,
            Some(ReadingStateDto::Other {
                raw: "tsundoku".into()
            })
        );
        let json = serde_json::to_string(&dto).unwrap();
        let round: BookDto = serde_json::from_str(&json).unwrap();
        assert_eq!(round.reading_state, dto.reading_state);
        // And back to the column's own word, unchanged.
        assert_eq!(
            Book::from(round).reading_status.as_deref(),
            Some("tsundoku")
        );
    }

    /// No reading at all is **absence**, not a state. There is no `NeverOpened`
    /// variant and this is what asserts it stays that way — a named state is
    /// one a UI filters on, counts, and eventually puts a badge beside, which
    /// is the framing `docs/decisions.md` bans.
    #[test]
    fn no_reading_is_absence_rather_than_a_variant() {
        let dto = BookDto::from(Book::default());
        assert_eq!(dto.reading_state, None);
        assert_eq!(dto.progress, ProgressDto::Untouched);
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
