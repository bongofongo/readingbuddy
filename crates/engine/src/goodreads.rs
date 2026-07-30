//! Goodreads CSV, in and out.
//!
//! The API died in December 2020, so the CSV is the interface — and it is the
//! better one anyway: it is a file the user owns, it carries the whole shelf,
//! and it works when Goodreads does not.
//!
//! Four things here are decisions rather than mechanics, and each is argued at
//! the place it happens:
//!
//! * **Excel armour.** `ISBN` and `ISBN13` arrive as `="0316769487"`, which is
//!   how a spreadsheet is stopped from eating the leading zero. It is stripped
//!   and then put through [`normalize_isbn`] like every other ISBN entering the
//!   system — see [`strip_armour`].
//! * **One matcher.** ISBN-13 → ISBN-10 → the same title+author scan a KOReader
//!   sidecar gets ([`crate::matching`], through [`koreader::scores_for`]), with
//!   the same `AUTO_MATCH` / `CANDIDATE_MIN` bands and the CSV's own `Author`
//!   column on the veto side. A second fuzzy matcher would be a second answer to
//!   "is this the book I already have", and the one that disagreed would be the
//!   one that made the duplicate.
//! * **`Exclusive Shelf` is readings, not a status column** — `read` is a
//!   finished reading, `currently-reading` an open one, and `to-read` is *a book
//!   with no reading at all*. That last one is why `to-read` needs no collection
//!   to live in.
//! * **`My Rating` is stored raw against the `goodreads` scale.** Never reversed
//!   through `rating_map` onto the user's own scale: the map is many-to-one, so
//!   its inverse is a guess, and the pair that has to survive is (value, scale).

use std::collections::HashMap;
use std::path::Path;

use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

use crate::book::{Book, normalize_isbn};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Severity};
use crate::error::{EngineError, Result};
use crate::koreader::{self, MatchCandidate};
use crate::matching::Query;
use crate::notes::{NewNoteInput, NoteKind};
use crate::storage::{STATUS_FINISHED, STATUS_READING, Storage};
use crate::{Engine, storage};

/// The `source` these rows are recorded under, in `external_ids`, `book_tags`
/// and `readings.source` alike. One string, so a later "forget everything
/// Goodreads told me" is one predicate.
pub const SOURCE: &str = "goodreads";

/// The seeded scale (migration `0009`): integers 0–5, no halves, identity map.
pub const SCALE: &str = "goodreads";

/// Goodreads writes dates as `YYYY/MM/DD`, and reads them back the same way.
const DATE: &[BorrowedFormatItem<'static>] = format_description!("[year]/[month]/[day]");

// ---- the row ---------------------------------------------------------------

/// Which of Goodreads' three exclusive shelves a row sits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shelf {
    Read,
    CurrentlyReading,
    ToRead,
    /// A shelf name Goodreads is not known to write. Carried rather than
    /// guessed at, and it changes no reading state — the same treatment
    /// `KoStatus::Other` gets.
    Other(String),
}

impl Shelf {
    fn parse(s: &str) -> Shelf {
        match s.trim() {
            "read" => Shelf::Read,
            "currently-reading" => Shelf::CurrentlyReading,
            "to-read" => Shelf::ToRead,
            other => Shelf::Other(other.to_string()),
        }
    }
}

/// One data row, reduced to the fields this import acts on.
#[derive(Debug, Clone, Default)]
pub struct GoodreadsRow {
    /// 1-based data row (the header is not counted), so a diagnostic can name
    /// the line the user has to go and look at.
    pub row: usize,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
    /// `None` means the cell was empty; `Some(0)` means Goodreads wrote a
    /// literal `0`, which is its encoding of *unrated*. **Neither is a rating**
    /// — and conflating them into `0.0` would store a real value on a 0–5 scale
    /// meaning "rated zero stars", which is not what either cell said.
    pub my_rating: Option<u8>,
    pub publisher: Option<String>,
    pub page_count: Option<i64>,
    pub publish_year: Option<i64>,
    pub date_read: Option<i64>,
    pub date_added: Option<i64>,
    pub shelves: Vec<String>,
    pub exclusive_shelf: Option<Shelf>,
    pub review: Option<String>,
    pub private_notes: Option<String>,
    /// Goodreads' `Read Count`, defaulted to 1 when the column is absent.
    pub read_count: i64,
}

impl GoodreadsRow {
    fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or("(untitled)")
    }

    /// How many readings this row describes.
    ///
    /// A row with **no** `Exclusive Shelf` — which is what our own export
    /// produces, since Goodreads' importer does not read that column — falls
    /// back to "a `Date Read` means it was read". Without that, exporting and
    /// re-importing would quietly drop every reading, and the round trip this
    /// module is judged by would be stable only in the trivial sense.
    fn wanted_readings(&self) -> i64 {
        match &self.exclusive_shelf {
            Some(Shelf::ToRead) => 0,
            Some(Shelf::CurrentlyReading) => 1,
            Some(Shelf::Read) => self.read_count.max(1),
            Some(Shelf::Other(_)) | None => i64::from(self.date_read.is_some()),
        }
    }

    fn is_currently_reading(&self) -> bool {
        self.exclusive_shelf == Some(Shelf::CurrentlyReading)
    }
}

// ---- parsing ---------------------------------------------------------------

/// Strip Goodreads' Excel armour: `="0316769487"` → `0316769487`.
///
/// The armour exists because a spreadsheet opening a bare `0316769487` helpfully
/// turns it into a number and eats the leading zero. The `=` makes it a formula
/// yielding a string instead. It is not CSV quoting — the csv reader hands the
/// field over with the `="…"` intact — so it has to come off here, before
/// [`normalize_isbn`] ever sees it.
fn strip_armour(raw: &str) -> Option<&str> {
    let s = raw.trim();
    let s = s.strip_prefix('=').unwrap_or(s);
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

fn cell(raw: &str) -> Option<String> {
    let s = raw.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// `YYYY/MM/DD` at UTC midnight. Also accepts `YYYY-MM-DD`, which is what a
/// spreadsheet hands back after a round trip through its own date formatting.
fn parse_date(raw: &str) -> Option<i64> {
    let s = raw.trim().replace('-', "/");
    time::Date::parse(&s, DATE)
        .ok()
        .map(|d| d.midnight().assume_utc().unix_timestamp())
}

fn format_date(unix: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|d| d.date().format(DATE).ok())
}

/// Read a Goodreads CSV.
///
/// Columns are found **by header name**, and a missing column is "the export
/// did not say" rather than an error: Goodreads' own importer documents a
/// smaller set than its exporter writes, third-party tools write subsets, and a
/// hard-coded column order would break on the first one of those. Only `Title`
/// is required, and only because a row with no title is a row with nothing to
/// match on.
pub fn parse_csv(path: &Path) -> Result<Vec<GoodreadsRow>> {
    let mut reader = csv::ReaderBuilder::new()
        // A review with a stray comma in it is a short row, and refusing the
        // whole file over one is not a service to anyone.
        .flexible(true)
        .from_path(path)
        .map_err(csv_err)?;

    let headers = reader.headers().map_err(csv_err)?.clone();
    let index: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_ascii_lowercase(), i))
        .collect();
    if !index.contains_key("title") {
        return Err(EngineError::InvalidInput(format!(
            "{} has no Title column — is it a Goodreads export?",
            path.display()
        )));
    }

    let mut out = Vec::new();
    for (n, record) in reader.records().enumerate() {
        let record = record.map_err(csv_err)?;
        // Several columns have two spellings in the wild: Goodreads' *exporter*
        // writes `My Review`, its *importer* documents `Review`, and our own
        // export writes the importer's name — so a file we wrote has to be a
        // file we can read.
        let get = |names: &[&str]| -> Option<&str> {
            names
                .iter()
                .find_map(|n| index.get(*n))
                .and_then(|i| record.get(*i))
                .map(str::trim)
        };
        let get = |name: &str| -> Option<&str> {
            match name {
                "my review" => get(&["my review", "review"]),
                "my rating" => get(&["my rating", "rating"]),
                _ => get(&[name]),
            }
        };

        let mut authors: Vec<String> = Vec::new();
        if let Some(a) = get("author").and_then(cell) {
            authors.push(a);
        }
        // Goodreads separates additional authors with ", " and appends roles in
        // brackets ("Deborah Smith (Translator)"). The role is dropped: we have
        // a `translators` column and no way to tell from here which is which
        // without parsing English.
        for extra in get("additional authors").unwrap_or("").split(',') {
            if let Some(a) = cell(extra)
                && !authors.contains(&a)
            {
                authors.push(a);
            }
        }

        // Both ISBN columns go through the same armour strip and the same
        // checksum validation. A Goodreads export carries plenty of junk in
        // these — `=""`, an ASIN, a 13-digit string that is not an ISBN — and
        // `normalize_isbn` returning None is the right answer to all of it.
        let isbn = |name: &str| -> Option<String> {
            get(name).and_then(strip_armour).and_then(normalize_isbn)
        };
        let (mut isbn_10, mut isbn_13) = (isbn("isbn"), isbn("isbn13"));
        // The columns are named, not trusted: an export with the two swapped
        // (they do exist) would otherwise write a 13 into `isbn_10`, which is a
        // UNIQUE column other lookups key on.
        if isbn_10.as_ref().is_some_and(|s| s.len() == 13) {
            isbn_13 = isbn_13.or_else(|| isbn_10.take());
        }
        if isbn_13.as_ref().is_some_and(|s| s.len() == 10) {
            isbn_10 = isbn_10.or_else(|| isbn_13.take());
        }

        out.push(GoodreadsRow {
            row: n + 1,
            external_id: get("book id").and_then(cell),
            title: get("title").and_then(cell),
            authors,
            isbn_10,
            isbn_13,
            my_rating: get("my rating")
                .and_then(cell)
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|r| *r <= 5),
            publisher: get("publisher").and_then(cell),
            page_count: get("number of pages")
                .and_then(cell)
                .and_then(|s| s.parse().ok()),
            publish_year: get("year published")
                .and_then(cell)
                .and_then(|s| s.parse().ok())
                .or_else(|| {
                    get("original publication year")
                        .and_then(cell)
                        .and_then(|s| s.parse().ok())
                }),
            date_read: get("date read").and_then(parse_date),
            date_added: get("date added").and_then(parse_date),
            shelves: get("bookshelves")
                .unwrap_or("")
                .split(',')
                .filter_map(cell)
                .collect(),
            exclusive_shelf: get("exclusive shelf")
                .and_then(cell)
                .map(|s| Shelf::parse(&s)),
            review: get("my review").and_then(cell),
            private_notes: get("private notes").and_then(cell),
            read_count: get("read count")
                .and_then(cell)
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
        });
    }
    Ok(out)
}

/// An unreadable file is [`EngineError::Io`] like any other; a malformed one is
/// the user's input, not ours. Keeping the two apart is what lets
/// `ErrorClass::from` classify it correctly without reading the message.
fn csv_err(e: csv::Error) -> EngineError {
    let rendered = e.to_string();
    match e.into_kind() {
        csv::ErrorKind::Io(io) => EngineError::Io(io),
        _ => EngineError::InvalidInput(format!("goodreads csv: {rendered}")),
    }
}

// ---- the report ------------------------------------------------------------

/// How a row found its book. Deliberately *not*
/// [`MatchMethod`](crate::MatchMethod), which is KOReader's vocabulary for how a
/// *sidecar* found one — the branches differ, and one enum covering both would
/// have to carry variants that are unreachable from either caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoodreadsMatch {
    /// An `external_ids` row already links this `Book Id` to a book. The only
    /// branch that is not a guess, and what makes re-importing the same export
    /// idempotent for books with no ISBN.
    ExternalId,
    Isbn,
    Title,
    /// Nothing matched and nothing was close, so the book was created from the
    /// row.
    New,
}

impl std::fmt::Display for GoodreadsMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GoodreadsMatch::ExternalId => write!(f, "goodreads id"),
            GoodreadsMatch::Isbn => write!(f, "isbn"),
            GoodreadsMatch::Title => write!(f, "title"),
            GoodreadsMatch::New => write!(f, "new"),
        }
    }
}

/// What happened to a row's prose — the same three answers for a review and for
/// the private notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOutcome {
    /// The row carried none.
    Absent,
    Written,
    /// Already here, byte for byte.
    Unchanged,
    /// Already here and *different*. Ours is kept and a diagnostic says so —
    /// the CSV is where the words came from, but the vault is where they live
    /// now, and an import silently overwriting the user's own edit is the one
    /// outcome that cannot be undone.
    KeptOurs,
}

#[derive(Debug)]
pub struct GoodreadsBookReport {
    /// `None` in a dry run for a book that does not exist yet.
    pub book_id: Option<i64>,
    pub title: String,
    pub matched_by: GoodreadsMatch,
    pub readings_added: usize,
    pub tags_added: usize,
    pub review: TextOutcome,
    pub private_notes: TextOutcome,
    /// The raw `My Rating` this import **wrote**, against the `goodreads`
    /// scale. `None` covers an empty cell, an explicit `0` (Goodreads for
    /// unrated), a rating already stored identically, and one the user has
    /// since given on their own scale.
    pub rating: Option<u8>,
}

/// A row we will not guess about.
///
/// Deliberately the same shape as
/// [`UnmatchedSidecar`](crate::koreader::UnmatchedSidecar): a title, an
/// identity the caller can act on, and the library books in the ambiguous band.
/// Unmatched has to be a *decision*, not a dead end — which is only true if the
/// report says what it probably is.
#[derive(Debug)]
pub struct UnmatchedRow {
    pub row: usize,
    pub title: String,
    pub authors: Vec<String>,
    pub external_id: Option<String>,
    pub candidates: Vec<MatchCandidate>,
}

#[derive(Debug, Default)]
pub struct GoodreadsReport {
    pub dry_run: bool,
    /// Data rows read from the file, whatever became of them.
    pub rows: usize,
    pub books: Vec<GoodreadsBookReport>,
    pub unmatched: Vec<UnmatchedRow>,
    pub warnings: Vec<Diagnostic>,
}

impl GoodreadsReport {
    pub fn created(&self) -> usize {
        self.books
            .iter()
            .filter(|b| b.matched_by == GoodreadsMatch::New)
            .count()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ImportOptions {
    pub dry_run: bool,
    /// Create a book for a row that landed in the candidate band instead of
    /// reporting it. The same escape hatch `ko pull --new` has, for the same
    /// reason: refusing is right by default and has to have a next move.
    pub create_ambiguous: bool,
}

// ---- import ----------------------------------------------------------------

/// Import a Goodreads CSV export.
///
/// Takes the whole [`Engine`] rather than a [`Storage`]: a review is a note, and
/// notes are a vault file plus a row plus an FTS entry plus its wikilink edges —
/// all of which `Engine::open_review` already gets right, including the
/// `idx_one_review` accretion that makes a second import find the first one's
/// note.
pub async fn import(engine: &Engine, path: &Path, opts: ImportOptions) -> Result<GoodreadsReport> {
    let rows = parse_csv(path)?;
    let mut report = GoodreadsReport {
        dry_run: opts.dry_run,
        rows: rows.len(),
        ..Default::default()
    };
    let storage = &engine.storage;
    let scale = storage
        .rating_scale_by_name(SCALE)
        .await?
        .ok_or_else(|| EngineError::NotFound(format!("the '{SCALE}' rating scale")))?;

    for row in &rows {
        if row.title.is_none() {
            report.warnings.push(row_skipped(
                row.row,
                "no Title, so there is nothing to match on",
            ));
            continue;
        }

        let matched = match_row(storage, row).await?;
        let (book_id, matched_by) = match matched {
            Matched::Book(id, how) => (Some(id), how),
            Matched::Ambiguous(candidates) if !opts.create_ambiguous => {
                report.unmatched.push(UnmatchedRow {
                    row: row.row,
                    title: row.display_title().to_string(),
                    authors: row.authors.clone(),
                    external_id: row.external_id.clone(),
                    candidates,
                });
                continue;
            }
            Matched::Ambiguous(_) | Matched::Nothing => (None, GoodreadsMatch::New),
        };

        if opts.dry_run {
            report
                .books
                .push(preview(storage, row, book_id, matched_by).await?);
            continue;
        }

        let book_id = match book_id {
            Some(id) => id,
            None => create_book(storage, row).await?,
        };
        report.books.push(
            apply(
                engine,
                row,
                book_id,
                matched_by,
                &scale,
                &mut report.warnings,
            )
            .await?,
        );
    }
    Ok(report)
}

enum Matched {
    Book(i64, GoodreadsMatch),
    /// Nothing matched, but the library holds something close enough that
    /// creating a book would probably be creating a duplicate.
    Ambiguous(Vec<MatchCandidate>),
    Nothing,
}

/// ISBN-13 → ISBN-10 → the shared title scan. In that order because it runs
/// from the identity we are surest of to the one we are least sure of, and stops
/// at the first that answers.
async fn match_row(storage: &Storage, row: &GoodreadsRow) -> Result<Matched> {
    if let Some(id) = &row.external_id
        && let Some(book_id) = storage.book_for_external_id(SOURCE, id).await?
    {
        return Ok(Matched::Book(book_id, GoodreadsMatch::ExternalId));
    }
    for isbn in [&row.isbn_13, &row.isbn_10].into_iter().flatten() {
        if let Some(book) = storage.find_book_by_isbn(isbn).await?
            && let Some(book_id) = book.id
        {
            return Ok(Matched::Book(book_id, GoodreadsMatch::Isbn));
        }
    }

    // One scan, both answers — see `calibre::match_book`.
    let scored =
        koreader::scores_for(storage, &Query::new(row.title.as_deref(), &row.authors)).await?;
    if let Some(s) = scored.first()
        && s.can_auto
        && let Some(book_id) = s.book.id
    {
        return Ok(Matched::Book(book_id, GoodreadsMatch::Title));
    }
    let candidates = koreader::band(scored);
    Ok(if candidates.is_empty() {
        Matched::Nothing
    } else {
        Matched::Ambiguous(candidates)
    })
}

async fn create_book(storage: &Storage, row: &GoodreadsRow) -> Result<i64> {
    let book_id = storage
        .upsert_book(&Book {
            title: row.title.clone(),
            authors: row.authors.clone(),
            publisher: row.publisher.clone(),
            publish_year: row.publish_year,
            page_count: row.page_count,
            isbn_10: row.isbn_10.clone(),
            isbn_13: row.isbn_13.clone(),
            ..Default::default()
        })
        .await?;
    // `Date Added` is when the *user* added it, which is exactly what
    // `books.created_at` means — and only for a book this import just created:
    // for one we already had, when it arrived here is ours to know and the CSV
    // has no standing to rewrite it.
    if let Some(added) = row.date_added {
        storage.set_book_created_at(book_id, added).await?;
    }
    Ok(book_id)
}

/// The read-only half, for `--dry-run`. It answers the same questions [`apply`]
/// does and writes nothing.
async fn preview(
    storage: &Storage,
    row: &GoodreadsRow,
    book_id: Option<i64>,
    matched_by: GoodreadsMatch,
) -> Result<GoodreadsBookReport> {
    let (readings_added, tags_added) = match book_id {
        None => (row.wanted_readings().max(0) as usize, row.shelves.len()),
        Some(id) => {
            let have = storage.readings_from_source(id, SOURCE).await?.len() as i64;
            let known: Vec<String> = storage
                .book_tags(id)
                .await?
                .into_iter()
                .filter(|t| t.source == SOURCE)
                .map(|t| t.tag)
                .collect();
            (
                (row.wanted_readings() - have).max(0) as usize,
                row.shelves
                    .iter()
                    .filter(|s| !known.contains(&slug_tag(s)))
                    .count(),
            )
        }
    };
    Ok(GoodreadsBookReport {
        book_id,
        title: row.display_title().to_string(),
        matched_by,
        readings_added,
        tags_added,
        review: match row.review {
            Some(_) => TextOutcome::Written,
            None => TextOutcome::Absent,
        },
        private_notes: match row.private_notes {
            Some(_) => TextOutcome::Written,
            None => TextOutcome::Absent,
        },
        rating: row.my_rating.filter(|r| *r > 0),
    })
}

async fn apply(
    engine: &Engine,
    row: &GoodreadsRow,
    book_id: i64,
    matched_by: GoodreadsMatch,
    scale: &storage::RatingScale,
    warnings: &mut Vec<Diagnostic>,
) -> Result<GoodreadsBookReport> {
    let storage = &engine.storage;

    if let Some(id) = &row.external_id {
        storage.link_external_id(SOURCE, id, book_id).await?;
    }
    let tags: Vec<(String, String)> = row
        .shelves
        .iter()
        .map(|s| (slug_tag(s), s.clone()))
        .collect();
    let tags_added = storage.add_book_tags(book_id, SOURCE, &tags).await?;

    let readings_added = reconcile_readings(storage, row, book_id, warnings).await?;
    let reading_id = storage
        .readings_from_source(book_id, SOURCE)
        .await?
        .last()
        .map(|r| r.id);

    // ---- review + rating ---------------------------------------------------
    //
    // A rating lives on a review, so a row that carries only `My Rating` opens
    // one too — empty, and none the worse for it. Without that the rating would
    // have nowhere to go and would vanish, which is what an export→import→export
    // round trip notices first.
    let mut review = TextOutcome::Absent;
    let mut rating = None;
    let rated = row.my_rating.is_some_and(|r| r > 0);
    if row.review.is_some() || rated {
        match reading_id {
            Some(reading_id) => {
                let note = engine.open_review(book_id, Some(reading_id)).await?;
                if let Some(text) = &row.review {
                    review = write_body(engine, note.id, text).await?;
                    if review == TextOutcome::KeptOurs {
                        warnings.push(kept_ours(
                            DiagnosticKind::GoodreadsReviewDiverged {
                                title: row.display_title().to_string(),
                            },
                            "the review here differs from the one in the CSV; yours was kept",
                        ));
                    }
                }
                rating = apply_rating(engine, note.id, row, scale, warnings).await?;
            }
            // A review anchors to a reading (`idx_one_review` is on
            // `reading_id`), and a `to-read` row has none by design. Inventing
            // one would make the library list say the user is reading a book
            // they have not opened, so the prose is kept as an ordinary note
            // instead — and the report says which, rather than the words
            // silently landing somewhere else.
            None => {
                if let Some(text) = &row.review {
                    let title = format!("Goodreads review: {}", row.display_title());
                    review = write_side_note(engine, book_id, &title, text).await?;
                }
                warnings.push(kept_ours(
                    DiagnosticKind::GoodreadsUnanchoredReview {
                        title: row.display_title().to_string(),
                    },
                    "a review needs a reading to anchor to; kept as a plain note",
                ));
            }
        }
    }

    // `Private Notes` are per-book prose that is neither a review nor a
    // reflection. Folding them into either would put words in the user's
    // reflection that they did not write there.
    let mut private_notes = TextOutcome::Absent;
    if let Some(text) = &row.private_notes {
        let title = format!("Goodreads notes: {}", row.display_title());
        private_notes = write_side_note(engine, book_id, &title, text).await?;
    }

    Ok(GoodreadsBookReport {
        book_id: Some(book_id),
        title: row.display_title().to_string(),
        matched_by,
        readings_added,
        tags_added,
        review,
        private_notes,
        rating,
    })
}

/// `Read Count` becomes readings, and this is where the one genuine collision in
/// this item lives.
///
/// The rule asked for is "only the most recent carries `Date Read`; the rest get
/// NULL dates" — and `finished_at IS NULL` is precisely what *open* means, under
/// a partial unique index (`idx_readings_one_open`) that permits exactly one open
/// reading per book. Three readings with NULL end dates are not representable,
/// and weakening that invariant to make an import fit would be the wrong trade
/// by a wide margin.
///
/// So: `started_at` is NULL on every one of them (Goodreads' CSV has no start
/// date at all, and inventing one is the thing this whole item refuses), and the
/// earlier readings are closed at the **same date the row already carries**.
/// That is a true upper bound rather than a fabrication — every earlier reading
/// did end before the last one did — and it is the only date the file contains.
/// Spacing them out to look plausible is what would be dishonest.
///
/// When the row carries no date at all, one reading is recorded open-shaped
/// (migration `0005`'s back-fill made the same call: "an open reading the user
/// can close" beats an invented date) and any remaining rereads are reported as
/// a diagnostic instead of vanishing.
async fn reconcile_readings(
    storage: &Storage,
    row: &GoodreadsRow,
    book_id: i64,
    warnings: &mut Vec<Diagnostic>,
) -> Result<usize> {
    let want = row.wanted_readings();
    let have = storage.readings_from_source(book_id, SOURCE).await?.len() as i64;
    let mut missing = (want - have).max(0);
    if missing == 0 {
        // The shelf may still have moved on the far side: a book that was
        // `currently-reading` when we last imported and is `read` now has the
        // right *number* of readings and the wrong end date.
        if let Some(when) = row.date_read
            && !row.is_currently_reading()
            && let Some(last) = storage.readings_from_source(book_id, SOURCE).await?.last()
            && last.finished_at.is_none()
        {
            storage
                .close_reading_at(last.id, when, STATUS_FINISHED)
                .await?;
        }
        return Ok(0);
    }

    if row.is_currently_reading() {
        // Already reading it is already reading it, whoever recorded that.
        if storage.active_reading(book_id).await?.is_some() {
            return Ok(0);
        }
        storage
            .record_reading(book_id, None, None, STATUS_READING, SOURCE)
            .await?;
        return Ok(1);
    }

    let stamp = row.date_read.or(row.date_added);
    let mut added = 0;
    match stamp {
        Some(when) => {
            while missing > 0 {
                storage
                    .record_reading(book_id, None, Some(when), STATUS_FINISHED, SOURCE)
                    .await?;
                added += 1;
                missing -= 1;
            }
        }
        None => {
            // Only one undated reading is representable, and only when the book
            // has no other open reading.
            if storage.active_reading(book_id).await?.is_none() {
                storage
                    .record_reading(book_id, None, None, STATUS_FINISHED, SOURCE)
                    .await?;
                added += 1;
                missing -= 1;
            }
            if missing > 0 {
                warnings.push(Diagnostic {
                    kind: DiagnosticKind::GoodreadsUndatableRereads {
                        title: row.display_title().to_string(),
                        dropped: missing as usize,
                    },
                    severity: Severity::Warning,
                    detail: format!(
                        "Read Count {} but no Date Read or Date Added, and a reading with no end \
                         date is an open one — {missing} not recorded",
                        row.read_count
                    ),
                });
            }
        }
    }
    Ok(added)
}

async fn apply_rating(
    engine: &Engine,
    note_id: i64,
    row: &GoodreadsRow,
    scale: &storage::RatingScale,
    warnings: &mut Vec<Diagnostic>,
) -> Result<Option<u8>> {
    // 0 is Goodreads' encoding of *unrated*, and an empty cell said nothing at
    // all. Neither is a rating, and storing either as `0.0` would put a real
    // "zero stars" on the user's shelf.
    let Some(value) = row.my_rating.filter(|r| *r > 0) else {
        return Ok(None);
    };
    // Ours to refresh only while it is still ours. A rating the user has given
    // on their own scale is a different number on a different axis, and the map
    // is many-to-one so it cannot even be compared.
    if let Some(existing) = engine.storage.review_rating(note_id).await? {
        if existing.scale.name != scale.name {
            warnings.push(kept_ours(
                DiagnosticKind::GoodreadsRatingDiverged {
                    title: row.display_title().to_string(),
                },
                &format!(
                    "already rated {} on the '{}' scale; the CSV's {value} was not written",
                    existing.value, existing.scale.name
                ),
            ));
            return Ok(None);
        }
        // Already exactly this. Reported as `None` rather than `Some(value)`
        // because the field is what the *import changed*, and a re-import that
        // announced "rated 5" every time would read as work it did not do.
        if existing.value == f64::from(value) {
            return Ok(None);
        }
    }
    engine
        .storage
        .set_review_rating(note_id, scale, f64::from(value))
        .await?;
    Ok(Some(value))
}

/// Write a note body, unless the note already says something else.
async fn write_body(engine: &Engine, note_id: i64, text: &str) -> Result<TextOutcome> {
    let note = engine
        .storage
        .get_note(note_id)
        .await?
        .ok_or_else(|| EngineError::NotFound(format!("note id {note_id}")))?;
    let body = engine.note_body(&note)?;
    if body.trim() == text.trim() {
        return Ok(TextOutcome::Unchanged);
    }
    if !body.trim().is_empty() {
        return Ok(TextOutcome::KeptOurs);
    }
    engine.update_note_body(&note, text).await?;
    Ok(TextOutcome::Written)
}

/// A plain note titled for its origin, found again by that title on a re-import.
async fn write_side_note(
    engine: &Engine,
    book_id: i64,
    title: &str,
    text: &str,
) -> Result<TextOutcome> {
    if let Some(existing) = engine
        .storage
        .list_notes(Some(book_id))
        .await?
        .into_iter()
        .find(|n| n.title == title)
    {
        return write_body(engine, existing.id, text).await;
    }
    engine
        .create_note(NewNoteInput {
            book_id: Some(book_id),
            kind: NoteKind::Note,
            title: Some(title.to_string()),
            body: text.to_string(),
            ..Default::default()
        })
        .await?;
    Ok(TextOutcome::Written)
}

/// Our normalization of a shelf name. The raw value is stored beside it, which
/// is what makes this safe to change later.
fn slug_tag(shelf: &str) -> String {
    shelf.trim().to_lowercase().replace(' ', "-")
}

fn row_skipped(row: usize, why: &str) -> Diagnostic {
    Diagnostic {
        kind: DiagnosticKind::GoodreadsRowSkipped { row },
        severity: Severity::Warning,
        detail: why.to_string(),
    }
}

fn kept_ours(kind: DiagnosticKind, detail: &str) -> Diagnostic {
    Diagnostic {
        kind,
        severity: Severity::Warning,
        detail: detail.to_string(),
    }
}

// ---- export ----------------------------------------------------------------

/// The columns Goodreads' *importer* reads. Its exporter writes twenty-four;
/// writing the other sixteen would be writing fields nothing on the far side
/// looks at.
const EXPORT_COLUMNS: [&str; 8] = [
    "Title",
    "Author",
    "ISBN",
    "My Rating",
    "Date Read",
    "Date Added",
    "Bookshelves",
    "Review",
];

/// One row of the export, before it is quoted.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportRow {
    pub title: String,
    pub author: String,
    pub isbn: Option<String>,
    pub my_rating: Option<u8>,
    pub date_read: Option<String>,
    pub date_added: Option<String>,
    pub bookshelves: String,
    pub review: String,
}

/// Serialize rows as a Goodreads-importable CSV.
///
/// ISBNs go back out **armoured** (`="0316769487"`), the way Goodreads writes
/// them, so a spreadsheet cannot eat the leading zero on the way to the import
/// form — and so our own reader meets, in its own output, exactly the shape it
/// has to survive from theirs.
pub fn write_csv(rows: &[ExportRow]) -> String {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(EXPORT_COLUMNS).expect("in-memory write");
    for r in rows {
        w.write_record([
            r.title.as_str(),
            r.author.as_str(),
            &r.isbn
                .as_ref()
                .map(|i| format!("=\"{i}\""))
                .unwrap_or_default(),
            &r.my_rating.map(|v| v.to_string()).unwrap_or_default(),
            r.date_read.as_deref().unwrap_or(""),
            r.date_added.as_deref().unwrap_or(""),
            r.bookshelves.as_str(),
            r.review.as_str(),
        ])
        .expect("in-memory write");
    }
    String::from_utf8(w.into_inner().expect("flush")).expect("csv is utf-8")
}

/// How many books an export payload describes.
///
/// Not `lines().count() - 1`: a review with a newline in it is one record over
/// two lines, and counting lines reported six books for five the first time a
/// multi-line review went through. Counting records is the csv reader's job, and
/// it is already here.
pub fn row_count(csv: &str) -> usize {
    csv::Reader::from_reader(csv.as_bytes()).records().count()
}

/// Build the export. Returns the payload and every honest failure along the way,
/// mirroring `export_flashcards`: the caller owns the file.
///
/// Two things are reported rather than papered over:
///
/// * **An unmapped rating skips its row.** Goodreads takes integers 0–5 with no
///   halves, and a rounded star is precisely the failure the explicit lookup
///   table exists to refuse — formulas are always wrong at the ends.
/// * **Rereads lose all but the most recent.** Goodreads' importable CSV has no
///   read-count column, so a book read three times exports as one row. Silent
///   truncation would look like data loss on the far side.
pub async fn export(engine: &Engine) -> Result<(String, Vec<Diagnostic>)> {
    let storage = &engine.storage;
    let mut warnings = Vec::new();
    let mut rows = Vec::new();

    for book in storage
        .list_books(i64::MAX, storage::BookSort::Title)
        .await?
    {
        let Some(book_id) = book.id else { continue };
        let readings = storage.list_readings(book_id).await?;
        if readings.len() > 1 {
            warnings.push(Diagnostic {
                kind: DiagnosticKind::GoodreadsRereadsDropped {
                    title: book.display_title().to_string(),
                    dropped: readings.len() - 1,
                },
                severity: Severity::Warning,
                detail: format!(
                    "read {} times; Goodreads' importable CSV has no read-count column, so only \
                     the most recent is exported",
                    readings.len()
                ),
            });
        }
        let latest = readings.last();

        // The review of *that* reading, so a reread's review is the one that
        // goes with the date beside it.
        let review_note = match latest {
            Some(r) => {
                storage
                    .note_for_reading(r.id, NoteKind::Review.as_str())
                    .await?
            }
            None => None,
        };
        let my_rating = match &review_note {
            Some(note) => match engine.goodreads_rating(note.id).await {
                Ok(r) => r,
                Err(EngineError::UnmappedRating { value, scale }) => {
                    warnings.push(Diagnostic {
                        kind: DiagnosticKind::GoodreadsRatingUnmapped {
                            title: book.display_title().to_string(),
                        },
                        severity: Severity::Warning,
                        detail: format!(
                            "{value} on the '{scale}' scale has no Goodreads mapping, so the row \
                             was skipped (`readingbuddy rating map {value} <0-5>`)"
                        ),
                    });
                    continue;
                }
                Err(e) => return Err(e),
            },
            None => None,
        };
        let review = match &review_note {
            Some(note) => engine.note_body(note)?,
            None => String::new(),
        };

        let bookshelves = storage
            .book_tags(book_id)
            .await?
            .into_iter()
            .map(|t| t.raw.unwrap_or(t.tag))
            .collect::<Vec<_>>()
            .join(", ");

        rows.push(ExportRow {
            title: book.display_title().to_string(),
            author: book.authors.first().cloned().unwrap_or_default(),
            isbn: book.any_isbn().map(str::to_string),
            my_rating,
            date_read: latest.and_then(|r| r.finished_at).and_then(format_date),
            date_added: book
                .created_at
                .map(|d| d.unix_timestamp())
                .and_then(format_date),
            bookshelves,
            review,
        });
    }

    // Ordered by what the *data* says, never by row id: an export re-imported
    // into an empty library gets fresh ids, and a round trip that reordered its
    // own rows would fail the stability property for no reason anyone could act
    // on.
    rows.sort_by(|a, b| {
        (a.title.to_lowercase(), &a.author, &a.isbn).cmp(&(
            b.title.to_lowercase(),
            &b.author,
            &b.isbn,
        ))
    });
    Ok((write_csv(&rows), warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_armour_comes_off_and_an_empty_cell_stays_empty() {
        assert_eq!(strip_armour("=\"0316769487\""), Some("0316769487"));
        assert_eq!(strip_armour("=\"\""), None);
        assert_eq!(strip_armour("0316769487"), Some("0316769487"));
        assert_eq!(
            strip_armour("  =\"9780316769488\"  "),
            Some("9780316769488")
        );
        assert_eq!(strip_armour(""), None);
        assert_eq!(strip_armour("="), None);
    }

    #[test]
    fn dates_round_trip_through_goodreads_own_format() {
        let read = parse_date("2019/03/12").expect("parses");
        assert_eq!(format_date(read).as_deref(), Some("2019/03/12"));
        // A spreadsheet hands them back with dashes.
        assert_eq!(parse_date("2019-03-12"), Some(read));
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("March 2019"), None);
    }

    #[test]
    fn the_three_shelves_are_named_and_the_rest_are_carried() {
        assert_eq!(Shelf::parse("read"), Shelf::Read);
        assert_eq!(Shelf::parse("currently-reading"), Shelf::CurrentlyReading);
        assert_eq!(Shelf::parse("to-read"), Shelf::ToRead);
        assert_eq!(Shelf::parse("dnf"), Shelf::Other("dnf".into()));
    }

    /// `to-read` is a book with no reading at all — the honest encoding, and the
    /// reason it needs no collection to live in.
    #[test]
    fn a_shelf_decides_how_many_readings_a_row_describes() {
        let row = |shelf: Option<Shelf>, count: i64, read: Option<i64>| GoodreadsRow {
            exclusive_shelf: shelf,
            read_count: count,
            date_read: read,
            ..Default::default()
        };
        assert_eq!(row(Some(Shelf::ToRead), 0, None).wanted_readings(), 0);
        assert_eq!(
            row(Some(Shelf::CurrentlyReading), 0, None).wanted_readings(),
            1
        );
        assert_eq!(row(Some(Shelf::Read), 3, Some(1)).wanted_readings(), 3);
        // A `read` row with a zero count is still a reading.
        assert_eq!(row(Some(Shelf::Read), 0, None).wanted_readings(), 1);
        // Our own export carries no Exclusive Shelf; a Date Read is what says
        // it was read.
        assert_eq!(row(None, 1, Some(1)).wanted_readings(), 1);
        assert_eq!(row(None, 1, None).wanted_readings(), 0);
    }

    #[test]
    fn the_export_writes_only_the_columns_goodreads_reads() {
        let csv = write_csv(&[ExportRow {
            title: "Pachinko".into(),
            author: "Min Jin Lee".into(),
            isbn: Some("9781455563937".into()),
            my_rating: Some(5),
            date_read: Some("2019/03/12".into()),
            date_added: Some("2018/12/01".into()),
            bookshelves: "korean-lit, favourites".into(),
            review: "A river of a book, and a comma, and a \"quote\".".into(),
        }]);
        let mut lines = csv.lines();
        assert_eq!(
            lines.next().unwrap(),
            "Title,Author,ISBN,My Rating,Date Read,Date Added,Bookshelves,Review"
        );
        let row = lines.next().unwrap();
        // Armoured *and* correctly quoted. Goodreads writes `="…"` as a bare
        // field, which is not strictly valid CSV; we quote it properly, and our
        // reader accepts both — `a_written_row_reads_back_as_itself` is where
        // that is actually asserted rather than eyeballed here.
        assert!(row.contains(r#""=""9781455563937""""#), "armoured: {row}");
        assert!(row.contains("\"korean-lit, favourites\""));
        assert!(row.contains("\"\"quote\"\""), "quotes doubled: {row}");
    }

    /// The writer and the reader are two halves of one format, and this is the
    /// only test that puts them together without a database in the way: a
    /// comma, a double quote, a newline and an armoured ISBN survive the trip.
    #[test]
    fn a_written_row_reads_back_as_itself() {
        let csv = write_csv(&[ExportRow {
            title: "Pachinko, and \"other\" stories".into(),
            author: "Min Jin Lee".into(),
            isbn: Some("0316769487".into()),
            my_rating: Some(4),
            date_read: Some("2019/03/12".into()),
            date_added: Some("2018/12/01".into()),
            bookshelves: "korean-lit, favourites".into(),
            review: "First line.\nSecond line.".into(),
        }]);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("out.csv");
        std::fs::write(&path, &csv).unwrap();

        let rows = parse_csv(&path).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.title.as_deref(), Some("Pachinko, and \"other\" stories"));
        assert_eq!(r.isbn_10.as_deref(), Some("0316769487"));
        assert_eq!(r.my_rating, Some(4));
        assert_eq!(r.date_read, parse_date("2019/03/12"));
        assert_eq!(r.shelves, vec!["korean-lit", "favourites"]);
        assert_eq!(r.review.as_deref(), Some("First line.\nSecond line."));
    }
}
