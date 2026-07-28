//! Calibre, through its command line.
//!
//! Two tiers, in `docs/decisions.md`'s order of importance:
//!
//! * **(i) `ebook-convert`** — format conversion. Calibre infers both formats
//!   from the file extensions, which is the whole interface.
//! * **(ii) `calibredb list --for-machine`** — the library the user has already
//!   curated, as JSON. No scraping, and the single biggest onboarding win
//!   available: empty shelf to full shelf without typing one ISBN.
//!
//! **Feature-detected, never a hard dependency.** Present → the features work;
//! absent → they are not there, and nothing anywhere asks the user to install or
//! configure calibre. Detection is [`Calibre::detect`], run **once per run** and
//! held on the [`Engine`](crate::Engine): probing `PATH` once per book on a
//! library import would be a syscall sweep per book for an answer that cannot
//! change mid-run.
//!
//! **Shelling out to a GPL-3 binary is not linking**, so there is no licence
//! contamination here — which makes this strictly better than the in-process
//! `epub =2.1.4` the workspace already carries.
//!
//! Four things below are decisions rather than mechanics, and three of them were
//! found by running calibre 7.26 rather than by reading about it.
//!
//! * **`authors` is a `&`-joined string, not a list.** `--for-machine` emits
//!   `"Min Jin Lee & Deborah Smith"` where `tags`, `languages` and `formats` all
//!   come back as JSON arrays. [`StringOrList`] accepts either, because the one
//!   thing worse than guessing the shape is guessing it right for one version.
//! * **`pubdate` is `0101-01-01T00:00:00+00:00` when there is no publication
//!   date.** That is calibre's `UNDEFINED_DATE` sentinel, and taken at face value
//!   it gives every undated book a `publish_year` of 101. [`year_of`] filters it.
//! * **A `--library` path that does not exist is silently *created*.**
//!   `calibredb --with-library /typo list` writes a `metadata.db` and a
//!   `.calnotes/` there and reports `[]` with exit 0 — so a mistyped path looks
//!   exactly like an empty library, and leaves a new calibre library on the
//!   user's disk on the way. [`library_root`] refuses a directory with no
//!   `metadata.db` **before** the binary is ever run. This is the one place the
//!   read-only-sounding half of this module could write to the filesystem.
//! * **Calibre's rating is not imported.** `list` reports 0–10 half-stars, and a
//!   rating in this system lives on a *review*, which anchors to a *reading* —
//!   which calibre knows nothing about. Importing one would mean fabricating a
//!   reading to hang it from, and then mapping a number through the explicit
//!   lookup table `docs/decisions.md` requires ratings to go through. Both are
//!   worse than leaving the rating where its origin keeps it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::book::{Book, normalize_isbn};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Severity};
use crate::error::{EngineError, Result};
use crate::koreader::{self, MatchCandidate};
use crate::providers::normalize_language;
use crate::storage::{LinkedBy, Storage};
use crate::{Engine, partial_md5};

/// The `source` a calibre row is recorded under, in `external_ids` and
/// `book_tags` alike.
pub const SOURCE: &str = "calibre";

/// The two binaries this module knows how to drive.
const CONVERT: &str = "ebook-convert";
const CALIBREDB: &str = "calibredb";

/// Where calibre installs itself when it is not on `PATH`.
///
/// macOS is the case that matters: `calibre.app` puts its command line tools
/// inside the bundle and adds nothing to `PATH`, so a perfectly ordinary install
/// is invisible to a `PATH` probe alone. Looking here is feature *detection* —
/// it is not the same thing as asking the user to configure something.
const KNOWN_DIRS: [&str; 3] = [
    "/Applications/calibre.app/Contents/MacOS",
    "/opt/calibre",
    "/usr/lib/calibre",
];

// ---- detection --------------------------------------------------------------

/// Which calibre tools this machine has, resolved once.
///
/// Two separate `Option`s rather than one "calibre is installed" flag: the tiers
/// are independent features, and a partial install (or a `PATH` that carries one
/// wrapper and not the other) must degrade to *the half that works* rather than
/// to nothing.
#[derive(Debug, Clone, Default)]
pub struct Calibre {
    convert: Option<PathBuf>,
    calibredb: Option<PathBuf>,
}

impl Calibre {
    /// Probe for the tools. `extra` is searched first, and exists so a test can
    /// point at a directory of fake binaries without touching the process
    /// environment — `PATH` is global and mutating it from a test is a data race
    /// with every other test in the binary.
    pub fn detect(extra: Option<&Path>) -> Calibre {
        Calibre {
            convert: find_tool(CONVERT, extra),
            calibredb: find_tool(CALIBREDB, extra),
        }
    }

    /// Construct from explicit paths. Tests use it; so would a future settings
    /// screen, if one ever turns out to be needed.
    pub fn at(convert: Option<PathBuf>, calibredb: Option<PathBuf>) -> Calibre {
        Calibre { convert, calibredb }
    }

    /// Tier (i) is available.
    pub fn can_convert(&self) -> bool {
        self.convert.is_some()
    }

    /// Tier (ii) is available.
    pub fn can_read_library(&self) -> bool {
        self.calibredb.is_some()
    }

    pub fn convert_path(&self) -> Option<&Path> {
        self.convert.as_deref()
    }

    pub fn calibredb_path(&self) -> Option<&Path> {
        self.calibredb.as_deref()
    }

    fn require(tool: &str, found: Option<&Path>) -> Result<PathBuf> {
        found
            .map(Path::to_path_buf)
            .ok_or_else(|| EngineError::CalibreMissing {
                tool: tool.to_string(),
            })
    }
}

/// `PATH`, then the known install directories. Executability is checked, not
/// assumed: a *directory* called `calibredb` on `PATH` is not a program, and
/// finding one would turn "calibre is absent" into "calibre fails on every call".
fn find_tool(name: &str, extra: Option<&Path>) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    extra
        .map(Path::to_path_buf)
        .into_iter()
        .chain(path_dirs)
        .chain(KNOWN_DIRS.iter().map(PathBuf::from))
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

// ---- tier (i): conversion ---------------------------------------------------

/// The argv for a conversion, as a pure function so a test can assert on the
/// command line without a process.
///
/// Deliberately bare. Calibre's option set is enormous (`--output-profile`,
/// `--embed-all-fonts`, per-format groups) and every one of those is a decision
/// about somebody's book that we have no standing to make by default; the
/// formats come from the two extensions and nothing else is sent.
pub fn convert_argv(input: &Path, output: &Path) -> Vec<String> {
    vec![input.display().to_string(), output.display().to_string()]
}

/// Convert a book from one format to another. Returns the path written.
///
/// **Refuses to overwrite.** `ebook-convert` will happily replace an existing
/// output file, and the caller here is a command line where the output path is
/// typed by hand; the caller that means it passes `overwrite`. Losing the file
/// is the one outcome with no undo, and the guard costs a `stat`.
pub async fn convert(
    calibre: &Calibre,
    input: &Path,
    output: &Path,
    overwrite: bool,
) -> Result<PathBuf> {
    let bin = Calibre::require(CONVERT, calibre.convert_path())?;
    if !input.is_file() {
        return Err(EngineError::InvalidInput(format!(
            "{} is not a file to convert",
            input.display()
        )));
    }
    // Calibre infers the target format from the extension alone, so an output
    // path without one produces an error from inside calibre that reads as a
    // bug in us. Refusing here says the actual thing.
    if output.extension().is_none() {
        return Err(EngineError::InvalidInput(format!(
            "{} has no extension, and that is what calibre reads the output format from",
            output.display()
        )));
    }
    if output.exists() && !overwrite {
        return Err(EngineError::InvalidInput(format!(
            "{} already exists",
            output.display()
        )));
    }

    run(&bin, CONVERT, &convert_argv(input, output)).await?;
    if !output.is_file() {
        return Err(EngineError::Calibre {
            tool: CONVERT.to_string(),
            message: format!("reported success but wrote no {}", output.display()),
        });
    }
    Ok(output.to_path_buf())
}

// ---- tier (ii): the library -------------------------------------------------

/// The argv for a library listing.
///
/// `--fields all` rather than a named subset: the fields we do not read today
/// cost one JSON parse and are the difference between adding a field here and
/// discovering that the *shape* of the output changes with the field list.
pub fn list_argv(library: Option<&Path>) -> Vec<String> {
    let mut argv = Vec::new();
    if let Some(lib) = library {
        argv.push("--with-library".to_string());
        argv.push(lib.display().to_string());
    }
    argv.push("list".to_string());
    argv.push("--for-machine".to_string());
    argv.push("--fields".to_string());
    argv.push("all".to_string());
    argv
}

/// Check a `--library` path *before* handing it to calibredb.
///
/// `calibredb --with-library /typo list` does not fail: it **creates** a library
/// there — `metadata.db`, `.calnotes/` and all — then reports `[]` with exit 0.
/// So without this, a mistyped path is indistinguishable from an empty library
/// and leaves a new calibre library on the user's disk on the way past. Verified
/// against calibre 7.26.
fn library_root(library: Option<&Path>) -> Result<Option<&Path>> {
    let Some(lib) = library else { return Ok(None) };
    if !lib.join("metadata.db").is_file() {
        return Err(EngineError::InvalidInput(format!(
            "{} has no metadata.db, so it is not a calibre library (calibredb would create one \
             there rather than say so)",
            lib.display()
        )));
    }
    Ok(Some(lib))
}

/// One book, as calibre describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CalibreBook {
    /// Calibre's per-library row id. Displayed, never keyed on — see `uuid`.
    pub calibre_id: i64,
    /// Calibre's stable identity, and what `external_ids` is keyed on. **Not
    /// the id**: ids are per-library and are reused after a delete, so id 4 in
    /// one library and id 4 in another are different books, and `external_ids`
    /// has no library column to tell them apart.
    pub uuid: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub publish_year: Option<i64>,
    pub language: Option<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub cover: Option<PathBuf>,
    /// Every format calibre holds, as absolute paths. All of them, not just the
    /// epub: the user may well be reading the azw3 on the device, and it is the
    /// file that was opened whose `partial_md5` the sidecar carries.
    pub formats: Vec<PathBuf>,
    pub series: Option<String>,
    /// Calibre's `timestamp` — when the book was added to *that* library.
    pub added: Option<i64>,
}

impl CalibreBook {
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or("(untitled)")
    }
}

/// Calibre emits `authors` as a `&`-joined string and `tags`/`languages`/
/// `formats` as arrays. Accepting both shapes everywhere costs one enum and
/// removes a whole class of "it worked on my calibre".
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrList {
    One(String),
    Many(Vec<String>),
}

impl StringOrList {
    /// `&` is calibre's author separator; `,` is what a hand-edited field ends
    /// up using often enough to be worth splitting on too.
    fn names(&self) -> Vec<String> {
        match self {
            StringOrList::One(s) => s
                .split('&')
                .flat_map(|part| part.split(','))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            StringOrList::Many(v) => v
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    /// Verbatim, for fields that are values rather than lists of names — a tag
    /// with a comma in it is one tag.
    fn values(&self) -> Vec<String> {
        match self {
            StringOrList::One(s) => s
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            StringOrList::Many(v) => v
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    fn first(&self) -> Option<String> {
        self.values().into_iter().next()
    }
}

/// The wire shape. Every field is optional because calibre **omits** a field
/// entirely when it has no value — `rating` and `series` are simply absent on a
/// book that has neither, rather than present and null.
#[derive(Debug, Deserialize)]
struct RawRow {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    authors: Option<StringOrList>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    pubdate: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    comments: Option<String>,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    formats: Option<StringOrList>,
    #[serde(default)]
    identifiers: Option<HashMap<String, String>>,
    #[serde(default)]
    isbn: Option<String>,
    #[serde(default)]
    languages: Option<StringOrList>,
    #[serde(default)]
    tags: Option<StringOrList>,
    #[serde(default)]
    series: Option<String>,
}

/// Parse `calibredb list --for-machine` output.
///
/// Pure, and public for the same reason [`crate::goodreads::parse_csv`] is: the
/// format belongs to another system, so the parser has to be testable against a
/// **recorded** sample of that system's real output rather than against our own
/// idea of it.
pub fn parse_library(json: &str) -> Result<Vec<CalibreBook>> {
    let rows: Vec<RawRow> = serde_json::from_str(json)?;
    Ok(rows.into_iter().map(row_to_book).collect())
}

fn row_to_book(r: RawRow) -> CalibreBook {
    // ISBN can arrive in either the dedicated column or the identifiers map,
    // and in practice the map is the one that is filled in. Both go through
    // `normalize_isbn` like every ISBN entering the system — calibre's field is
    // free text and holds ASINs, ISSNs and typos as readily as ISBNs.
    let raw_isbn = r
        .identifiers
        .as_ref()
        .and_then(|m| m.get("isbn"))
        .cloned()
        .or(r.isbn)
        .as_deref()
        .and_then(normalize_isbn);
    let (isbn_10, isbn_13) = match raw_isbn {
        Some(i) if i.len() == 10 => (Some(i), None),
        Some(i) => (None, Some(i)),
        None => (None, None),
    };

    CalibreBook {
        calibre_id: r.id,
        uuid: r.uuid.filter(|u| !u.trim().is_empty()),
        title: r.title.filter(|t| !t.trim().is_empty()),
        authors: r.authors.map(|a| a.names()).unwrap_or_default(),
        publisher: r.publisher.filter(|p| !p.trim().is_empty()),
        publish_year: r.pubdate.as_deref().and_then(year_of),
        language: r
            .languages
            .and_then(|l| l.first())
            .map(|l| normalize_language(&l)),
        isbn_10,
        isbn_13,
        description: r.comments.filter(|c| !c.trim().is_empty()),
        tags: r.tags.map(|t| t.values()).unwrap_or_default(),
        cover: r.cover.filter(|c| !c.is_empty()).map(PathBuf::from),
        formats: r
            .formats
            .map(|f| f.values().into_iter().map(PathBuf::from).collect())
            .unwrap_or_default(),
        series: r.series.filter(|s| !s.trim().is_empty()),
        added: r.timestamp.as_deref().and_then(unix_of),
    }
}

/// Calibre's `UNDEFINED_DATE` is year 101, which is what an undated book's
/// `pubdate` comes back as: `0101-01-01T00:00:00+00:00`. Anything before printing
/// existed is that sentinel, not a publication year.
const UNDEFINED_BEFORE: i64 = 1000;

/// The year out of an ISO-8601 timestamp, unless it is calibre's sentinel.
fn year_of(iso: &str) -> Option<i64> {
    let year: i64 = iso.get(..4)?.parse().ok()?;
    (year >= UNDEFINED_BEFORE).then_some(year)
}

/// A unix timestamp out of an ISO-8601 one. Only the date is used, so a missing
/// or odd timezone offset cannot move it by more than a day.
fn unix_of(iso: &str) -> Option<i64> {
    let date = time::Date::parse(
        iso.get(..10)?,
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .ok()?;
    let stamp = date.midnight().assume_utc().unix_timestamp();
    (year_of(iso)?.is_positive()).then_some(stamp)
}

/// Run `calibredb list --for-machine` and parse it.
pub async fn list_library(calibre: &Calibre, library: Option<&Path>) -> Result<Vec<CalibreBook>> {
    let bin = Calibre::require(CALIBREDB, calibre.calibredb_path())?;
    let library = library_root(library)?;
    let out = run(&bin, CALIBREDB, &list_argv(library)).await?;
    parse_library(&out)
}

// ---- import -----------------------------------------------------------------

/// How a calibre row found its book. The same shape as
/// [`GoodreadsMatch`](crate::goodreads::GoodreadsMatch) and deliberately not the
/// same enum: the branches differ, and one enum covering both would carry
/// variants unreachable from either caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibreMatch {
    /// An `external_ids` row already links this uuid. The only branch that is
    /// not a guess, and what makes a re-import idempotent.
    Uuid,
    Isbn,
    /// A file calibre holds is a file we have already identified — the middle
    /// rung of dedup level 3, and the one that survives a retitled book.
    Md5,
    Title,
    New,
}

impl std::fmt::Display for CalibreMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CalibreMatch::Uuid => "calibre uuid",
            CalibreMatch::Isbn => "isbn",
            CalibreMatch::Md5 => "file",
            CalibreMatch::Title => "title",
            CalibreMatch::New => "new",
        };
        f.write_str(s)
    }
}

#[derive(Debug)]
pub struct CalibreBookReport {
    /// `None` in a dry run for a book that does not exist yet.
    pub book_id: Option<i64>,
    pub title: String,
    pub matched_by: CalibreMatch,
    pub tags_added: usize,
    /// A cover was copied in from calibre's library.
    pub cover: bool,
    /// How many of calibre's files were identified by `partial_md5`, so a
    /// sidecar off the device later matches this book outright.
    pub files_linked: usize,
}

/// A row we will not guess about — the same shape as
/// [`UnmatchedRow`](crate::goodreads::UnmatchedRow), for the same reason:
/// unmatched has to be a decision, not a dead end.
#[derive(Debug)]
pub struct UnmatchedCalibreBook {
    pub calibre_id: i64,
    pub uuid: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub candidates: Vec<MatchCandidate>,
}

#[derive(Debug, Default)]
pub struct CalibreReport {
    pub dry_run: bool,
    /// Books calibre reported, whatever became of them.
    pub rows: usize,
    pub books: Vec<CalibreBookReport>,
    pub unmatched: Vec<UnmatchedCalibreBook>,
    pub warnings: Vec<Diagnostic>,
}

impl CalibreReport {
    pub fn created(&self) -> usize {
        self.books
            .iter()
            .filter(|b| b.matched_by == CalibreMatch::New)
            .count()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// The library to read. `None` means calibre's own default, which is the
    /// whole point of feature detection: the user configured it once, in
    /// calibre, and we do not ask again.
    pub library: Option<PathBuf>,
    pub dry_run: bool,
    /// Create a book for a row that landed in the candidate band instead of
    /// reporting it. The same escape hatch `ko pull --new` and
    /// `goodreads import --new` have.
    pub create_ambiguous: bool,
}

/// Import a calibre library.
///
/// Takes the whole [`Engine`] rather than a [`Storage`]: covers land in
/// `config.images_dir`, and the detected [`Calibre`] lives on the engine because
/// detection is once per run.
pub async fn import(engine: &Engine, opts: &ImportOptions) -> Result<CalibreReport> {
    let books = list_library(engine.calibre(), opts.library.as_deref()).await?;
    let storage = &engine.storage;
    let mut report = CalibreReport {
        dry_run: opts.dry_run,
        rows: books.len(),
        ..Default::default()
    };

    for cb in &books {
        if cb.title.is_none() {
            report.warnings.push(Diagnostic {
                kind: DiagnosticKind::CalibreRowSkipped {
                    calibre_id: cb.calibre_id,
                },
                severity: Severity::Warning,
                detail: "no title, so there is nothing to match on".to_string(),
            });
            continue;
        }
        if cb.uuid.is_none() {
            // Same shape as `SidecarNotIdentified`: the import still happens,
            // but it cannot be made idempotent, and the duplicate would appear
            // silently and long after the fact.
            report.warnings.push(Diagnostic {
                kind: DiagnosticKind::CalibreRowNotIdentified {
                    calibre_id: cb.calibre_id,
                },
                severity: Severity::Warning,
                detail: "no uuid; this book will be imported again on a second run".to_string(),
            });
        }

        // Hashed once per book and threaded through both halves. `match_book`
        // needs them for the `partial_md5` rung and `apply` needs the same set
        // to record; hashing inside each would read every file twice.
        let hashes = file_hashes(cb);
        let (book_id, matched_by) = match match_book(storage, cb, &hashes).await? {
            Matched::Book(id, how) => (Some(id), how),
            Matched::Ambiguous(candidates) if !opts.create_ambiguous => {
                report.unmatched.push(UnmatchedCalibreBook {
                    calibre_id: cb.calibre_id,
                    uuid: cb.uuid.clone(),
                    title: cb.display_title().to_string(),
                    authors: cb.authors.clone(),
                    candidates,
                });
                continue;
            }
            Matched::Ambiguous(_) | Matched::Nothing => (None, CalibreMatch::New),
        };

        if opts.dry_run {
            report
                .books
                .push(preview(storage, cb, book_id, matched_by, &hashes).await?);
            continue;
        }
        report.books.push(
            apply(
                engine,
                cb,
                book_id,
                matched_by,
                &hashes,
                &mut report.warnings,
            )
            .await?,
        );
    }
    Ok(report)
}

enum Matched {
    Book(i64, CalibreMatch),
    Ambiguous(Vec<MatchCandidate>),
    Nothing,
}

/// uuid → ISBN → `partial_md5` → the shared title scan.
///
/// That is dedup level 3 as `docs/decisions.md` states it, with calibre's own
/// stable id in front of it. **No second matcher**: `koreader::title_scores_for`
/// is the same fuzzy scan a sidecar and a Goodreads row get, with the same
/// `AUTO_MATCH` / `CANDIDATE_MIN` bands. Two fuzzy matchers would be two answers
/// to "is this the book I already have", and the one that disagreed would be the
/// one that made the duplicate.
async fn match_book(storage: &Storage, cb: &CalibreBook, hashes: &[String]) -> Result<Matched> {
    if let Some(uuid) = &cb.uuid
        && let Some(book_id) = storage.book_for_external_id(SOURCE, uuid).await?
    {
        return Ok(Matched::Book(book_id, CalibreMatch::Uuid));
    }
    for isbn in [&cb.isbn_13, &cb.isbn_10].into_iter().flatten() {
        if let Some(book) = storage.find_book_by_isbn(isbn).await?
            && let Some(book_id) = book.id
        {
            return Ok(Matched::Book(book_id, CalibreMatch::Isbn));
        }
    }
    for md5 in hashes {
        if let Some(book) = storage.find_book_by_partial_md5(md5).await?
            && let Some(book_id) = book.id
        {
            return Ok(Matched::Book(book_id, CalibreMatch::Md5));
        }
    }

    let scored = koreader::title_scores_for(storage, cb.title.as_deref()).await?;
    if let Some((score, book)) = scored.first()
        && *score >= koreader::AUTO_MATCH
        && let Some(book_id) = book.id
    {
        return Ok(Matched::Book(book_id, CalibreMatch::Title));
    }
    let candidates = koreader::candidates_for_title(storage, cb.title.as_deref()).await?;
    Ok(if candidates.is_empty() {
        Matched::Nothing
    } else {
        Matched::Ambiguous(candidates)
    })
}

/// The `partial_md5` of every format we can read.
///
/// An unreadable one is skipped rather than reported: calibre's library may sit
/// on a volume that is not mounted, and "one of five formats could not be
/// hashed" is not something a user can act on. What it costs is a match, and the
/// next rung catches it.
fn file_hashes(cb: &CalibreBook) -> Vec<String> {
    cb.formats
        .iter()
        .filter_map(|f| partial_md5::partial_md5(f).ok())
        .collect()
}

/// The read-only half, for `--dry-run`. Answers the same questions [`apply`]
/// does and writes nothing.
async fn preview(
    storage: &Storage,
    cb: &CalibreBook,
    book_id: Option<i64>,
    matched_by: CalibreMatch,
    hashes: &[String],
) -> Result<CalibreBookReport> {
    let (tags_added, cover) = match book_id {
        None => (cb.tags.len(), cb.cover.is_some()),
        Some(id) => {
            let known: Vec<String> = storage
                .book_tags(id)
                .await?
                .into_iter()
                .filter(|t| t.source == SOURCE)
                .map(|t| t.tag)
                .collect();
            let have_cover = storage
                .get_book(id)
                .await?
                .and_then(|b| b.cover_path)
                .is_some();
            (
                cb.tags
                    .iter()
                    .filter(|t| !known.contains(&slug_tag(t)))
                    .count(),
                cb.cover.is_some() && !have_cover,
            )
        }
    };
    Ok(CalibreBookReport {
        book_id,
        title: cb.display_title().to_string(),
        matched_by,
        tags_added,
        cover,
        files_linked: unlinked(storage, hashes).await?,
    })
}

/// How many of these files are **not** already identified.
///
/// The count has to be what the import *changed*, not what it looked at — the
/// same rule the Goodreads import applies to a rating it did not rewrite. Before
/// this, a re-import of an unchanged library announced "1 file identified"
/// against every book, for ever, which reads as work it did not do.
async fn unlinked(storage: &Storage, hashes: &[String]) -> Result<usize> {
    let mut n = 0;
    for md5 in hashes {
        if storage.find_book_by_partial_md5(md5).await?.is_none() {
            n += 1;
        }
    }
    Ok(n)
}

async fn apply(
    engine: &Engine,
    cb: &CalibreBook,
    book_id: Option<i64>,
    matched_by: CalibreMatch,
    hashes: &[String],
    warnings: &mut Vec<Diagnostic>,
) -> Result<CalibreBookReport> {
    let storage = &engine.storage;
    // Counted before the links are written, or every one of them would look new.
    let files_linked = unlinked(storage, hashes).await?;

    // **`upsert_book`, i.e. the provider no-clobber merge — not the device's
    // straight assignment.** `docs/decisions.md` names calibre as the origin for
    // curated metadata, which argues for assignment; but the pattern is chosen
    // by whether the record is *complete*, not by who owns it. A sidecar is the
    // device's complete state, so a missing note there means the user deleted
    // it. A `calibredb list` row is nothing of the kind: it carries no page
    // count at all, and its ISBN is empty far more often than not. Assigning it
    // straight would blank fields calibre has no opinion about.
    let mut book = Book {
        title: cb.title.clone(),
        authors: cb.authors.clone(),
        publisher: cb.publisher.clone(),
        publish_year: cb.publish_year,
        language: cb.language.clone(),
        isbn_10: cb.isbn_10.clone(),
        isbn_13: cb.isbn_13.clone(),
        description: cb.description.clone(),
        ..Default::default()
    };
    let created = book_id.is_none();
    let mut cover_copied = false;
    // The cover is the providers' field, and calibre's is a different edition's
    // as often as not — so it fills a gap and never replaces one.
    let have_cover = match book_id {
        Some(id) => storage
            .get_book(id)
            .await?
            .and_then(|b| b.cover_path)
            .is_some(),
        None => false,
    };
    if !have_cover && let Some(src) = &cb.cover {
        match copy_cover(src, &engine.config.images_dir, cb) {
            Ok(path) => {
                book.cover_path = Some(path.display().to_string());
                cover_copied = true;
            }
            Err(e) => warnings.push(Diagnostic {
                kind: DiagnosticKind::CalibreCoverUnreadable { path: src.clone() },
                severity: Severity::Warning,
                detail: e.to_string(),
            }),
        }
    }

    // `enrich_book` for a book we have already identified, `upsert_book` only
    // when creating one. `upsert_book`'s third branch — no `isbn_10`, no
    // `isbn_13` — is a plain unconditional insert and ignores `Book::id`
    // entirely, so using it on a match found by uuid or by file hash (neither of
    // which implies an ISBN) silently makes a second copy of the book on every
    // run. Found by `importing_the_same_library_twice_changes_nothing`, which is
    // why that test asserts the library's *size* and not only the report.
    let book_id = match book_id {
        Some(id) => {
            storage.enrich_book(id, &book).await?;
            id
        }
        None => storage.upsert_book(&book).await?,
    };
    // `timestamp` is when the user added it to *calibre*, which is exactly what
    // `books.created_at` means — and only for a book this import just created:
    // for one we already had, when it arrived here is ours to know.
    if created && let Some(added) = cb.added {
        storage.set_book_created_at(book_id, added).await?;
    }

    if let Some(uuid) = &cb.uuid {
        storage.link_external_id(SOURCE, uuid, book_id).await?;
    }
    let tags: Vec<(String, String)> = cb.tags.iter().map(|t| (slug_tag(t), t.clone())).collect();
    let tags_added = storage.add_book_tags(book_id, SOURCE, &tags).await?;

    // The payoff arrives later and elsewhere, exactly as it does in
    // `import_epub`: when this book's sidecar comes off the device, `match_book`
    // takes its `md5` branch instead of guessing at the title — including when
    // KOReader keeps sidecars in `dir` or `hash` mode, away from the book.
    //
    // `link_device_book`, never `set_device_link`: this is a scan, and a scan
    // must not relabel a link the user made by hand.
    for md5 in hashes {
        storage
            .link_device_book(md5, book_id, LinkedBy::Auto)
            .await?;
    }

    Ok(CalibreBookReport {
        book_id: Some(book_id),
        title: cb.display_title().to_string(),
        matched_by,
        tags_added,
        cover: cover_copied,
        files_linked,
    })
}

/// Copy calibre's cover into our own images dir.
///
/// Named from the uuid, not from calibre's path: every cover in a calibre
/// library is called `cover.jpg`, so the source name would collide for every
/// book after the first.
fn copy_cover(src: &Path, images_dir: &Path, cb: &CalibreBook) -> Result<PathBuf> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("jpg");
    let stem = cb
        .uuid
        .clone()
        .unwrap_or_else(|| format!("id-{}", cb.calibre_id));
    std::fs::create_dir_all(images_dir)?;
    let dst = images_dir.join(format!("calibre-{stem}.{ext}"));
    std::fs::copy(src, &dst)?;
    Ok(dst)
}

/// Our normalization of a tag. The raw value is stored beside it, which is what
/// makes this safe to change later — the same treatment a Goodreads shelf gets,
/// and for the same reason: `docs/decisions.md` defers collections, and these
/// rows are inert provenance kept so that design can be made against real data.
fn slug_tag(tag: &str) -> String {
    tag.trim().to_lowercase().replace(' ', "-")
}

// ---- running a calibre tool -------------------------------------------------

/// Run a calibre binary and return its stdout.
///
/// `tokio::process`, not `std::process`: `ebook-convert` on a large book runs for
/// minutes, and blocking the executor thread for that would stop every other
/// task in the frontend — including, in the TUI, the draw loop.
async fn run(bin: &Path, tool: &str, argv: &[String]) -> Result<String> {
    let out = tokio::process::Command::new(bin)
        .args(argv)
        .output()
        .await
        .map_err(|e| EngineError::Calibre {
            tool: tool.to_string(),
            message: format!("{}: {e}", bin.display()),
        })?;
    if !out.status.success() {
        return Err(EngineError::Calibre {
            tool: tool.to_string(),
            message: tail(&String::from_utf8_lossy(&out.stderr)),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Calibre is chatty on failure and the interesting part is at the end. Trimmed
/// so a diagnostic stays a line or two rather than a screen of Python.
fn tail(stderr: &str) -> String {
    let msg: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let n = msg.len().saturating_sub(3);
    let text = msg[n..].join("; ");
    if text.is_empty() {
        "failed with no output".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded from calibre 7.26, not written from the documentation — the same
    /// rule the Goodreads `recorded/` fixtures follow, and for the same reason:
    /// generating a sample from our own understanding of another system's format
    /// would prove only that we agree with ourselves.
    const RECORDED: &str = include_str!("../tests/fixtures/calibre/recorded/library.json");

    #[test]
    fn the_recorded_library_parses_the_way_calibre_actually_writes_it() {
        let books = parse_library(RECORDED).unwrap();
        assert_eq!(books.len(), 3);

        let p = &books[0];
        assert_eq!(p.title.as_deref(), Some("Pachinko"));
        // The trap: `authors` is a `&`-joined *string* where every other
        // multi-valued field is a JSON array.
        assert_eq!(p.authors, vec!["Min Jin Lee", "Deborah Smith"]);
        assert_eq!(
            p.uuid.as_deref(),
            Some("c47437a8-d73c-4719-838d-ff362479bf63")
        );
        assert_eq!(p.isbn_13.as_deref(), Some("9781455563937"));
        assert_eq!(p.isbn_10, None);
        assert_eq!(p.publish_year, Some(2017));
        assert_eq!(
            p.language.as_deref(),
            Some("en"),
            "MARC 'eng' is normalized"
        );
        assert_eq!(p.tags, vec!["fiction", "korean-lit"]);
        assert_eq!(p.series.as_deref(), Some("Saga"));
        assert_eq!(p.formats.len(), 2, "every format, not just the epub");
        assert!(p.cover.is_some());
    }

    /// The second trap, and the one that would have shipped: an undated book
    /// carries calibre's `UNDEFINED_DATE` rather than no `pubdate` at all.
    #[test]
    fn calibres_undefined_date_is_not_a_publication_year() {
        assert_eq!(year_of("0101-01-01T00:00:00+00:00"), None);
        assert_eq!(year_of("2017-02-07T00:00:00+00:00"), Some(2017));
        assert_eq!(year_of(""), None);
        assert_eq!(year_of("not a date"), None);

        let books = parse_library(RECORDED).unwrap();
        let undated = &books[1];
        assert_eq!(undated.publish_year, None, "0101 is a sentinel, not a year");
        // …and its `timestamp` is a real date, so the sentinel filter is not
        // simply dropping every date.
        assert!(undated.added.is_some());
    }

    /// Calibre omits a field entirely when it has no value, rather than writing
    /// null — so a minimal row must parse, not fail.
    #[test]
    fn a_row_with_almost_nothing_on_it_still_parses() {
        let books = parse_library(RECORDED).unwrap();
        let bare = &books[2];
        assert_eq!(bare.title.as_deref(), Some("Untitled Draft"));
        assert!(bare.authors.is_empty());
        assert_eq!(bare.series, None);
        assert_eq!(bare.publisher, None);
        assert_eq!(bare.isbn_10, None);
        assert_eq!(bare.isbn_13, None);
        assert!(bare.tags.is_empty());
        assert!(bare.formats.is_empty());
        assert_eq!(bare.cover, None);
    }

    #[test]
    fn an_empty_library_is_no_books_rather_than_an_error() {
        assert_eq!(parse_library("[]").unwrap().len(), 0);
    }

    /// Both shapes, because the point of accepting both is that we do not know
    /// which one the next calibre writes.
    #[test]
    fn authors_parse_from_a_joined_string_or_a_list() {
        let joined = parse_library(r#"[{"id":1,"title":"T","authors":"A & B"}]"#).unwrap();
        assert_eq!(joined[0].authors, vec!["A", "B"]);
        let listed = parse_library(r#"[{"id":1,"title":"T","authors":["A","B"]}]"#).unwrap();
        assert_eq!(listed[0].authors, vec!["A", "B"]);
        // A tag with a space keeps it; the slug is where normalization happens.
        let tagged = parse_library(r#"[{"id":1,"title":"T","tags":["Korean Lit"]}]"#).unwrap();
        assert_eq!(tagged[0].tags, vec!["Korean Lit"]);
        assert_eq!(slug_tag("Korean Lit"), "korean-lit");
    }

    /// An ISBN reaches `normalize_isbn` whichever column it came in, and
    /// calibre's free-text identifier junk does not become one.
    #[test]
    fn an_isbn_is_validated_wherever_calibre_put_it() {
        let from_map =
            parse_library(r#"[{"id":1,"title":"T","identifiers":{"isbn":"0316769487"}}]"#).unwrap();
        assert_eq!(from_map[0].isbn_10.as_deref(), Some("0316769487"));

        let from_col = parse_library(r#"[{"id":1,"title":"T","isbn":"9781455563937"}]"#).unwrap();
        assert_eq!(from_col[0].isbn_13.as_deref(), Some("9781455563937"));

        // An ASIN is what that column holds as often as an ISBN.
        let junk =
            parse_library(r#"[{"id":1,"title":"T","identifiers":{"isbn":"B01N0Q4B5F"}}]"#).unwrap();
        assert_eq!(junk[0].isbn_10, None);
        assert_eq!(junk[0].isbn_13, None);
    }

    /// The command lines, asserted rather than eyeballed — they are the whole
    /// interface to another program, and a test is the only thing that notices a
    /// flag going missing.
    #[test]
    fn the_command_lines_are_what_calibre_documents() {
        assert_eq!(
            list_argv(None),
            ["list", "--for-machine", "--fields", "all"]
        );
        assert_eq!(
            list_argv(Some(Path::new("/books/lib"))),
            [
                "--with-library",
                "/books/lib",
                "list",
                "--for-machine",
                "--fields",
                "all"
            ]
        );
        // Conversion sends the two paths and nothing else: calibre reads both
        // formats off the extensions, and every option beyond that is a decision
        // about somebody's book that we have no standing to make.
        assert_eq!(
            convert_argv(Path::new("in.epub"), Path::new("out.azw3")),
            ["in.epub", "out.azw3"]
        );
    }

    /// Absent is a first-class answer, and it is *typed* — the CLI branches on
    /// it to say "calibre is not installed" rather than printing a failure.
    #[tokio::test]
    async fn an_absent_calibre_is_a_named_error_and_never_a_panic() {
        let none = Calibre::default();
        assert!(!none.can_convert());
        assert!(!none.can_read_library());

        let e = list_library(&none, None).await.unwrap_err();
        assert!(matches!(e, EngineError::CalibreMissing { .. }), "{e:?}");
        let e = convert(&none, Path::new("/x.epub"), Path::new("/x.azw3"), false)
            .await
            .unwrap_err();
        assert!(matches!(e, EngineError::CalibreMissing { .. }), "{e:?}");
    }

    /// The guard that keeps a typo from creating a calibre library on the user's
    /// disk. Verified against calibre 7.26, which does exactly that.
    #[test]
    fn a_directory_that_is_not_a_calibre_library_is_refused_before_anything_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let e = library_root(Some(tmp.path())).unwrap_err();
        assert!(matches!(e, EngineError::InvalidInput(_)), "{e:?}");
        assert!(e.to_string().contains("metadata.db"));

        std::fs::write(tmp.path().join("metadata.db"), b"").unwrap();
        assert_eq!(library_root(Some(tmp.path())).unwrap(), Some(tmp.path()));
        // No library asked for is calibre's own default, which is fine.
        assert_eq!(library_root(None).unwrap(), None);
    }

    /// A directory named like a tool is not a tool, and neither is a file
    /// nobody can run. Without the executable check, "calibre is absent" would
    /// become "calibre fails on every call".
    ///
    /// Asserted on [`is_executable`] rather than on [`find_tool`], because
    /// `find_tool` falls through to the real `PATH` — so on a developer machine
    /// with calibre installed the negative half would find the real binary and
    /// pass for the wrong reason. The positive half below *is* `find_tool`,
    /// which is where "the extra directory is searched first" is pinned.
    #[cfg(unix)]
    #[test]
    fn detection_wants_an_executable_file_not_a_name() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();

        std::fs::create_dir(tmp.path().join(CALIBREDB)).unwrap();
        assert!(!is_executable(&tmp.path().join(CALIBREDB)), "a directory");
        assert!(!is_executable(&tmp.path().join("absent")));

        let bin = tmp.path().join(CONVERT);
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        assert!(!is_executable(&bin), "present but not executable");

        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&bin));
        assert_eq!(
            find_tool(CONVERT, Some(tmp.path())),
            Some(bin),
            "the extra directory is searched before PATH"
        );
    }

    #[test]
    fn a_calibre_failure_keeps_the_end_of_what_it_said() {
        let noisy = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(tail(&noisy), "line 17; line 18; line 19");
        assert_eq!(tail("   \n\n"), "failed with no output");
    }
}
