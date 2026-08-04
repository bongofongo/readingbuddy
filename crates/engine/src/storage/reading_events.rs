//! The source-agnostic activity log (migration `0011`), its fillers, and the
//! first reading-stats aggregates in the engine.
//!
//! The problem, stated plainly: reading-time data is KOReader-only, and a
//! surface built directly on it opens to blanks for a reader whose library came
//! from a Goodreads CSV. So no source is consumed in its own shape. Every source
//! writes `(book, day, source) → minutes?, pages?, confidence`, and a filler
//! that lands later — KOReader's `statistics.sqlite3`, a locally-read PDF —
//! changes no query and no view.
//!
//! **Three fillers exist today**, and all three run without touching a device:
//!
//! | filler | day from | source | confidence |
//! |---|---|---|---|
//! | [`Storage::fill_events_from_highlights`] | `highlights.ko_datetime` | `koreader` | inferred |
//! | [`Storage::fill_events_from_notes`] | `notes.created_at` | `vault` | measured |
//! | [`Storage::fill_events_from_readings`] | `readings.started_at`/`finished_at` | the reading's own | measured |
//!
//! The vault filler is the one worth dwelling on: notes, reflections, reviews
//! and citations are data readingbuddy fully originates, so no importer can fail
//! to supply them, and they are the right signal for an app positioned as the
//! desk rather than the reader.
//!
//! **Nothing here invents a day.** A highlight with no `ko_datetime`, or with
//! one SQLite cannot read as a date, produces no event at all; a reading with no
//! `started_at` contributes only the endpoint it has.
//!
//! **Every aggregate can say it does not know.** `minutes` and `pages` come back
//! `None` when no event in the period carried one — *not* `Some(0)`, which is a
//! claim, and the same discipline as `goodreads_for` returning `None` rather
//! than rounding. The counts are `i64` because they count evidence the engine
//! fully originates (its own readings, its own notes, its own edges), so zero
//! there is knowable rather than assumed. None of them counts something the user
//! has not done, and none of them is a run of consecutive days: `docs/decisions.md`
//! bans task-completion framing by name, and a streak is the shape of it that
//! looks most like a feature.

use sqlx::{Row, Sqlite, Transaction};

use super::{Storage, now_unix};
use crate::error::{EngineError, Result};

/// Events derived from device sidecars — the highlight filler today,
/// `statistics.sqlite3` (item 31) later, on the same rows.
pub const SOURCE_KOREADER: &str = "koreader";
/// Events derived from the vault: notes, reflections, reviews.
pub const SOURCE_VAULT: &str = "vault";

/// How much the row is willing to claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Something recorded it: our own timestamp, a device's clock, a date the
    /// user typed.
    Measured,
    /// Derived. A highlight dated that day means you were in the book; it does
    /// not mean anyone measured anything.
    Inferred,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Measured => "measured",
            Confidence::Inferred => "inferred",
        }
    }

    /// Read one back. An unrecognised token reads as `Inferred` on purpose: the
    /// vocabulary lives in a comment rather than a `CHECK` so that it can grow,
    /// and of the two ways to be wrong about a token we have never seen,
    /// claiming a measurement nobody made is the one that matters.
    fn from_db(s: &str) -> Confidence {
        match s {
            "measured" => Confidence::Measured,
            _ => Confidence::Inferred,
        }
    }
}

/// One day of one book, as one source saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingEvent {
    pub book_id: i64,
    /// Which read this day belongs to, where the evidence agrees on one.
    pub reading_id: Option<i64>,
    /// `YYYY-MM-DD`, UTC.
    pub day: String,
    /// Minutes read. `None` is "not known", never zero.
    pub minutes: Option<i64>,
    /// Pages turned. `None` is "not known", never zero.
    pub pages: Option<i64>,
    pub source: String,
    pub confidence: Confidence,
    pub created_at: i64,
}

/// What a filler writes. The seam items 22 and 31 come in through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReadingEvent {
    pub book_id: i64,
    pub reading_id: Option<i64>,
    pub day: String,
    pub minutes: Option<i64>,
    pub pages: Option<i64>,
    pub source: String,
    pub confidence: Confidence,
}

/// What one filler pass did.
///
/// `updated` counts rows that **actually changed**, not rows the upsert
/// re-wrote: the `DO UPDATE` carries a `WHERE` so an unchanged row is not
/// touched at all. Without that, every refill would report the whole table as
/// updated and idempotency could never be observed — the same trap the tier-2
/// corpus fell into, and the same fix `refresh_device_fields` uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FillStats {
    pub inserted: u64,
    pub updated: u64,
}

/// One pass of every filler that needs no device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefillReport {
    pub highlights: FillStats,
    pub notes: FillStats,
    pub readings: FillStats,
}

impl RefillReport {
    pub fn inserted(&self) -> u64 {
        self.highlights.inserted + self.notes.inserted + self.readings.inserted
    }

    pub fn updated(&self) -> u64 {
        self.highlights.updated + self.notes.updated + self.readings.updated
    }
}

/// An inclusive span of days, both ends `YYYY-MM-DD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayRange {
    from: String,
    to: String,
}

impl DayRange {
    /// Both ends inclusive. Refuses anything that is not a `YYYY-MM-DD` day and
    /// refuses an inverted span — a backwards range would otherwise select
    /// nothing and every aggregate would report a confident, wrong zero.
    pub fn new(from: &str, to: &str) -> Result<DayRange> {
        for d in [from, to] {
            if !is_day(d) {
                return Err(EngineError::InvalidInput(format!(
                    "{d:?} is not a YYYY-MM-DD day"
                )));
            }
        }
        if from > to {
            return Err(EngineError::InvalidInput(format!(
                "the range {from}..{to} ends before it starts"
            )));
        }
        Ok(DayRange {
            from: from.to_string(),
            to: to.to_string(),
        })
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    pub fn to(&self) -> &str {
        &self.to
    }
}

/// What is known about a period. Read the `Option`s as "we have no data",
/// never as zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivitySummary {
    pub range: DayRange,
    /// Readings closed inside the period. Ours, so a zero here is knowable.
    pub books_finished: i64,
    /// Days the period holds at least one event, from any source.
    pub activity_days: i64,
    pub notes_created: i64,
    /// Wikilink edges belonging to notes created in the period.
    ///
    /// **Attributed to the note, because `note_links` carries no timestamp of
    /// its own** and `set_note_links` replaces a note's whole edge set on every
    /// save, so an edge's own creation date is not recorded anywhere and adding
    /// a column would only record the date of the last edit to the note.
    pub links_created: i64,
    /// `None` when nothing in the period measured minutes. A month with no
    /// device data has no minutes; it does not have zero of them.
    pub minutes: Option<i64>,
    /// `None` when nothing in the period measured pages.
    pub pages: Option<i64>,
}

/// One day of the period, for a caller that wants the days and not just how
/// many there were. Only days carrying an event appear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayActivity {
    pub day: String,
    /// Distinct books with an event that day.
    pub books: i64,
    pub minutes: Option<i64>,
    pub pages: Option<i64>,
}

/// `YYYY-MM-DD`, UTC, from unix seconds.
pub fn day_of_unix(ts: i64) -> Result<String> {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    Ok(time::OffsetDateTime::from_unix_timestamp(ts)
        .map_err(|e| EngineError::InvalidInput(format!("timestamp {ts} is out of range: {e}")))?
        .format(fmt)?)
}

/// The day out of a KOReader `YYYY-MM-DD HH:MM:SS` stamp. `None` rather than a
/// guess when the stamp is not one — no source invents a day.
///
/// Taken from the text rather than by way of a unix timestamp because the device
/// writes local wall-clock with no zone, which `ko_datetime_to_unix` reads as
/// UTC; going through the number and back is the identity, and doing it in two
/// places is how the two would eventually disagree.
pub fn day_of_ko_datetime(s: &str) -> Option<String> {
    let s = s.trim();
    let day = s.get(..10)?;
    is_day(day).then(|| day.to_string())
}

/// Shape *and* validity: `2026-13-45` has the shape and is not a day.
fn is_day(s: &str) -> bool {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    time::Date::parse(s, fmt).is_ok()
}

/// The merge, written once because three fillers share it.
///
/// **No-clobber, the provider pattern rather than the device's straight
/// assignment**, and chosen for the reason `calibre.rs` states: the pattern
/// follows whether the record is *complete*, not who owns it. Each filler is a
/// partial view of the day — the highlight filler knows nothing about minutes,
/// the statistics filler will know nothing about the vault — so a NULL from one
/// of them means "no opinion" and must not erase another's answer.
///
/// `confidence` is the one field that is not a `COALESCE`: it ratchets to
/// `measured` and never back. That makes the merge order-independent, which is
/// what makes running the fillers in any order converge on the same table.
const EVENT_MERGE: &str = "
    reading_id = COALESCE(excluded.reading_id, reading_events.reading_id),
    minutes    = COALESCE(excluded.minutes,    reading_events.minutes),
    pages      = COALESCE(excluded.pages,      reading_events.pages),
    confidence = CASE WHEN 'measured' IN (excluded.confidence, reading_events.confidence)
                      THEN 'measured' ELSE reading_events.confidence END";

/// Whether [`EVENT_MERGE`] would change anything — the `WHERE` on the
/// `DO UPDATE`, so that `rows_affected` means "changed" rather than "seen".
/// `IS NOT` because these columns are nullable and `<>` is not null-safe.
const EVENT_DIFFERS: &str = "
       reading_events.reading_id IS NOT COALESCE(excluded.reading_id, reading_events.reading_id)
    OR reading_events.minutes    IS NOT COALESCE(excluded.minutes,    reading_events.minutes)
    OR reading_events.pages      IS NOT COALESCE(excluded.pages,      reading_events.pages)
    OR (excluded.confidence = 'measured' AND reading_events.confidence <> 'measured')";

const EVENT_COLUMNS: &str =
    "book_id, reading_id, day, minutes, pages, source, confidence, created_at";

fn row_to_event(r: &sqlx::sqlite::SqliteRow) -> ReadingEvent {
    let confidence: String = r.get("confidence");
    ReadingEvent {
        book_id: r.get("book_id"),
        reading_id: r.get("reading_id"),
        day: r.get("day"),
        minutes: r.get("minutes"),
        pages: r.get("pages"),
        source: r.get("source"),
        confidence: Confidence::from_db(&confidence),
        created_at: r.get("created_at"),
    }
}

/// Count the rows so a filler can report inserts and changes apart.
///
/// `rows_affected` on an upsert cannot tell the two apart, and a report that
/// says "wrote 400 rows" every single time is a report in which idempotency is
/// invisible.
async fn count(tx: &mut Transaction<'_, Sqlite>) -> Result<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM reading_events")
            .fetch_one(&mut **tx)
            .await?,
    )
}

/// Run one filler's `INSERT … SELECT … ON CONFLICT` and split its effect.
async fn fill(tx: &mut Transaction<'_, Sqlite>, sql: &str, now: i64) -> Result<FillStats> {
    let before = count(tx).await?;
    let touched = sqlx::query(sql).bind(now).execute(&mut **tx).await?;
    let after = count(tx).await?;
    let inserted = (after - before).max(0) as u64;
    Ok(FillStats {
        inserted,
        updated: touched.rows_affected().saturating_sub(inserted),
    })
}

impl Storage {
    /// Every filler that needs no device, in one transaction.
    ///
    /// One transaction because a half-filled log is a log whose aggregates
    /// disagree with each other, and because the three fillers overlap: a
    /// KOReader-sourced reading and its own highlights land on the same rows.
    ///
    /// Idempotent by construction, and it is the property this table lives on:
    /// a second call with nothing changed upstream reports `inserted == 0` and
    /// `updated == 0`.
    #[tracing::instrument(skip(self))]
    pub async fn refill_reading_events(&self) -> Result<RefillReport> {
        let now = now_unix();
        let mut tx = self.pool().begin().await?;
        let report = RefillReport {
            highlights: fill(&mut tx, &highlight_fill_sql(), now).await?,
            notes: fill(&mut tx, &note_fill_sql(), now).await?,
            readings: fill(&mut tx, &reading_fill_sql(), now).await?,
        };
        tx.commit().await?;
        tracing::debug!(
            inserted = report.inserted(),
            updated = report.updated(),
            "reading events refilled"
        );
        Ok(report)
    }

    /// The highlight filler alone. A day you were in the book, **inferred**:
    /// nobody measured anything, the device merely stamped a highlight.
    pub async fn fill_events_from_highlights(&self) -> Result<FillStats> {
        let mut tx = self.pool().begin().await?;
        let stats = fill(&mut tx, &highlight_fill_sql(), now_unix()).await?;
        tx.commit().await?;
        Ok(stats)
    }

    /// The vault filler alone.
    ///
    /// The filler worth dwelling on: notes, reflections and reviews are data
    /// readingbuddy fully originates, so this one cannot come back empty because
    /// an importer let us down, and it is the signal an app positioned as the
    /// desk should be able to show.
    pub async fn fill_events_from_notes(&self) -> Result<FillStats> {
        let mut tx = self.pool().begin().await?;
        let stats = fill(&mut tx, &note_fill_sql(), now_unix()).await?;
        tx.commit().await?;
        Ok(stats)
    }

    /// The reading-endpoint filler alone: the days a read opened and closed.
    ///
    /// `source` is the **reading's own** — a Goodreads CSV's `Date Read` lands
    /// as `goodreads`, a locally-opened read as whatever opened it — because
    /// `source` names where the fact came from, and this is the only filler that
    /// knows something about that beyond its own name.
    pub async fn fill_events_from_readings(&self) -> Result<FillStats> {
        let mut tx = self.pool().begin().await?;
        let stats = fill(&mut tx, &reading_fill_sql(), now_unix()).await?;
        tx.commit().await?;
        Ok(stats)
    }

    /// Write events directly. The seam for a source that is not derivable from
    /// what is already in the database — item 22's typed page, item 31's
    /// measured minutes.
    ///
    /// Same merge as the fillers, so a hand-written event and a derived one
    /// about the same day combine rather than fight.
    pub async fn record_reading_events(&self, events: &[NewReadingEvent]) -> Result<FillStats> {
        let now = now_unix();
        let mut tx = self.pool().begin().await?;
        let mut stats = FillStats::default();
        for e in events {
            if !is_day(&e.day) {
                return Err(EngineError::InvalidInput(format!(
                    "{:?} is not a YYYY-MM-DD day",
                    e.day
                )));
            }
            let before = count(&mut tx).await?;
            let touched = sqlx::query(&format!(
                "INSERT INTO reading_events ({EVENT_COLUMNS})
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (book_id, day, source) DO UPDATE SET {EVENT_MERGE}
                 WHERE {EVENT_DIFFERS}"
            ))
            .bind(e.book_id)
            .bind(e.reading_id)
            .bind(&e.day)
            .bind(e.minutes)
            .bind(e.pages)
            .bind(&e.source)
            .bind(e.confidence.as_str())
            .bind(now)
            .execute(&mut *tx)
            .await?;
            let after = count(&mut tx).await?;
            let inserted = (after - before).max(0) as u64;
            stats.inserted += inserted;
            stats.updated += touched.rows_affected().saturating_sub(inserted);
        }
        tx.commit().await?;
        Ok(stats)
    }

    /// One book's log, oldest day first.
    pub async fn reading_events(&self, book_id: i64) -> Result<Vec<ReadingEvent>> {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM reading_events WHERE book_id = ?
             ORDER BY day ASC, source ASC"
        ))
        .bind(book_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(row_to_event).collect())
    }

    /// What is known about a period.
    ///
    /// Read `minutes` and `pages` as "we have no data" when they are `None`.
    /// `SUM` over rows that all hold NULL is NULL in SQLite, and over no rows at
    /// all it is NULL too, which is exactly the answer wanted in both cases —
    /// so the absence is carried by the database rather than reconstructed here.
    pub async fn activity_summary(&self, range: &DayRange) -> Result<ActivitySummary> {
        let (from, to) = (range.from(), range.to());

        let books_finished: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM readings
              WHERE finished_at IS NOT NULL
                AND date(finished_at, 'unixepoch') BETWEEN ? AND ?",
        )
        .bind(from)
        .bind(to)
        .fetch_one(self.pool())
        .await?;

        let activity_days: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT day) FROM reading_events WHERE day BETWEEN ? AND ?",
        )
        .bind(from)
        .bind(to)
        .fetch_one(self.pool())
        .await?;

        let notes_created: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM notes WHERE date(created_at, 'unixepoch') BETWEEN ? AND ?",
        )
        .bind(from)
        .bind(to)
        .fetch_one(self.pool())
        .await?;

        let links_created: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM note_links l JOIN notes n ON n.id = l.from_note
              WHERE date(n.created_at, 'unixepoch') BETWEEN ? AND ?",
        )
        .bind(from)
        .bind(to)
        .fetch_one(self.pool())
        .await?;

        let totals = sqlx::query(
            "SELECT sum(minutes) AS minutes, sum(pages) AS pages
               FROM reading_events WHERE day BETWEEN ? AND ?",
        )
        .bind(from)
        .bind(to)
        .fetch_one(self.pool())
        .await?;

        Ok(ActivitySummary {
            range: range.clone(),
            books_finished,
            activity_days,
            notes_created,
            links_created,
            minutes: totals.get("minutes"),
            pages: totals.get("pages"),
        })
    }

    /// The days of a period that carry an event, oldest first. The set behind
    /// [`ActivitySummary::activity_days`], for a caller that wants to show them
    /// rather than count them.
    pub async fn activity_by_day(&self, range: &DayRange) -> Result<Vec<DayActivity>> {
        let rows = sqlx::query(
            "SELECT day,
                    count(DISTINCT book_id) AS books,
                    sum(minutes)            AS minutes,
                    sum(pages)              AS pages
               FROM reading_events
              WHERE day BETWEEN ? AND ?
              GROUP BY day
              ORDER BY day ASC",
        )
        .bind(range.from())
        .bind(range.to())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| DayActivity {
                day: r.get("day"),
                books: r.get("books"),
                minutes: r.get("minutes"),
                pages: r.get("pages"),
            })
            .collect())
    }
}

/// `date()` is doing two jobs in every filler below: it turns a stamp into a
/// day, and it returns NULL for anything it cannot read as one — `2026-13-45`
/// included. The outer `WHERE day IS NOT NULL` is therefore the whole of "no
/// source invents a day".
///
/// The read attribution is `CASE WHEN count(DISTINCT …) = 1`: a day whose
/// evidence points at exactly one reading is attributed to it, a day that
/// straddles a reread is left NULL. `attribute_highlights` makes the same call
/// for the same reason — the alternative is picking one, and picking one is
/// wrong half the time with nothing on screen looking wrong.
fn upsert_wrapper(select: &str) -> String {
    format!(
        "INSERT INTO reading_events ({EVENT_COLUMNS})
         {select}
         ON CONFLICT (book_id, day, source) DO UPDATE SET {EVENT_MERGE}
         WHERE {EVENT_DIFFERS}"
    )
}

fn highlight_fill_sql() -> String {
    upsert_wrapper(
        "SELECT book_id,
                CASE WHEN count(DISTINCT reading_id) = 1 THEN min(reading_id) END,
                day, NULL, NULL, 'koreader', 'inferred', ?
           FROM (SELECT book_id, reading_id, date(ko_datetime) AS day
                   FROM highlights
                  WHERE ko_datetime IS NOT NULL)
          WHERE day IS NOT NULL
          GROUP BY book_id, day",
    )
}

fn note_fill_sql() -> String {
    // `notes.book_id` is nullable — a note whose book was deleted keeps its
    // prose and loses its anchor — and an event has nowhere to hang without one.
    upsert_wrapper(
        "SELECT book_id,
                CASE WHEN count(DISTINCT reading_id) = 1 THEN min(reading_id) END,
                day, NULL, NULL, 'vault', 'measured', ?
           FROM (SELECT book_id, reading_id, date(created_at, 'unixepoch') AS day
                   FROM notes
                  WHERE book_id IS NOT NULL)
          WHERE day IS NOT NULL
          GROUP BY book_id, day",
    )
}

fn reading_fill_sql() -> String {
    // Two endpoints, one row each, `UNION ALL` rather than two statements so a
    // read opened and closed on one day collapses into the single event it is.
    // Grouped by `source` as well, because this filler's source varies per row.
    upsert_wrapper(
        "SELECT book_id,
                CASE WHEN count(DISTINCT id) = 1 THEN min(id) END,
                day, NULL, NULL, source, 'measured', ?
           FROM (SELECT book_id, id, source, date(started_at,  'unixepoch') AS day
                   FROM readings WHERE started_at IS NOT NULL
                  UNION ALL
                 SELECT book_id, id, source, date(finished_at, 'unixepoch') AS day
                   FROM readings WHERE finished_at IS NOT NULL)
          WHERE day IS NOT NULL
          GROUP BY book_id, day, source",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::storage::{NewHighlight, NewNoteMeta};

    const DAY: i64 = 86_400;
    /// 2026-01-05 00:00:00 UTC.
    const JAN5: i64 = 1_767_571_200;

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(&Book {
                title: Some("Pachinko".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        (s, id)
    }

    fn hl(text: &str, when: Option<&str>) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{text}")),
            pos1: None,
            ko_datetime: when.map(str::to_string),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    async fn note_on(s: &Storage, book: i64, title: &str, at: i64) -> i64 {
        let path = format!("vault/{title}.md");
        let id = s
            .insert_note(
                NewNoteMeta {
                    book_id: Some(book),
                    highlight_id: None,
                    reading_id: None,
                    file_path: &path,
                    title,
                    kind: "note",
                    page: None,
                    location: None,
                },
                "",
                &[],
            )
            .await
            .unwrap();
        sqlx::query("UPDATE notes SET created_at = ? WHERE id = ?")
            .bind(at)
            .bind(id)
            .execute(s.pool())
            .await
            .unwrap();
        id
    }

    async fn days(s: &Storage, book: i64) -> Vec<(String, String)> {
        s.reading_events(book)
            .await
            .unwrap()
            .into_iter()
            .map(|e| (e.day, e.source))
            .collect()
    }

    /// Every row, in a comparable shape, so "the table did not move" is one
    /// assertion rather than seven.
    async fn snapshot(s: &Storage) -> Vec<ReadingEvent> {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM reading_events ORDER BY book_id, day, source"
        ))
        .fetch_all(s.pool())
        .await
        .unwrap();
        rows.iter().map(row_to_event).collect()
    }

    // ---- the day encoding -------------------------------------------------

    #[test]
    fn a_day_is_read_off_a_ko_stamp_and_never_guessed() {
        assert_eq!(
            day_of_ko_datetime("2026-01-05 21:14:08"),
            Some("2026-01-05".into())
        );
        assert_eq!(
            day_of_ko_datetime("  2026-01-05 21:14:08 "),
            Some("2026-01-05".into())
        );
        assert_eq!(day_of_ko_datetime("not a date"), None);
        assert_eq!(day_of_ko_datetime(""), None);
        assert_eq!(day_of_ko_datetime("2026-13-45 00:00:00"), None);
        assert_eq!(day_of_unix(JAN5).unwrap(), "2026-01-05");
        // The two agree, which is what stops an event and the reading it was
        // attributed to landing on different days.
        assert_eq!(
            day_of_ko_datetime("2026-01-05 21:14:08").unwrap(),
            day_of_unix(super::super::ko_datetime_to_unix("2026-01-05 21:14:08").unwrap()).unwrap()
        );
    }

    #[test]
    fn a_range_refuses_junk_and_refuses_to_run_backwards() {
        assert!(DayRange::new("2026-01-01", "2026-01-31").is_ok());
        assert!(DayRange::new("2026-01-01", "2026-01-01").is_ok());
        assert!(DayRange::new("2026-1-1", "2026-01-31").is_err());
        assert!(DayRange::new("2026-02-30", "2026-03-31").is_err());
        assert!(
            DayRange::new("2026-01-31", "2026-01-01").is_err(),
            "a backwards range selects nothing, and every aggregate would then \
             report a confident zero"
        );
    }

    // ---- the fillers ------------------------------------------------------

    #[tokio::test]
    async fn the_highlight_filler_makes_one_inferred_day_per_book_day() {
        let (s, book) = seeded().await;
        for h in [
            hl("a", Some("2026-01-05 08:00:00")),
            hl("b", Some("2026-01-05 21:14:08")),
            hl("c", Some("2026-01-07 09:00:00")),
        ] {
            s.insert_highlight(book, &h).await.unwrap();
        }
        let stats = s.fill_events_from_highlights().await.unwrap();
        assert_eq!(stats.inserted, 2, "two days, not three highlights");
        assert_eq!(stats.updated, 0);

        let events = s.reading_events(book).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].day, "2026-01-05");
        assert_eq!(events[0].source, SOURCE_KOREADER);
        assert_eq!(events[0].confidence, Confidence::Inferred);
        assert_eq!(events[0].minutes, None, "a highlight measures no minutes");
        assert_eq!(events[0].pages, None);
    }

    #[tokio::test]
    async fn a_highlight_without_a_readable_day_produces_no_event() {
        let (s, book) = seeded().await;
        for h in [
            hl("no stamp", None),
            hl("junk", Some("sometime last winter")),
            hl("impossible", Some("2026-13-45 00:00:00")),
            hl("real", Some("2026-01-05 08:00:00")),
        ] {
            s.insert_highlight(book, &h).await.unwrap();
        }
        s.fill_events_from_highlights().await.unwrap();
        assert_eq!(
            days(&s, book).await,
            vec![("2026-01-05".into(), "koreader".into())],
            "three unusable stamps invent no days between them"
        );
    }

    #[tokio::test]
    async fn the_vault_filler_is_measured_and_needs_no_importer() {
        let (s, book) = seeded().await;
        note_on(&s, book, "first", JAN5 + 3600).await;
        note_on(&s, book, "second", JAN5 + 7200).await;
        note_on(&s, book, "later", JAN5 + 3 * DAY).await;

        let stats = s.fill_events_from_notes().await.unwrap();
        assert_eq!(stats.inserted, 2, "two days, three notes");
        let events = s.reading_events(book).await.unwrap();
        assert_eq!(events[0].day, "2026-01-05");
        assert_eq!(events[0].source, SOURCE_VAULT);
        assert_eq!(
            events[0].confidence,
            Confidence::Measured,
            "our own timestamp on our own file"
        );
    }

    #[tokio::test]
    async fn the_reading_filler_carries_the_readings_own_source() {
        let (s, book) = seeded().await;
        s.record_reading(
            book,
            Some(JAN5),
            Some(JAN5 + 5 * DAY),
            "finished",
            "goodreads",
        )
        .await
        .unwrap();

        s.fill_events_from_readings().await.unwrap();
        let events = s.reading_events(book).await.unwrap();
        assert_eq!(events.len(), 2, "an opening day and a closing day");
        assert_eq!(events[0].day, "2026-01-05");
        assert_eq!(events[1].day, "2026-01-10");
        assert!(
            events.iter().all(|e| e.source == "goodreads"),
            "a Goodreads CSV's dates are Goodreads', not this filler's"
        );
        assert!(events.iter().all(|e| e.confidence == Confidence::Measured));
        assert!(
            events.iter().all(|e| e.reading_id.is_some()),
            "one reading owns both endpoints, so both are attributable"
        );
    }

    #[tokio::test]
    async fn a_read_opened_and_closed_in_one_day_is_one_event() {
        let (s, book) = seeded().await;
        s.record_reading(book, Some(JAN5), Some(JAN5 + 3600), "finished", "manual")
            .await
            .unwrap();
        s.fill_events_from_readings().await.unwrap();
        assert_eq!(
            days(&s, book).await,
            vec![("2026-01-05".into(), "manual".into())]
        );
    }

    #[tokio::test]
    async fn a_day_straddling_two_reads_is_left_unattributed() {
        let (s, book) = seeded().await;
        // Two reads that both close on the same day is the shape; the second's
        // start is what puts two ids on one day.
        s.record_reading(
            book,
            Some(JAN5 - 10 * DAY),
            Some(JAN5),
            "finished",
            "manual",
        )
        .await
        .unwrap();
        s.record_reading(book, Some(JAN5), Some(JAN5 + DAY), "finished", "manual")
            .await
            .unwrap();
        s.fill_events_from_readings().await.unwrap();

        let on_jan5 = s
            .reading_events(book)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.day == "2026-01-05")
            .unwrap();
        assert_eq!(
            on_jan5.reading_id, None,
            "picking one of two reads is wrong half the time, and looks right"
        );
    }

    // ---- idempotency and the merge ----------------------------------------

    #[tokio::test]
    async fn a_second_refill_changes_nothing() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("a", Some("2026-01-05 08:00:00")))
            .await
            .unwrap();
        note_on(&s, book, "n", JAN5 + 3600).await;
        s.record_reading(book, Some(JAN5), None, "reading", "koreader")
            .await
            .unwrap();

        let first = s.refill_reading_events().await.unwrap();
        assert!(first.inserted() > 0);
        let before = snapshot(&s).await;

        let second = s.refill_reading_events().await.unwrap();
        assert_eq!(second.inserted(), 0, "a refill duplicates nothing");
        assert_eq!(
            second.updated(),
            0,
            "and rewrites nothing either — otherwise idempotency is unobservable"
        );
        assert_eq!(snapshot(&s).await, before);
    }

    #[tokio::test]
    async fn one_koreader_day_holds_both_the_highlight_and_the_endpoint() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("a", Some("2026-01-05 08:00:00")))
            .await
            .unwrap();
        s.record_reading(book, Some(JAN5), None, "reading", "koreader")
            .await
            .unwrap();

        s.fill_events_from_highlights().await.unwrap();
        let inferred = s.reading_events(book).await.unwrap();
        assert_eq!(inferred.len(), 1);
        assert_eq!(inferred[0].confidence, Confidence::Inferred);

        let stats = s.fill_events_from_readings().await.unwrap();
        assert_eq!(stats.inserted, 0, "same book, same day, same source");
        assert_eq!(stats.updated, 1);
        let merged = s.reading_events(book).await.unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].confidence,
            Confidence::Measured,
            "confidence ratchets up when a measured filler reaches the same day"
        );
        assert!(merged[0].reading_id.is_some());
    }

    #[tokio::test]
    async fn a_recorded_event_merges_with_a_derived_one_rather_than_fighting_it() {
        let (s, book) = seeded().await;
        s.insert_highlight(book, &hl("a", Some("2026-01-05 08:00:00")))
            .await
            .unwrap();
        s.fill_events_from_highlights().await.unwrap();

        // What item 31 will do: the same source, the same day, with numbers.
        let stats = s
            .record_reading_events(&[NewReadingEvent {
                book_id: book,
                reading_id: None,
                day: "2026-01-05".into(),
                minutes: Some(42),
                pages: Some(19),
                source: SOURCE_KOREADER.into(),
                confidence: Confidence::Measured,
            }])
            .await
            .unwrap();
        assert_eq!(stats.inserted, 0);
        assert_eq!(stats.updated, 1);

        let e = &s.reading_events(book).await.unwrap()[0];
        assert_eq!(e.minutes, Some(42));
        assert_eq!(e.pages, Some(19));
        assert_eq!(e.confidence, Confidence::Measured);

        // And the filler that knows nothing about minutes does not erase them.
        s.fill_events_from_highlights().await.unwrap();
        assert_eq!(s.reading_events(book).await.unwrap()[0].minutes, Some(42));
    }

    #[tokio::test]
    async fn deleting_a_reading_keeps_the_days_it_explained() {
        let (s, book) = seeded().await;
        let r = s
            .record_reading(book, Some(JAN5), Some(JAN5 + DAY), "finished", "manual")
            .await
            .unwrap();
        s.fill_events_from_readings().await.unwrap();
        sqlx::query("DELETE FROM readings WHERE id = ?")
            .bind(r)
            .execute(s.pool())
            .await
            .unwrap();

        let events = s.reading_events(book).await.unwrap();
        assert_eq!(events.len(), 2, "the days survive the read");
        assert!(events.iter().all(|e| e.reading_id.is_none()));
    }

    #[tokio::test]
    async fn deleting_a_book_takes_its_events() {
        let (s, book) = seeded().await;
        note_on(&s, book, "n", JAN5).await;
        s.refill_reading_events().await.unwrap();
        assert!(!s.reading_events(book).await.unwrap().is_empty());
        s.delete_book(book).await.unwrap();
        assert!(s.reading_events(book).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_recorded_event_refuses_a_day_that_is_not_one() {
        let (s, book) = seeded().await;
        let bad = NewReadingEvent {
            book_id: book,
            reading_id: None,
            day: "yesterday".into(),
            minutes: Some(10),
            pages: None,
            source: "local".into(),
            confidence: Confidence::Measured,
        };
        assert!(matches!(
            s.record_reading_events(&[bad]).await,
            Err(EngineError::InvalidInput(_))
        ));
    }

    // ---- the aggregates ---------------------------------------------------

    #[tokio::test]
    async fn a_period_with_no_device_data_has_no_minutes_rather_than_zero() {
        let (s, book) = seeded().await;
        note_on(&s, book, "n", JAN5 + 3600).await;
        s.refill_reading_events().await.unwrap();

        let jan = DayRange::new("2026-01-01", "2026-01-31").unwrap();
        let sum = s.activity_summary(&jan).await.unwrap();
        assert_eq!(sum.activity_days, 1, "the vault knows you were here");
        assert_eq!(sum.notes_created, 1);
        assert_eq!(
            sum.minutes, None,
            "nothing measured minutes, and zero would be a claim"
        );
        assert_eq!(sum.pages, None);

        // An empty period is the same answer, not a different one.
        let feb = DayRange::new("2026-02-01", "2026-02-28").unwrap();
        let sum = s.activity_summary(&feb).await.unwrap();
        assert_eq!(sum.activity_days, 0);
        assert_eq!(sum.minutes, None);
    }

    #[tokio::test]
    async fn a_measured_zero_is_not_an_absent_one() {
        let (s, book) = seeded().await;
        s.record_reading_events(&[NewReadingEvent {
            book_id: book,
            reading_id: None,
            day: "2026-01-05".into(),
            // A device that says you opened the book and turned no pages is
            // saying something; it is not saying nothing.
            minutes: Some(3),
            pages: Some(0),
            source: SOURCE_KOREADER.into(),
            confidence: Confidence::Measured,
        }])
        .await
        .unwrap();

        let jan = DayRange::new("2026-01-01", "2026-01-31").unwrap();
        let sum = s.activity_summary(&jan).await.unwrap();
        assert_eq!(sum.pages, Some(0));
        assert_ne!(sum.pages, None);
        assert_eq!(sum.minutes, Some(3));
    }

    #[tokio::test]
    async fn the_named_aggregates_count_what_the_period_holds() {
        let (s, book) = seeded().await;
        s.record_reading(book, Some(JAN5), Some(JAN5 + 5 * DAY), "finished", "manual")
            .await
            .unwrap();
        let a = note_on(&s, book, "alpha", JAN5 + 3600).await;
        note_on(&s, book, "beta", JAN5 + 2 * DAY).await;
        s.set_note_links(a, "alpha", &["beta".into(), "gamma".into()])
            .await
            .unwrap();
        s.refill_reading_events().await.unwrap();

        let jan = DayRange::new("2026-01-01", "2026-01-31").unwrap();
        let sum = s.activity_summary(&jan).await.unwrap();
        assert_eq!(sum.books_finished, 1);
        assert_eq!(sum.notes_created, 2);
        assert_eq!(sum.links_created, 2, "a resolved edge and a dangling one");
        assert_eq!(
            sum.activity_days, 3,
            "the two endpoints and the day of the second note"
        );

        let by_day = s.activity_by_day(&jan).await.unwrap();
        assert_eq!(
            by_day.iter().map(|d| d.day.as_str()).collect::<Vec<_>>(),
            vec!["2026-01-05", "2026-01-07", "2026-01-10"]
        );
        assert!(by_day.iter().all(|d| d.books == 1 && d.minutes.is_none()));

        // A period before any of it knows nothing, and says so the same way.
        let dec = DayRange::new("2025-12-01", "2025-12-31").unwrap();
        let sum = s.activity_summary(&dec).await.unwrap();
        assert_eq!(
            (sum.books_finished, sum.activity_days, sum.notes_created),
            (0, 0, 0)
        );
        assert_eq!(sum.minutes, None);
    }

    /// An indexes-only migration is judged on `EXPLAIN QUERY PLAN`, and this one
    /// carries two indexes whose whole job is a query shape. The day index is
    /// what stops every period aggregate scanning the log.
    #[tokio::test]
    async fn the_day_index_is_the_plan_the_planner_picks() {
        let (s, book) = seeded().await;
        for n in 0..40 {
            s.record_reading_events(&[NewReadingEvent {
                book_id: book,
                reading_id: None,
                day: format!("2026-01-{:02}", (n % 28) + 1),
                minutes: Some(n),
                pages: None,
                source: format!("s{n}"),
                confidence: Confidence::Measured,
            }])
            .await
            .unwrap();
        }
        sqlx::query("ANALYZE").execute(s.pool()).await.unwrap();

        let plan = sqlx::query(
            "EXPLAIN QUERY PLAN
             SELECT count(DISTINCT day) FROM reading_events WHERE day BETWEEN ? AND ?",
        )
        .bind("2026-01-01")
        .bind("2026-01-07")
        .fetch_all(s.pool())
        .await
        .unwrap()
        .iter()
        .map(|r| r.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("; ");
        assert!(
            plan.contains("idx_reading_events_day"),
            "the period aggregate must not scan the log: {plan}"
        );
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use crate::book::Book;
    use crate::storage::{NewHighlight, NewNoteMeta};
    use proptest::prelude::*;

    /// One piece of evidence to seed the library with. Deliberately a mixture:
    /// the interesting inputs are the ones where two fillers land on one day.
    #[derive(Debug, Clone)]
    enum Evidence {
        Highlight {
            book: usize,
            day: u8,
        },
        Note {
            book: usize,
            day: u8,
        },
        Reading {
            book: usize,
            start: u8,
            len: u8,
        },
        /// What item 31 will write. Included because the merge has to survive a
        /// filler it has never met.
        Measured {
            book: usize,
            day: u8,
            minutes: u8,
        },
    }

    fn evidence() -> impl Strategy<Value = Evidence> {
        prop_oneof![
            (0usize..3, 0u8..20).prop_map(|(book, day)| Evidence::Highlight { book, day }),
            (0usize..3, 0u8..20).prop_map(|(book, day)| Evidence::Note { book, day }),
            (0usize..3, 0u8..20, 0u8..5).prop_map(|(book, start, len)| Evidence::Reading {
                book,
                start,
                len
            }),
            (0usize..3, 0u8..20, 0u8..90).prop_map(|(book, day, minutes)| Evidence::Measured {
                book,
                day,
                minutes
            }),
        ]
    }

    const DAY: i64 = 86_400;
    /// 2026-01-01 00:00:00 UTC.
    const BASE: i64 = 1_767_225_600;

    async fn seed(s: &Storage, books: &[i64], ev: &[Evidence], n: &mut usize) {
        for e in ev {
            *n += 1;
            match *e {
                Evidence::Highlight { book, day } => {
                    let stamp = day_of_unix(BASE + i64::from(day) * DAY).unwrap();
                    s.insert_highlight(
                        books[book],
                        &NewHighlight {
                            text: format!("h{n}"),
                            chapter: None,
                            page: Some(1),
                            pos0: Some(format!("/p[{n}]")),
                            pos1: None,
                            ko_datetime: Some(format!("{stamp} 12:00:00")),
                            ko_datetime_updated: None,
                            color: None,
                            note: None,
                            source: "koreader".into(),
                        },
                    )
                    .await
                    .unwrap();
                }
                Evidence::Note { book, day } => {
                    let path = format!("vault/n{n}.md");
                    let title = format!("n{n}");
                    let id = s
                        .insert_note(
                            NewNoteMeta {
                                book_id: Some(books[book]),
                                highlight_id: None,
                                reading_id: None,
                                file_path: &path,
                                title: &title,
                                kind: "note",
                                page: None,
                                location: None,
                            },
                            "",
                            &[],
                        )
                        .await
                        .unwrap();
                    sqlx::query("UPDATE notes SET created_at = ? WHERE id = ?")
                        .bind(BASE + i64::from(day) * DAY + 3600)
                        .bind(id)
                        .execute(s.pool())
                        .await
                        .unwrap();
                }
                Evidence::Reading { book, start, len } => {
                    let from = BASE + i64::from(start) * DAY;
                    // `record_reading` is the door Goodreads import comes in
                    // through, and it takes closed readings without complaint.
                    s.record_reading(
                        books[book],
                        Some(from),
                        Some(from + i64::from(len) * DAY),
                        "finished",
                        "manual",
                    )
                    .await
                    .unwrap();
                }
                Evidence::Measured { book, day, minutes } => {
                    s.record_reading_events(&[NewReadingEvent {
                        book_id: books[book],
                        reading_id: None,
                        day: day_of_unix(BASE + i64::from(day) * DAY).unwrap(),
                        minutes: Some(i64::from(minutes)),
                        pages: None,
                        source: SOURCE_KOREADER.into(),
                        confidence: Confidence::Measured,
                    }])
                    .await
                    .unwrap();
                }
            }
        }
    }

    async fn library(ev: &[Evidence]) -> Storage {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut books = Vec::new();
        for n in 0..3 {
            books.push(
                s.upsert_book(&Book {
                    title: Some(format!("book {n}")),
                    ..Default::default()
                })
                .await
                .unwrap(),
            );
        }
        let mut n = 0;
        seed(&s, &books, ev, &mut n).await;
        s
    }

    async fn snapshot(s: &Storage) -> Vec<ReadingEvent> {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM reading_events ORDER BY book_id, day, source"
        ))
        .fetch_all(s.pool())
        .await
        .unwrap();
        rows.iter().map(row_to_event).collect()
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    proptest! {
        /// Refilling is idempotent, and the fillers commute.
        ///
        /// A property rather than more examples because the rule is general and
        /// the interesting input is the *overlap*: whichever evidence happens to
        /// share a `(book, day, source)` decides whether the merge is exercised
        /// at all, and three hand-picked libraries are three that happened to
        /// overlap the way the author expected. Order-independence is asserted
        /// beside repeat-stability because the merge only earns "run the
        /// fillers in any order" if `confidence` never ratchets back down.
        #[test]
        fn refilling_is_idempotent_and_order_independent(
            ev in proptest::collection::vec(evidence(), 0..14),
        ) {
            rt().block_on(async {
                let a = library(&ev).await;
                a.refill_reading_events().await.unwrap();
                let once = snapshot(&a).await;

                let again = a.refill_reading_events().await.unwrap();
                prop_assert_eq!(again.inserted(), 0, "a refill duplicated a row");
                prop_assert_eq!(again.updated(), 0, "a refill rewrote an unchanged row");
                prop_assert_eq!(snapshot(&a).await, once.clone());

                // The same evidence, the fillers run backwards, twice over.
                let b = library(&ev).await;
                for _ in 0..2 {
                    b.fill_events_from_readings().await.unwrap();
                    b.fill_events_from_notes().await.unwrap();
                    b.fill_events_from_highlights().await.unwrap();
                }
                let reversed = snapshot(&b).await;

                // `created_at` is a wall clock and the two libraries were built
                // moments apart; everything the fillers *decide* must agree.
                prop_assert_eq!(reversed.len(), once.len());
                for (x, y) in reversed.iter().zip(once.iter()) {
                    prop_assert_eq!(&x.day, &y.day);
                    prop_assert_eq!(&x.source, &y.source);
                    prop_assert_eq!(x.book_id, y.book_id);
                    prop_assert_eq!(x.reading_id, y.reading_id);
                    prop_assert_eq!(x.minutes, y.minutes);
                    prop_assert_eq!(x.pages, y.pages);
                    prop_assert_eq!(x.confidence, y.confidence);
                }
                Ok(())
            })?;
        }

        /// Absent is not zero, in both directions, over any library.
        ///
        /// `minutes` is `None` **exactly when** no event in the period carries
        /// one, and `Some(n)` is the sum of those that do — so a period whose
        /// every event came from the vault reports no minutes, while a period
        /// holding a single measured `0` reports `Some(0)`. A property because
        /// the failure is one `unwrap_or(0)` away and reads as correct in every
        /// example where some device data happens to exist.
        #[test]
        fn minutes_are_absent_exactly_when_nothing_measured_them(
            ev in proptest::collection::vec(evidence(), 0..14),
            (from, to) in (0u8..20, 0u8..20),
        ) {
            let (from, to) = (from.min(to), from.max(to));
            rt().block_on(async {
                let s = library(&ev).await;
                s.refill_reading_events().await.unwrap();

                let range = DayRange::new(
                    &day_of_unix(BASE + i64::from(from) * DAY).unwrap(),
                    &day_of_unix(BASE + i64::from(to) * DAY).unwrap(),
                ).unwrap();

                let events: Vec<ReadingEvent> = snapshot(&s).await
                    .into_iter()
                    .filter(|e| e.day.as_str() >= range.from() && e.day.as_str() <= range.to())
                    .collect();
                let measured: Vec<i64> = events.iter().filter_map(|e| e.minutes).collect();

                let sum = s.activity_summary(&range).await.unwrap();
                if measured.is_empty() {
                    prop_assert_eq!(sum.minutes, None, "zero is a claim this period cannot make");
                } else {
                    prop_assert_eq!(sum.minutes, Some(measured.iter().sum::<i64>()));
                }
                prop_assert_eq!(sum.pages, None, "no filler here supplies pages");

                // And the day count is the set of days, however many sources
                // spoke on each of them.
                let mut days: Vec<&str> = events.iter().map(|e| e.day.as_str()).collect();
                days.sort_unstable();
                days.dedup();
                prop_assert_eq!(sum.activity_days, days.len() as i64);
                prop_assert_eq!(
                    s.activity_by_day(&range).await.unwrap().len(),
                    days.len()
                );
                Ok(())
            })?;
        }
    }
}
