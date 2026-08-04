//! Reading time, from the device's own `statistics.sqlite3`.
//!
//! [`crate::partial_md5`]'s doc comment has always named "the
//! `statistics.sqlite3` join" as one of the three things that function exists
//! for, and until this module nothing did it. `device_books` already held the
//! key on our side; KOReader's statistics plugin holds the same key on its
//! side, under the column name `book.md5`.
//!
//! This is **one more filler of `reading_events`** and deliberately nothing
//! else: day, minutes, pages, `source = 'koreader'`, `confidence = 'measured'`.
//! No view, no query and no line of frontend changes because it arrived, which
//! is the whole of item 21's argument for a source-agnostic log. There is no
//! `reading_sessions` table here and no KOReader-shaped struct that reaches the
//! facade — if either ever appears, the design has gone wrong.
//!
//! # Why this is its own module and not part of `device.rs`
//!
//! `device.rs` exists because *a scan is not an import*, and the same argument
//! applies again one level down. Three things separate this from that module:
//! it reads a **SQLite database** rather than a tree of Lua sidecars, so it
//! shares no parsing, no `find_sidecars` walk and no `sidecar_seen` cache; it
//! joins on `device_books` rather than on `koreader::match_book`, so it shares
//! no matching; and it **writes**, which is the one thing `scan_device` is
//! defined by not doing. Folding a writing verb into the module whose identity
//! is read-only scanning is exactly the confusion `device.rs` was split out to
//! avoid.
//!
//! # What the schema actually is
//!
//! Settled from **KOReader's own source** — `plugins/statistics.koplugin/main.lua`
//! on `koreader/koreader@master` — which is the order of authority
//! `docs/koreader-format.md` established: the source is the spec, a fixture is
//! only ever evidence. Verbatim, and reproduced in the fixture generator
//! (`crates/corpus/src/kostats.rs`) so that there is one transcription of it:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS book (
//!     id integer PRIMARY KEY autoincrement,
//!     title text, authors text, notes integer, last_open integer,
//!     highlights integer, pages integer, series text, language text,
//!     md5 text, total_read_time integer, total_read_pages integer);
//!
//! CREATE TABLE IF NOT EXISTS page_stat_data (
//!     id_book integer, page integer NOT NULL DEFAULT 0,
//!     start_time integer NOT NULL DEFAULT 0, duration integer NOT NULL DEFAULT 0,
//!     total_pages integer NOT NULL DEFAULT 0,
//!     UNIQUE (id_book, page, start_time), FOREIGN KEY(id_book) REFERENCES book(id));
//! ```
//!
//! Five things that were **not** in the spec's description of it, each of which
//! changed what this module does:
//!
//! 1. **`book.md5` is `util.partialMD5`** — the plugin computes it with
//!    `util.partialMD5(book_stats.file)`, the same function
//!    [`crate::partial_md5`] reimplements. So the join is exact, not fuzzy, and
//!    no title matching happens here at all. Note this does *not* contradict
//!    `docs/koreader-format.md` §2.1: that section is about the **sidecar's**
//!    `stats` subtable, which really does carry no `md5`. The statistics
//!    database is a different file and does.
//! 2. **`page_stat_data.duration` is in seconds**, and `start_time` is
//!    `os.time()` — a true unix epoch, unlike the sidecar's zoneless local
//!    wall clock. See [`DAY_SKEW`] below, which is the sharpest correction this
//!    item forced.
//! 3. **The database is WAL** (`PRAGMA journal_mode=WAL` where the device
//!    supports it), so copying only the main file loses every page turn since
//!    the last checkpoint. [`copy_out`] takes the `-wal` and `-shm` siblings.
//! 4. **`PRAGMA user_version` carries `DB_SCHEMA_VERSION`**, currently
//!    `20221111`. That is the version gate the item asked for, and it is a real
//!    number rather than a guess.
//! 5. **There is a `page_stat` VIEW** that rescales page numbers onto the
//!    book's current page count. This module reads the raw table instead — see
//!    [`AGGREGATE`].
//!
//! # Privacy
//!
//! Reading time is the user's private reading. Per the root `CLAUDE.md` tracing
//! rule, **no duration, page count or day appears above `trace!`**; the
//! `debug!` line carries structural counts only.

use std::path::{Path, PathBuf};

use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};

use crate::device::koreader_dir;
use crate::diagnostic::Diagnostic;
use crate::error::{EngineError, Result};
use crate::storage::{Confidence, FillStats, NewReadingEvent, SOURCE_KOREADER, Storage};

/// `DB_SCHEMA_VERSION` in `plugins/statistics.koplugin/main.lua`, stamped into
/// `PRAGMA user_version` when the plugin creates or migrates its database.
///
/// A database reporting anything else is **refused**, not read on a hunch. The
/// plugin stamps this precisely so a reader can tell, and a wrong number of
/// minutes is worse than none: nothing downstream can distinguish it from a
/// right one, and it would ratchet `confidence` to `measured` on the way past.
pub const KNOWN_SCHEMA_VERSION: i64 = 20221111;

/// The file name, and the directory it sits in relative to the KOReader
/// install (`DataStorage:getSettingsDir()`).
pub const STATS_DB_NAME: &str = "statistics.sqlite3";
const SETTINGS_DIR: &str = "settings";

/// **The correction worth carrying out of this item.**
///
/// The two KOReader sources disagree about what a timestamp is, and item 21's
/// log has one `day` column for both:
///
/// * A sidecar's `datetime` is `os.date("%Y-%m-%d %H:%M:%S")` — device-*local*
///   wall clock with no zone — which `ko_datetime_to_unix` and the highlight
///   filler both read as UTC. That is not a bug; there is no offset recorded to
///   read it any other way.
/// * `page_stat_data.start_time` is `os.time()` — a genuine unix epoch, which
///   `date(start_time,'unixepoch')` turns into a genuine **UTC** day.
///
/// So for a reader west of Greenwich, an evening's reading can be stamped one
/// day by the highlight filler and the previous day by this one, and the two
/// then land on different `(book_id, day, source)` rows instead of merging.
///
/// **Left as-is, deliberately.** The alternatives are all worse: shifting
/// `start_time` by a guessed offset invents data, and shifting it by *this*
/// machine's offset would make the import's result depend on where the desktop
/// happens to be, so re-importing on a laptop that flew somewhere would rewrite
/// history. Item 21 chose UTC for the whole table and this follows it. The
/// visible consequence is bounded and benign — an extra `inferred` row on an
/// adjacent day, never a wrong minute count — and the honest fix is for a
/// device to record its offset, which no KOReader version does.
pub const DAY_SKEW: () = ();

/// What one pass over a device's statistics did.
///
/// Every field is a count of something structural. `minutes` is deliberately
/// **not** summarised here: the aggregate belongs to
/// [`Storage::activity_summary`], which is the surface item 21 built for it,
/// and duplicating it in an importer's report is how a second answer to the
/// same question gets started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatsImportReport {
    /// `PRAGMA user_version` as read. `None` when there was no database.
    pub schema_version: Option<i64>,
    /// Rows in the device's `book` table.
    pub books_in_db: usize,
    /// Of those, how many joined to a book in this library.
    pub books_matched: usize,
    /// `(book, day)` pairs the device had measured time for, among matched
    /// books.
    pub days: usize,
    /// What the write did. `inserted == 0 && updated == 0` on a second run over
    /// an unchanged device is the property this module lives on.
    pub events: FillStats,
    pub warnings: Vec<Diagnostic>,
}

/// The expected path of a device's statistics database, given its KOReader
/// install directory.
pub fn statistics_db(ko_dir: &Path) -> PathBuf {
    ko_dir.join(SETTINGS_DIR).join(STATS_DB_NAME)
}

/// One day of one book, as the device measured it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MeasuredDay {
    md5: String,
    day: String,
    seconds: i64,
    pages: i64,
}

/// The aggregation, run against **our copy** of the device's database.
///
/// Three decisions, in order of how much they matter:
///
/// * **`page_stat_data`, not the `page_stat` view.** The view exists to rescale
///   historical page numbers onto the book's *current* page count, which it
///   does by exploding one row into several through a `numbers` helper table
///   and dividing `duration` between them with integer division. For a
///   per-day total that is strictly worse on both columns: the division loses
///   seconds, and the explosion inflates a `count(DISTINCT page)`. We want what
///   the device recorded, not what it would look like after a re-render.
/// * **`start_time > 0`.** The column's declared default is `0`, and a zero
///   would aggregate into `1970-01-01` — a day nobody read, presented as a day
///   somebody did. A device with a dead clock is the ordinary way to get one.
/// * **`count(DISTINCT page)`, not `count(*)`.** A page revisited three times
///   in an evening is one page turned, and three rows in this table.
///
/// `date(...)` doubles as the validator exactly as it does in the fillers: it
/// returns NULL for anything it cannot read as a day, and `HAVING day IS NOT
/// NULL` is the whole of "no source invents a day".
const AGGREGATE: &str = "
    SELECT b.md5                            AS md5,
           date(p.start_time, 'unixepoch')  AS day,
           sum(p.duration)                  AS seconds,
           count(DISTINCT p.page)           AS pages
      FROM page_stat_data p
      JOIN book b ON b.id = p.id_book
     WHERE b.md5 IS NOT NULL AND b.md5 <> '' AND p.start_time > 0
     GROUP BY b.md5, day
    HAVING day IS NOT NULL
     ORDER BY b.md5, day";

/// Import measured reading time off a mounted device.
///
/// **Not part of `sync_device`'s default path**, and that is a rule rather than
/// an oversight: `docs/decisions.md` makes arrival read-only, and a scan that
/// silently started importing months of timing data would not be read-only in
/// spirit even though every byte it writes is ours. The user asks for this by
/// name.
pub async fn import_device_statistics(
    storage: &Storage,
    mount: &Path,
) -> Result<StatsImportReport> {
    let Some(ko_dir) = koreader_dir(mount) else {
        return Err(EngineError::InvalidInput(format!(
            "{} is not a KOReader device",
            mount.display()
        )));
    };
    import_statistics(storage, &statistics_db(&ko_dir)).await
}

/// Import measured reading time from a statistics database at a known path.
///
/// Never fails on absence. No database, an unreadable one, a schema we do not
/// know, a book we do not hold — every one of those is a [`Diagnostic`] and an
/// otherwise-empty report. *A month with no device data returns absent minutes,
/// not zero*, and that distinction is carried all the way down: this module
/// writes no row at all for a day the device did not measure, so
/// [`Storage::activity_summary`] reports `None` rather than `Some(0)`.
#[tracing::instrument(skip(storage), fields(path = %db_path.display()))]
pub async fn import_statistics(storage: &Storage, db_path: &Path) -> Result<StatsImportReport> {
    let mut report = StatsImportReport::default();

    if !db_path.is_file() {
        report
            .warnings
            .push(Diagnostic::statistics_db_absent(db_path.to_path_buf()));
        return Ok(report);
    }

    // Copy first, always. It is the user's device and a live database that
    // KOReader may be mid-write on; the copy is ours, and the original is never
    // opened by SQLite at all.
    let scratch = tempfile::tempdir()?;
    let copy = match copy_out(db_path, scratch.path()) {
        Ok(c) => c,
        Err(e) => {
            report.warnings.push(Diagnostic::statistics_db_unreadable(
                db_path.to_path_buf(),
                &EngineError::from(e),
            ));
            return Ok(report);
        }
    };

    // Every read of the copy is inside this one fallible region, and that is
    // deliberate rather than tidy: SQLite does **not** notice that a file is
    // not a database when it opens it. It reports `file is not a database` on
    // the first statement, so a version read outside the guard propagates a raw
    // sqlx error out of a function whose whole contract is that absence is
    // ordinary. Found by `a_file_that_is_not_a_database_degrades`.
    let measured = match read_statistics(&copy, db_path, &mut report).await {
        // Read fine.
        Ok(Some(days)) => days,
        // A schema we do not know: `report` already carries the diagnostic.
        Ok(None) => return Ok(report),
        Err(e) => {
            report.warnings.push(Diagnostic::statistics_db_unreadable(
                db_path.to_path_buf(),
                &e,
            ));
            return Ok(report);
        }
    };

    write_events(storage, measured, &mut report).await?;

    tracing::debug!(
        books_in_db = report.books_in_db,
        books_matched = report.books_matched,
        days = report.days,
        inserted = report.events.inserted,
        updated = report.events.updated,
        "imported device statistics"
    );
    Ok(report)
}

/// Open the copy, check the schema version, and read it.
///
/// `Ok(None)` means the version gate refused — a decision, with its diagnostic
/// already recorded. `Err` means SQLite would not read the file, which the
/// caller turns into [`DiagnosticKind::StatisticsDbUnreadable`]; the split
/// keeps "we chose not to" and "we could not" from being the same value.
async fn read_statistics(
    copy: &Path,
    origin: &Path,
    report: &mut StatsImportReport,
) -> Result<Option<Vec<MeasuredDay>>> {
    let mut conn = open_copy(copy).await?;

    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut conn)
        .await?;
    report.schema_version = Some(version);
    if version != KNOWN_SCHEMA_VERSION {
        report.warnings.push(Diagnostic::statistics_schema_unknown(
            origin.to_path_buf(),
            version,
        ));
        conn.close().await.ok();
        return Ok(None);
    }

    let days = read_days(&mut conn, report).await?;
    conn.close().await.ok();
    Ok(Some(days))
}

/// Read the device's book list and its per-day totals.
///
/// The book list is walked separately from the aggregate so that a book with
/// rows but no usable `md5`, and a book we simply do not hold, are each
/// reported once — the aggregate alone could not say anything about a row it
/// filtered out.
async fn read_days(
    conn: &mut SqliteConnection,
    report: &mut StatsImportReport,
) -> Result<Vec<MeasuredDay>> {
    let books = sqlx::query("SELECT title, md5 FROM book")
        .fetch_all(&mut *conn)
        .await?;
    report.books_in_db = books.len();
    for b in &books {
        let md5: Option<String> = b.get("md5");
        if md5.as_deref().unwrap_or("").is_empty() {
            let title: Option<String> = b.get("title");
            report
                .warnings
                .push(Diagnostic::statistics_book_not_identified(
                    title.as_deref().unwrap_or("(untitled)"),
                ));
        }
    }

    let rows = sqlx::query(AGGREGATE).fetch_all(&mut *conn).await?;
    Ok(rows
        .iter()
        .map(|r| MeasuredDay {
            md5: r.get("md5"),
            day: r.get("day"),
            seconds: r.get("seconds"),
            pages: r.get("pages"),
        })
        .collect())
}

/// Join to the library and write. One `record_reading_events` call, so the
/// no-clobber merge and the idempotency it buys are item 21's, not a second
/// implementation of them.
async fn write_events(
    storage: &Storage,
    measured: Vec<MeasuredDay>,
    report: &mut StatsImportReport,
) -> Result<()> {
    let mut events = Vec::new();
    let mut unmatched = std::collections::BTreeSet::new();
    let mut matched = std::collections::BTreeSet::new();

    for m in measured {
        let Some(book) = storage.find_book_by_partial_md5(&m.md5).await? else {
            unmatched.insert(m.md5);
            continue;
        };
        let Some(book_id) = book.id else { continue };
        matched.insert(book_id);

        // Per-file statistics know nothing about rereads, so the attribution
        // comes from our own reading windows and stays NULL when they do not
        // settle on one.
        let reading_id = storage.reading_for_day(book_id, &m.day).await?;

        tracing::trace!(
            book_id,
            day = %m.day,
            seconds = m.seconds,
            pages = m.pages,
            "measured day"
        );

        events.push(NewReadingEvent {
            book_id,
            reading_id,
            day: m.day,
            minutes: Some(minutes_of(m.seconds)),
            pages: Some(m.pages),
            source: SOURCE_KOREADER.to_string(),
            confidence: Confidence::Measured,
        });
    }

    for md5 in unmatched {
        report
            .warnings
            .push(Diagnostic::statistics_book_unmatched(&md5));
    }
    report.books_matched = matched.len();
    report.days = events.len();
    report.events = storage.record_reading_events(&events).await?;
    Ok(())
}

/// Seconds to whole minutes, **rounded to nearest**.
///
/// The grain of `reading_events` is a day and its unit is minutes, so some
/// rounding is unavoidable. Two consequences worth stating rather than
/// discovering:
///
/// * A day of genuine but very short reading records `Some(0)`, not `None`.
///   That is correct and is the distinction item 21 pinned in
///   `a_measured_zero_is_not_an_absent_one`: a device saying "you opened this
///   for twenty seconds" is *saying something*. `None` is reserved for days the
///   device never measured at all, which this module writes no row for.
/// * Nearest rather than floor, because floor biases every single day downward
///   and a month of short sessions would then read as materially less reading
///   than happened.
fn minutes_of(seconds: i64) -> i64 {
    (seconds.max(0) + 30) / 60
}

/// Copy the database and its write-ahead siblings into `dir`.
///
/// **The `-wal` file is not optional.** KOReader runs this database in WAL mode
/// wherever the device supports it, so the main file alone is the state as of
/// the last checkpoint — on an actively-used device that can be missing the
/// whole of a recent session. `-shm` goes too: it is rebuildable, but copying
/// it keeps the three files mutually consistent as a set.
///
/// The copy is what gets opened, so the device file is never touched by SQLite:
/// no lock taken, no journal replayed on its volume, no `VACUUM`, nothing
/// written. That is the same discipline `scan_device` holds, and here it is
/// enforced by there being no code path that names the original after this
/// function returns.
fn copy_out(src: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    let dst = dir.join(STATS_DB_NAME);
    std::fs::copy(src, &dst)?;
    for suffix in ["-wal", "-shm"] {
        let sibling = with_suffix(src, suffix);
        if sibling.is_file() {
            std::fs::copy(&sibling, with_suffix(&dst, suffix))?;
        }
    }
    Ok(dst)
}

/// `foo.sqlite3` + `-wal` -> `foo.sqlite3-wal`, which is the name SQLite
/// itself uses (it appends to the full name, it does not replace an extension).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Open our copy.
///
/// `create_if_missing(false)` is load-bearing rather than tidy, and `calibre.rs`
/// paid for the lesson: `calibredb --with-library /typo` *creates* a library at
/// the typo and reports success, so a mistyped path is indistinguishable from
/// an empty one. An SQLite driver that helpfully creates the file would do
/// exactly the same thing here — report a database with no books rather than a
/// database that is not there.
///
/// Opened read-write even though nothing writes: this is our own copy in our
/// own temporary directory, and SQLite needs write access to replay the `-wal`
/// we just copied beside it. Read-only would silently skip that recovery and
/// report the pre-checkpoint state — the very data loss `copy_out` exists to
/// avoid.
async fn open_copy(path: &Path) -> Result<SqliteConnection> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false);
    Ok(SqliteConnection::connect_with(&opts).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_round_to_the_nearest_minute_and_a_short_day_is_measured_zero() {
        assert_eq!(minutes_of(0), 0);
        assert_eq!(minutes_of(29), 0, "a genuinely tiny day is a measured zero");
        assert_eq!(minutes_of(30), 1);
        assert_eq!(minutes_of(90), 2, "nearest, not floor");
        assert_eq!(minutes_of(3600), 60);
        // Never negative, whatever a device with a broken clock recorded.
        assert_eq!(minutes_of(-5), 0);
    }

    #[test]
    fn the_wal_siblings_are_named_the_way_sqlite_names_them() {
        let p = Path::new("/x/statistics.sqlite3");
        assert_eq!(
            with_suffix(p, "-wal"),
            Path::new("/x/statistics.sqlite3-wal"),
            "SQLite appends to the whole name; it does not replace the extension"
        );
        assert_eq!(
            with_suffix(p, "-shm"),
            Path::new("/x/statistics.sqlite3-shm")
        );
    }

    #[test]
    fn the_db_sits_in_the_settings_dir_of_the_install() {
        assert_eq!(
            statistics_db(Path::new("/Volumes/KOBOeReader/.adds/koreader")),
            Path::new("/Volumes/KOBOeReader/.adds/koreader/settings/statistics.sqlite3")
        );
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use crate::book::Book;
    use crate::storage::{DayRange, LinkedBy, day_of_unix};
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    /// 2026-01-05 00:00:00 UTC.
    const BASE: i64 = 1_767_571_200;
    const DAY: i64 = 86_400;

    /// The two tables this module reads, and nothing else.
    ///
    /// A third transcription of the plugin's DDL, and deliberately the *minimal*
    /// one: this harness exists to vary the data, not to re-assert the schema.
    /// The committed fixture is where the full verbatim DDL is exercised, and
    /// it is the generator's copy that is authoritative.
    const DDL: &str = "
        PRAGMA user_version=20221111;
        CREATE TABLE book (id integer PRIMARY KEY autoincrement, title text,
            authors text, notes integer, last_open integer, highlights integer,
            pages integer, series text, language text, md5 text,
            total_read_time integer, total_read_pages integer);
        CREATE TABLE page_stat_data (id_book integer, page integer NOT NULL DEFAULT 0,
            start_time integer NOT NULL DEFAULT 0, duration integer NOT NULL DEFAULT 0,
            total_pages integer NOT NULL DEFAULT 0,
            UNIQUE (id_book, page, start_time), FOREIGN KEY(id_book) REFERENCES book(id));";

    /// One recorded page turn: which book, which day, how long.
    #[derive(Debug, Clone)]
    struct Session {
        book: usize,
        day: u8,
        seconds: u16,
    }

    fn session() -> impl Strategy<Value = Session> {
        // `seconds` reaches below 30 on purpose: a sub-minute session is the
        // input that separates "measured zero" from "absent", and a strategy
        // that only produced comfortable durations would never generate the
        // case this property exists for.
        (0usize..3, 0u8..20, 0u16..2400).prop_map(|(book, day, seconds)| Session {
            book,
            day,
            seconds,
        })
    }

    fn md5_of(book: usize) -> String {
        format!("{book:032x}")
    }

    /// Write a statistics database holding exactly these sessions.
    async fn device(dir: &Path, sessions: &[Session]) -> PathBuf {
        let path = dir.join(STATS_DB_NAME);
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let mut conn = SqliteConnection::connect_with(&opts).await.unwrap();
        sqlx::raw_sql(DDL).execute(&mut conn).await.unwrap();
        for book in 0..3 {
            sqlx::query("INSERT INTO book (id, title, md5) VALUES (?, ?, ?)")
                .bind(book as i64 + 1)
                .bind(format!("book {book}"))
                .bind(md5_of(book))
                .execute(&mut conn)
                .await
                .unwrap();
        }
        for (n, s) in sessions.iter().enumerate() {
            // `page` is the row index so the UNIQUE constraint never fires;
            // this property is about durations, not about page counting.
            sqlx::query(
                "INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages)
                 VALUES (?, ?, ?, ?, 300)",
            )
            .bind(s.book as i64 + 1)
            .bind(n as i64)
            .bind(BASE + i64::from(s.day) * DAY + 3_600 + n as i64)
            .bind(i64::from(s.seconds))
            .execute(&mut conn)
            .await
            .unwrap();
        }
        conn.close().await.unwrap();
        path
    }

    async fn library(storage: &Storage) {
        for book in 0..3 {
            let id = storage
                .upsert_book(&Book {
                    title: Some(format!("book {book}")),
                    ..Default::default()
                })
                .await
                .unwrap();
            storage
                .link_device_book(&md5_of(book), id, LinkedBy::Auto)
                .await
                .unwrap();
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    proptest! {
        /// **Absent is not zero, and a measured zero is not absent.**
        ///
        /// A property rather than examples because the failure is one
        /// `unwrap_or(0)` away and reads as perfectly correct in every example
        /// where the device happens to have recorded a comfortable number of
        /// minutes. The two directions are asserted together on purpose: it is
        /// easy to satisfy either alone, and the interesting inputs are the
        /// ones where a whole period consists of sessions too short to round up
        /// to a minute — there the honest answer is `Some(0)`, and both
        /// `None` and a fabricated positive number are wrong.
        ///
        /// The expected value is recomputed from the generated sessions rather
        /// than from the database, so it shares nothing with the aggregate
        /// under test but the rounding rule itself.
        #[test]
        fn minutes_are_absent_exactly_when_the_device_measured_nothing(
            sessions in proptest::collection::vec(session(), 0..12),
            (a, b) in (0u8..20, 0u8..20),
        ) {
            let (from, to) = (a.min(b), a.max(b));
            rt().block_on(async {
                let tmp = tempfile::tempdir().unwrap();
                let storage = Storage::connect("sqlite::memory:").await.unwrap();
                library(&storage).await;
                let db = device(tmp.path(), &sessions).await;

                import_statistics(&storage, &db).await.unwrap();

                let range = DayRange::new(
                    &day_of_unix(BASE + i64::from(from) * DAY).unwrap(),
                    &day_of_unix(BASE + i64::from(to) * DAY).unwrap(),
                ).unwrap();

                // What the device said, grouped the way an event is: per
                // (book, day), rounded once, then summed.
                let mut per_day: BTreeMap<(usize, u8), i64> = BTreeMap::new();
                for s in &sessions {
                    if s.day >= from && s.day <= to {
                        *per_day.entry((s.book, s.day)).or_default() += i64::from(s.seconds);
                    }
                }
                let want: Option<i64> = if per_day.is_empty() {
                    None
                } else {
                    Some(per_day.values().map(|&sec| minutes_of(sec)).sum())
                };

                let sum = storage.activity_summary(&range).await.unwrap();
                prop_assert_eq!(sum.minutes, want);

                // The claim spelled out, so a regression names itself.
                if per_day.is_empty() {
                    prop_assert_eq!(
                        sum.minutes, None,
                        "a period the device never measured must not report zero"
                    );
                } else {
                    prop_assert!(
                        sum.minutes.is_some(),
                        "a period the device did measure must report a number, \
                         even when every session was too short to round up"
                    );
                }

                // Days are events; a measured day exists however short it was.
                prop_assert_eq!(sum.activity_days, {
                    let mut days: Vec<u8> = per_day.keys().map(|&(_, d)| d).collect();
                    days.sort_unstable();
                    days.dedup();
                    days.len() as i64
                });
                Ok(())
            })?;
        }
    }
}
