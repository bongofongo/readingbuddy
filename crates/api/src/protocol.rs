//! The request/response vocabulary, and the envelope it travels in.
//!
//! This is the part `crates/daemon` exists to move bytes for and **not to
//! understand**. A transport reads a [`Call`], hands it to [`crate::Api::call`]
//! and writes the [`Reply`]; there is no branch on the method anywhere outside
//! this crate, which is the whole reason iOS can link the same surface with no
//! daemon at all.
//!
//! ## Why the requests are named and the responses are shaped
//!
//! [`Request`] has one variant per facade method, adjacently tagged, so a
//! payload reads as `{"method":"get_book","params":{"id":3}}` — a name a human
//! can grep for and a client can construct without a schema in front of them.
//!
//! [`Response`] is deliberately *not* the mirror of that. Sixty-odd response
//! variants that each appear exactly once would be sixty names to keep in sync
//! for no information: a reply is already tied to its call by [`Call::id`], so
//! the client knows what it asked. What it needs from the wire is the *shape*,
//! and there are about thirty of those.
//!
//! ## Not here: the mount watcher
//!
//! `MountWatcher` is a stream of events with no request to answer, and
//! request/response has no shape for "tell me when a reader arrives". Wrapping
//! it as a poll would be an invention rather than a translation — it would give
//! the far side a different debounce from the one `watch.rs` guarantees. A
//! subscription is its own design and belongs with the transports that need
//! one; a client on this machine calls `scan_device` when it likes.

use serde::{Deserialize, Serialize};

use crate::dto::*;
use crate::error::ApiError;

/// The version of *this vocabulary*, bumped only when an existing method or
/// shape changes meaning.
///
/// Adding a method does not bump it: an older client never sends the new name,
/// and a newer client meets [`crate::error::ErrorCode::BadRequest`] on an older
/// daemon — which is a clear failure rather than a silent misread, and is why
/// the number can stay still through ordinary growth.
pub const API_VERSION: u32 = 1;

/// The build, for a human reading a log. Never branch on it.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A protocol enum is built once per call and consumed immediately, so the size
/// of its largest variant is not a cost anyone pays. Boxing to even the
/// variants out would change nothing on the wire — `Box<T>` is transparent to
/// serde — and would put a `Box::new` at every construction site for a value
/// that lives for one `await`.
///
/// One call. Every variant maps to exactly one [`crate::Api`] method.
#[allow(clippy::large_enum_variant)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    // ---- meta and configuration ----
    ApiVersion,
    Paths,
    GoogleApiKey,
    SetGoogleApiKey {
        #[serde(default)]
        key: Option<String>,
    },
    VerifyGoogleKey {
        key: String,
    },

    // ---- metadata search ----
    Search {
        request: SearchRequestDto,
    },
    LookupIsbn {
        isbn: String,
    },

    // ---- library ----
    SaveBook {
        book: BookDto,
    },
    ListBooks {
        limit: i64,
        #[serde(default)]
        sort: BookSortDto,
    },
    GetBook {
        id: i64,
    },
    ResolveBooks {
        selector: String,
    },
    BookTags {
        book_id: i64,
    },
    /// Ask the providers about a book already in the library (item 30).
    ///
    /// No bulk form, here or on the facade: the per-book cost is a provider
    /// fan-out and a loop over the shelf is a rate-limit policy nobody has
    /// decided.
    EnrichBook {
        book_id: i64,
    },
    /// Record what the user says, and that it was the user who said it.
    ///
    /// **Sets and cannot clear** — the merge is the ordinary partial-record one,
    /// so a field the record is silent about is left alone. Every field written
    /// is stamped `user` and is from then on held against every provider merge.
    SetBookFields {
        book_id: i64,
        fields: BookDto,
    },
    /// Where each field of a book came from, and when (item 29). An **absent**
    /// field means nobody has claimed it.
    FieldProvenance {
        book_id: i64,
    },
    CurrentlyReading {
        limit: i64,
    },
    DeleteBook {
        id: i64,
    },
    FetchCover {
        book_id: i64,
    },
    MergeBooks {
        src: i64,
        dst: i64,
    },

    // ---- readings ----
    ListReadings {
        book_id: i64,
    },
    GetReading {
        id: i64,
    },
    ActiveReading {
        book_id: i64,
    },
    UpdateProgress {
        book_id: i64,
        #[serde(default)]
        page: Option<i64>,
        #[serde(default)]
        finished: Option<bool>,
    },
    Reread {
        book_id: i64,
    },

    // ---- highlights ----
    ListHighlights {
        book_id: i64,
    },
    /// What was highlighted during one reading. The reading-scoped half of
    /// `ListHighlights`, keyed by the reading rather than the book.
    HighlightsForReading {
        reading_id: i64,
    },
    SetAnnotation {
        highlight_id: i64,
        #[serde(default)]
        annotation: Option<String>,
    },

    // ---- epub and owned files ----
    ImportEpub {
        path: String,
    },
    ImportFile {
        path: String,
        /// Create a book even over a near-miss candidate. Without it an
        /// ambiguous file comes back as `Unmatched` with **nothing written**.
        #[serde(default)]
        new: bool,
    },
    AddFileToBook {
        book_id: i64,
        path: String,
    },
    IdentifyFile {
        path: String,
    },
    BookFiles {
        book_id: i64,
    },
    /// The chapter list, read out of the owned epub on every call and stored
    /// nowhere — see [`TableOfContentsDto`].
    TableOfContents {
        book_id: i64,
    },
    FilePath {
        sha256: String,
    },
    RemoveFile {
        sha256: String,
    },

    // ---- koreader ----
    ImportKoreader {
        path: String,
        #[serde(default)]
        dry_run: bool,
    },
    PullBookFromSidecar {
        path: String,
    },
    SidecarCandidates {
        path: String,
    },
    LinkSidecar {
        path: String,
        book_id: i64,
    },

    // ---- the device ----
    CandidateMounts,
    IsKoreaderMount {
        path: String,
    },
    ScanDevice {
        root: String,
    },
    SyncDevice {
        paths: Vec<String>,
    },
    /// Measured reading time out of the device's `statistics.sqlite3`.
    ///
    /// **A method of its own, and deliberately not part of `sync_device`** —
    /// `docs/decisions.md` makes arrival read-only, and a scan that quietly
    /// began importing months of timing data would not be read-only in spirit.
    /// The user asks for this by name, and so does a client.
    ImportDeviceStatistics {
        mount: String,
    },

    // ---- the activity log ----
    /// Rebuild the log from everything already stored: highlight stamps, note
    /// timestamps, reading endpoints. Idempotent, and called by no importer —
    /// a log that refilled itself as a side effect would be whichever importer
    /// ran last.
    RefillReadingEvents,
    ReadingEvents {
        book_id: i64,
    },
    /// Both ends inclusive, `YYYY-MM-DD`. An inverted range is a
    /// [`crate::error::ErrorCode::InvalidInput`], never a confident empty
    /// answer.
    ActivitySummary {
        from: String,
        to: String,
    },
    /// The days behind `ActivitySummary::activity_days`. Only days carrying an
    /// event come back.
    ActivityByDay {
        from: String,
        to: String,
    },

    // ---- notes ----
    CreateNote {
        note: NewNoteDto,
    },
    ListNotes {
        #[serde(default)]
        book_id: Option<i64>,
    },
    SearchNotes {
        query: String,
        limit: i64,
    },
    GetNote {
        id: i64,
    },
    NoteForReading {
        reading_id: i64,
        kind: NoteKindDto,
    },
    NotePath {
        note_id: i64,
    },
    NoteBody {
        note_id: i64,
    },
    UpdateNoteBody {
        note_id: i64,
        body: String,
    },
    DeleteNote {
        note_id: i64,
    },
    RefreshNoteFromDisk {
        note_id: i64,
    },
    OutgoingLinks {
        note_id: i64,
    },
    Backlinks {
        note_id: i64,
    },

    // ---- reflection and review ----
    OpenReflection {
        book_id: i64,
        #[serde(default)]
        reading_id: Option<i64>,
    },
    OpenReview {
        book_id: i64,
        #[serde(default)]
        reading_id: Option<i64>,
    },

    // ---- ratings ----
    SetRating {
        note_id: i64,
        value: f64,
    },
    ReviewRating {
        note_id: i64,
    },
    ClearReviewRating {
        note_id: i64,
    },
    GoodreadsRating {
        note_id: i64,
    },
    PutRatingScale {
        name: String,
        min: f64,
        max: f64,
        step: f64,
    },
    RatingScales,
    ActiveRatingScale,
    RatingScaleByName {
        name: String,
    },
    RatingMap {
        scale_id: i64,
    },
    MapRating {
        scale_id: i64,
        value: f64,
        goodreads: u8,
    },

    // ---- citations ----
    Cite {
        note_id: i64,
        highlight_id: i64,
    },
    Uncite {
        note_id: i64,
        highlight_id: i64,
    },
    CitationsFor {
        note_id: i64,
    },

    // ---- goodreads ----
    ImportGoodreads {
        path: String,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        create_ambiguous: bool,
    },
    ExportGoodreads,
    LinkGoodreadsRow {
        external_id: String,
        book_id: i64,
    },

    // ---- calibre ----
    CalibreStatus,
    ConvertEbook {
        input: String,
        output: String,
        #[serde(default)]
        overwrite: bool,
    },
    CalibreLibrary {
        #[serde(default)]
        library: Option<String>,
    },
    ImportCalibreLibrary {
        #[serde(default)]
        library: Option<String>,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        create_ambiguous: bool,
        /// Import only these calibre rows. Empty — and so an absent field — is
        /// the whole library, which is what keeps an older client's request
        /// meaning what it did before.
        #[serde(default)]
        only: Vec<i64>,
    },
    LinkCalibreBook {
        uuid: String,
        book_id: i64,
    },

    // ---- flashcards ----
    ListFlashcards {
        #[serde(default)]
        include_exported: bool,
    },
    ListFlashcardsForBook {
        book_id: i64,
    },
    ExportFlashcards {
        #[serde(default)]
        include_exported: bool,
    },
}

/// What came back, by shape. See [`Request`] on the size of these variants.
#[allow(clippy::large_enum_variant)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", content = "value", rename_all = "snake_case")]
pub enum Response {
    /// It worked and there was nothing to say.
    Unit,
    Bool(bool),
    Id(i64),
    Text(String),
    /// A path, or nothing. `null` is an answer here, not an omission — a book
    /// with no cover URL has no cover to fetch.
    MaybePath(Option<String>),
    Paths(Vec<String>),
    Version {
        api: u32,
        crate_version: String,
    },
    Where(PathsDto),

    Book(Option<BookDto>),
    Books(Vec<BookDto>),
    BookTags(Vec<BookTagDto>),
    OpenReadings(Vec<OpenReadingDto>),
    MergeReport(MergeReportDto),

    Reading(Option<ReadingDto>),
    Readings(Vec<ReadingDto>),

    Highlights(Vec<HighlightDto>),

    SearchOutcome(SearchOutcomeDto),

    Note(Option<NoteDto>),
    Notes(Vec<NoteDto>),
    NoteHits(Vec<NoteSearchHitDto>),
    Links(Vec<OutgoingLinkDto>),
    CreatedNote(CreatedNoteDto),

    Rating(Option<RatingDto>),
    GoodreadsRating(Option<u8>),
    RatingScale(Option<RatingScaleDto>),
    RatingScales(Vec<RatingScaleDto>),
    RatingMap(Vec<RatingMapEntryDto>),

    BookFiles(Vec<BookFileDto>),
    FileIdentity(FileIdentityDto),
    FileImport(FileImportReportDto),
    /// `null` is "no file here we can read a chapter list from", which is a
    /// different answer from a present value with no entries.
    TableOfContents(Option<TableOfContentsDto>),

    EnrichReport(EnrichReportDto),
    FieldProvenance(Vec<FieldSourceDto>),

    ReadingEvents(Vec<ReadingEventDto>),
    RefillReport(RefillReportDto),
    ActivitySummary(ActivitySummaryDto),
    ActivityByDay(Vec<DayActivityDto>),
    StatsImport(StatsImportReportDto),

    ImportReport(ImportReportDto),
    PullReport(PullReportDto),
    PullReports(Vec<PullReportDto>),
    Candidates(Vec<MatchCandidateDto>),
    DeviceScan(DeviceScanDto),

    GoodreadsReport(GoodreadsReportDto),
    /// The CSV, plus every honest failure along the way. The payload comes
    /// back and the caller owns the file — same contract the flashcard export
    /// has had since the beginning.
    GoodreadsExport {
        csv: String,
        warnings: Vec<DiagnosticDto>,
    },

    CalibreStatus(CalibreStatusDto),
    CalibreLibrary(Vec<CalibreBookDto>),
    CalibreReport(CalibreReportDto),

    Flashcards(Vec<FlashcardDto>),
    FlashcardExport {
        tsv: String,
        count: usize,
    },
}

/// A request with an id, as it arrives.
///
/// The id is the client's, echoed back untouched: a transport may pipeline, and
/// without it a client with two calls in flight cannot tell the replies apart.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    #[serde(default)]
    pub id: u64,
    pub request: Request,
}

/// A reply, always well-formed.
///
/// `outcome` is an ordinary tagged enum rather than `Result`, because
/// `Result`'s serde shape is `{"Ok":…}` — capitalised, and a detail of the
/// standard library rather than a decision anyone made about this protocol.
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    /// Stamped on every reply, so a client can refuse a daemon it does not
    /// understand without a handshake round trip.
    pub api_version: u32,
    pub outcome: Outcome,
}

#[allow(clippy::large_enum_variant)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "bindings.ts")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ok { response: Response },
    Error { error: ApiError },
}

impl Reply {
    pub fn ok(id: u64, response: Response) -> Reply {
        Reply {
            id,
            api_version: API_VERSION,
            outcome: Outcome::Ok { response },
        }
    }

    pub fn err(id: u64, error: ApiError) -> Reply {
        Reply {
            id,
            api_version: API_VERSION,
            outcome: Outcome::Error { error },
        }
    }

    /// The reply as a single line of JSON, newline included.
    ///
    /// Framing lives here rather than in the transport for one reason: a
    /// `Reply` whose payload contained a raw newline would desynchronise a
    /// line-delimited stream, and `serde_json::to_string` is what guarantees it
    /// cannot — it escapes newlines inside strings. Putting the guarantee and
    /// the terminator in the same function is what keeps them true together.
    ///
    /// Serialization of a `Reply` cannot fail (no map with non-string keys, no
    /// non-finite float that is not already `null`), but "cannot" is not
    /// "does not", so the fallback is an error reply rather than a panic in
    /// somebody's daemon.
    pub fn to_line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(mut s) => {
                s.push('\n');
                s
            }
            Err(e) => {
                let fallback = Reply::err(
                    self.id,
                    ApiError::new(
                        crate::error::ErrorCode::Internal,
                        format!("reply could not be serialized: {e}"),
                    ),
                );
                // This one is a plain literal, so it cannot recurse.
                serde_json::to_string(&fallback).unwrap_or_else(|_| {
                    format!(
                        r#"{{"id":{},"api_version":{API_VERSION},"outcome":{{"status":"error","error":{{"code":"internal","message":"unserializable"}}}}}}"#,
                        self.id
                    )
                }) + "\n"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_reads_as_a_method_and_params() {
        let json = serde_json::to_value(Request::GetBook { id: 3 }).unwrap();
        assert_eq!(json["method"], "get_book");
        assert_eq!(json["params"]["id"], 3);
    }

    /// A method with nothing to pass carries no `params` at all, so a client
    /// need not send an empty object to ask the simplest question there is.
    #[test]
    fn a_parameterless_request_is_just_its_name() {
        let json = serde_json::to_string(&Request::ApiVersion).unwrap();
        assert_eq!(json, r#"{"method":"api_version"}"#);
        assert_eq!(
            serde_json::from_str::<Request>(&json).unwrap(),
            Request::ApiVersion
        );
    }

    /// Defaults are the forward-compatibility lever, and the flags that carry
    /// them are all "do the safe thing": a `dry_run`/`new`/`overwrite` omitted
    /// must never be the destructive reading.
    #[test]
    fn an_omitted_flag_takes_the_safe_default() {
        let r: Request =
            serde_json::from_str(r#"{"method":"import_file","params":{"path":"/x.epub"}}"#)
                .unwrap();
        assert_eq!(
            r,
            Request::ImportFile {
                path: "/x.epub".into(),
                new: false
            }
        );
        let r: Request = serde_json::from_str(
            r#"{"method":"convert_ebook","params":{"input":"a.epub","output":"b.azw3"}}"#,
        )
        .unwrap();
        assert_eq!(
            r,
            Request::ConvertEbook {
                input: "a.epub".into(),
                output: "b.azw3".into(),
                overwrite: false
            }
        );
    }

    #[test]
    fn a_reply_round_trips() {
        let reply = Reply::ok(9, Response::Id(42));
        let line = reply.to_line();
        assert!(line.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<Reply>(line.trim_end()).unwrap(),
            reply
        );
    }

    /// The framing guarantee. A line-delimited transport is only safe while no
    /// payload can contain a bare newline — and note bodies contain them all
    /// day long.
    #[test]
    fn a_newline_in_the_payload_never_becomes_a_frame_break() {
        let reply = Reply::ok(1, Response::Text("line one\nline two\n".into()));
        let line = reply.to_line();
        assert_eq!(line.matches('\n').count(), 1, "{line}");
        assert_eq!(
            serde_json::from_str::<Reply>(line.trim_end()).unwrap(),
            reply
        );
    }

    #[test]
    fn an_error_reply_names_its_code() {
        let reply = Reply::err(
            2,
            ApiError::new(crate::error::ErrorCode::NotFound, "book id 3"),
        );
        let json: serde_json::Value = serde_json::from_str(reply.to_line().trim_end()).unwrap();
        assert_eq!(json["outcome"]["status"], "error");
        assert_eq!(json["outcome"]["error"]["code"], "not_found");
        assert_eq!(json["api_version"], API_VERSION);
    }
}
