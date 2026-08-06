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
/// the number can stay still through ordinary growth. Items 18, 19, 22 and the
/// surfacing item all grew this vocabulary without moving it.
///
/// **2 — item 34 removed `SearchNotes` and `Response::NoteHits`.** Their
/// replacement, [`Request::SearchMarks`], answers the same question and more of
/// it, so the two could have stood side by side and the number could have
/// stayed at 1. Keeping both was rejected: a client able to ask for notes alone
/// and highlights alone has to merge two rankings, has no rule for doing it,
/// and will merge them by source order — which is exactly the failure the
/// unified request exists to remove, so a second door makes the guarantee
/// optional. That is a removal, and a removal is what this number is for. It
/// costs one client (`rb notes --search`, updated here) and it can never be
/// done as cheaply again.
pub const API_VERSION: u32 = 2;

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
    /// One page of the library (item 18).
    ///
    /// `limit` and `sort` are what this method has always taken and mean exactly
    /// what they did; `offset` and `filter` are additive and an omitted one is
    /// the old behaviour, which is why this grew rather than being replaced and
    /// why [`API_VERSION`] does not move.
    ///
    /// **`limit` selects along the sort key** — the first `limit` books *by that
    /// key*, not `limit` arbitrary books shown in that order. See
    /// [`readingbuddy::BookSort`].
    ListBooks {
        /// Negative is no limit.
        limit: i64,
        #[serde(default)]
        sort: BookSortDto,
        /// Rows to skip. Pagination is an **offset**, not a cursor — two of the
        /// five sorts have no cursor key that exists in the database, and
        /// `docs/decisions.md` entry 18 has the argument.
        #[serde(default)]
        offset: i64,
        /// Absent is every book.
        #[serde(default)]
        filter: Option<BookFilterDto>,
    },
    /// How many books match — the number a shelf needs *before* it needs the
    /// rows.
    ///
    /// Its own method rather than a field beside the page, because a count is a
    /// property of the **filter** and a page is not: a shelf asks this once and
    /// pages many times, and bundling would make every scroll pay for a scan of
    /// the whole matching set. The clause is shared in the engine, which is
    /// where sharing it actually guarantees the two agree.
    CountBooks {
        #[serde(default)]
        filter: Option<BookFilterDto>,
    },
    /// What is behind each of these books — highlights, notes, owned files — in
    /// one call for a whole page (item 18).
    ///
    /// The detail screen makes four calls for one book; a list of eight hundred
    /// cannot, which is why no list could show this before. One reply row per id
    /// **in the order asked**, zeros included.
    BookSummaries {
        book_ids: Vec<i64>,
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

    // ---- moments ----
    /// What is worth noticing and has not been shown yet, newest first
    /// (item 23).
    ///
    /// **Poll this.** There is no push channel in this protocol — every reply
    /// carries the id it answers — and a moment stream would reopen the
    /// argument the mount watcher was deliberately left outside this
    /// vocabulary to avoid (see the module doc). A client asks on launch and
    /// after a write that could mint one.
    ///
    /// `limit` is absent for everything. It takes from the **newest** end, and
    /// it is the only lever a client gets: there is no count of what is
    /// pending, here or anywhere, because that is a badge.
    PendingMoments {
        #[serde(default)]
        limit: Option<i64>,
    },
    /// Record that a moment was surfaced, so the ceremony does not replay.
    ///
    /// **Idempotent** — acknowledging twice, or from two clients, writes one
    /// row and keeps the first time. `id` is a `MomentDto::id` handed straight
    /// back; a string that names no kind this build knows is an
    /// [`crate::error::ErrorCode::InvalidInput`] rather than a silent row.
    AcknowledgeMoment {
        id: String,
    },

    // ---- notes ----
    CreateNote {
        note: NewNoteDto,
    },
    /// Notes, newest first, for one book, for one reading, or for the whole
    /// vault.
    ///
    /// `limit` selects along `created_at`. **Absent is every note**, which is
    /// what this method has always done and is deliberately still reachable: a
    /// client walking the whole graph needs it, and a default cap would turn a
    /// correctness pass into a silently truncated one. A screen passes a number.
    ///
    /// **`reading_id` is item 40's addition** and is `#[serde(default)]`, so a
    /// client that never heard of it sends the same bytes it always did and
    /// [`API_VERSION`] does not move. It exists for `search_marks`' reason in a
    /// second place: `NoteDto` carries `reading_id`, so a client is one
    /// `filter` away from doing this above the seam — and above the seam the
    /// limit has already cut the *book's* twelve most recent notes, so a card
    /// for an older read shows however few of them happened to be its, or
    /// nothing at all.
    ///
    /// The two ids are **alternatives, not a conjunction**. Sending both is an
    /// [`crate::error::ErrorCode::InvalidInput`] rather than a preference for
    /// one of them: a reading belongs to exactly one book, so the pair is
    /// either redundant or a contradiction, and a contradiction's honest answer
    /// is an empty list that no client can tell from an empty vault.
    ListNotes {
        #[serde(default)]
        book_id: Option<i64>,
        #[serde(default)]
        reading_id: Option<i64>,
        #[serde(default)]
        limit: Option<i64>,
    },
    /// Notes and highlights matching one query, as **one ranked list**.
    ///
    /// This replaced `SearchNotes`, which is the change that moved
    /// [`API_VERSION`] to 2, and the replacement rather than the addition is
    /// the decision. Leaving both doors up would leave a client able to ask for
    /// notes and highlights separately and then interleave them itself — which
    /// it cannot do honestly, because it has no rule for whether a note
    /// outranks a highlight and would fall back to source order. Ranking once,
    /// below the seam, is the whole content of this method, and a second door
    /// past it makes that guarantee optional.
    ///
    /// `source` narrows which indexes are asked; **absent is both**, and it
    /// never changes the order. `limit` is the length of the answer. An empty
    /// query is no hits and no error.
    ///
    /// **`book_id` is item 40's addition** — absent is the whole library, and
    /// it is `#[serde(default)]`, so this is additive and [`API_VERSION`] does
    /// not move. It narrows *what is in* the two lists, in `source`'s own
    /// idiom, and like `source` it never changes how they are ordered.
    ///
    /// It is here because it **cannot** be done above the seam. `limit` cuts
    /// the global ranked list, so a client searching the library and then
    /// keeping one book's hits gets an empty answer whenever the top `limit`
    /// marks live in other books — which, at four hundred books, is most
    /// queries. `SearchHitDto` has always carried enough to write that filter,
    /// which is exactly why the request had to grow the parameter.
    SearchMarks {
        query: String,
        #[serde(default)]
        source: Option<SearchSourceDto>,
        #[serde(default)]
        book_id: Option<i64>,
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

    /// A number of rows. **Not [`Response::Id`]** — an id identifies and a count
    /// measures, and a shape that meant both would be a shape a client has to
    /// know the question to read.
    Count(i64),

    Book(Option<BookDto>),
    Books(Vec<BookDto>),
    BookSummaries(Vec<BookSummaryDto>),
    BookTags(Vec<BookTagDto>),
    OpenReadings(Vec<OpenReadingDto>),
    MergeReport(MergeReportDto),

    Reading(Option<ReadingDto>),
    Readings(Vec<ReadingDto>),

    Highlights(Vec<HighlightDto>),

    SearchOutcome(SearchOutcomeDto),

    Note(Option<NoteDto>),
    Notes(Vec<NoteDto>),
    SearchHits(Vec<SearchHitDto>),
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
    /// Newest first. Its **length is not a fact the protocol states** — there
    /// is no count field here and no count endpoint beside it.
    Moments(Vec<MomentDto>),
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
