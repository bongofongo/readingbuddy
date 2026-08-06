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
//! **Two more fillers write through [`Storage::record_reading_events`]** rather
//! than deriving anything, because their evidence is not already in the
//! database: item 31's `statistics.sqlite3` minutes, and item 22's
//! [`Storage::record_typed_page`] — a page you typed *here*, `source = "local"`,
//! `confidence = measured`, filed by [`Storage::update_progress`] itself so that
//! no frontend has to remember to. Both are what the table was shaped for:
//! between them they added no query, no view and no line of any frontend.
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

use super::readings::READING_WINDOWS;
use super::{Storage, now_unix};
use crate::error::{EngineError, Result};

/// Events derived from device sidecars — the highlight filler today,
/// `statistics.sqlite3` (item 31) later, on the same rows.
pub const SOURCE_KOREADER: &str = "koreader";
/// Events derived from the vault: notes, reflections, reviews.
pub const SOURCE_VAULT: &str = "vault";
/// A page the user typed **here** — item 22's fifth filler, and the fourth
/// ownership row (*readingbuddy owns what you read here*) as a column value.
///
/// It is deliberately **not** the reading's own `source`, which
/// [`Storage::fill_events_from_readings`] uses for its two endpoints. The two
/// mean different things and both can be true of one day: a `koreader` reading
/// whose page you corrected by hand this afternoon carries a `koreader` row
/// saying the device opened this read and a `local` row saying you moved the
/// bookmark yourself. Folding the second into the first would attribute your
/// typing to the device; folding it into `manual` — the word `update_progress`
/// writes when it opens a reading — would attribute the *device's* readings to
/// your hand the moment you touched one. One word per claimant is what this
/// column is for, and the primary key `(book_id, day, source)` is what makes
/// two claimants two rows rather than a fight over one.
pub const SOURCE_LOCAL: &str = "local";

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

/// One month of the period, for a caller drawing a year rather than a week.
/// Only months carrying an event appear, exactly as [`DayActivity`] only
/// carries days that do — the gaps are the client's to draw, and drawing them
/// *as an absence* is the whole point.
///
/// **This is not [`DayActivity`] added up, and `books` is why.** Minutes and
/// days do sum; distinct books do not — a reader who opened the same two books
/// on twelve days read two books that month and not twenty-four — so a client
/// bucketing days into months in its own language either gets that field wrong
/// or cannot produce it at all. Which is a semantic decision, and semantic
/// decisions are the engine's (item 17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthActivity {
    /// `YYYY-MM`.
    pub month: String,
    /// Distinct books with an event **anywhere in the month**. Deliberately not
    /// derivable from the days.
    pub books: i64,
    /// Days of this month that carry an event. The month's own share of
    /// [`ActivitySummary::activity_days`], and the denominator a client would
    /// otherwise invent.
    pub activity_days: i64,
    /// `None` when nothing in the month measured minutes. **Never `Some(0)`** —
    /// a month with no device data has no minutes, it does not have zero of
    /// them, and collapsing the two is the lie the whole column exists to
    /// refuse.
    pub minutes: Option<i64>,
    /// `None` when nothing in the month measured pages.
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

    /// The fifth filler: a page the user typed here, on the day they typed it.
    ///
    /// Item 22. Called by [`Storage::update_progress`] and by nothing else, so
    /// every typed page lands in the activity log without a frontend having to
    /// remember to say so — which is what item 21's whole argument was for.
    ///
    /// Four calls are made here and each of them is a claim about honesty.
    ///
    /// **A delta needs two points.** The first page typed for a reading records
    /// the day and `pages: None`, because "you are on page 42" is not "you read
    /// 42 pages today" — you may have read it over a month, and this is the one
    /// place where the difference is a fabricated number rather than a missing
    /// one. Only a *subsequent* page yields a count.
    ///
    /// **Backwards is not negative.** Correcting 200 down to 190 is a fix, not
    /// minus ten pages; it contributes nothing and erases nothing.
    ///
    /// **The day accumulates rather than replaces.** [`EVENT_MERGE`] is
    /// `COALESCE`, so writing a second delta over the first would report the
    /// evening's twenty pages and lose the morning's ten. The running total for
    /// the day is read back and added to, which also makes a repeat of the same
    /// page a no-op — re-typing 190 adds zero and `EVENT_DIFFERS` then declines
    /// to touch the row at all.
    ///
    /// **`confidence` is `Measured`**, because a person typed it. That ratchet
    /// is one-way, so a `local` row can never fall back to `inferred`.
    pub async fn record_typed_page(
        &self,
        book_id: i64,
        reading_id: i64,
        previous_page: Option<i64>,
        page: i64,
        at: i64,
    ) -> Result<FillStats> {
        let day = day_of_unix(at)?;

        // Only a move forward from a page we already had is a number of pages.
        let delta = previous_page.map(|p| page - p).filter(|d| *d > 0);
        let pages = match delta {
            Some(d) => {
                let so_far: Option<i64> = sqlx::query_scalar(
                    "SELECT pages FROM reading_events
                      WHERE book_id = ? AND day = ? AND source = ?",
                )
                .bind(book_id)
                .bind(&day)
                .bind(SOURCE_LOCAL)
                .fetch_optional(self.pool())
                .await?
                .flatten();
                Some(so_far.unwrap_or(0) + d)
            }
            None => None,
        };

        self.record_reading_events(&[NewReadingEvent {
            book_id,
            // The reading we just wrote to, named outright. `reading_for_day`
            // exists for a source that has to *infer* which read a day belongs
            // to; this one was told.
            reading_id: Some(reading_id),
            day,
            // Nothing here measures time. A typed page says when and how far,
            // never for how long, and `Some(0)` would be a claim that somebody
            // read for no time at all.
            minutes: None,
            pages,
            source: SOURCE_LOCAL.to_string(),
            confidence: Confidence::Measured,
        }])
        .await
    }

    /// Which read a **day** belongs to, or `None` when the evidence does not
    /// settle on one.
    ///
    /// The day analogue of [`Storage::attribute_highlights`], and it goes
    /// through the *same* [`READING_WINDOWS`] rather than a second copy — the
    /// derivation of a missing `started_at` is the part that was wrong once
    /// already, and a reread must not silently collect an earlier read's
    /// minutes.
    ///
    /// Item 31 needs this because KOReader's `statistics.sqlite3` is **per
    /// file**: it knows how long you read, and nothing whatever about rereads.
    ///
    /// Two deliberate calls:
    ///
    /// * The test is **overlap**, not containment. A day is 86 400 seconds
    ///   wide and a reading can end at noon inside it; that read did hold part
    ///   of the day, and a containment test would attribute the whole day to
    ///   nobody every time a read ended mid-morning.
    /// * Overlapping *two* windows yields `None` rather than the later one.
    ///   `attribute_highlights` can prefer the later window because a highlight
    ///   is an instant and the tie is genuinely near-arbitrary; a day that
    ///   straddles a reread boundary is not a tie, it is a day whose minutes
    ///   belong to both reads and cannot be split at this grain.
    pub async fn reading_for_day(&self, book_id: i64, day: &str) -> Result<Option<i64>> {
        if !is_day(day) {
            return Err(EngineError::InvalidInput(format!(
                "{day:?} is not a YYYY-MM-DD day"
            )));
        }
        Ok(sqlx::query_scalar(&format!(
            "WITH windows AS ({READING_WINDOWS}),
                  span AS (SELECT CAST(strftime('%s', ?2 || ' 00:00:00') AS INTEGER) AS d0)
             SELECT CASE WHEN count(*) = 1 THEN min(w.reading_id) END
               FROM windows w, span
              WHERE w.win_start <= span.d0 + 86399 AND w.win_end >= span.d0"
        ))
        .bind(book_id)
        .bind(day)
        .fetch_one(self.pool())
        .await?)
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

    /// The months of a period that carry an event, oldest first.
    ///
    /// The same aggregate as [`Storage::activity_by_day`] one grain up, and it
    /// exists because the grain is **not** a client's to change. Bucketing days
    /// into months above this seam goes wrong twice: `minutes: None` collapses
    /// to `0` on the first `reduce` in any language that has one, turning "the
    /// device never measured this month" into "you read for zero minutes"; and
    /// `books` cannot be recovered at all, since distinct-books-per-day do not
    /// sum to distinct-books-per-month. The alternative — a summary call per
    /// month — is sixty round trips to draw five years.
    ///
    /// `substr(day, 1, 7)` is the whole grouping because `day` is a
    /// zero-padded ISO date, so its lexicographic order is its chronological
    /// order and its first seven characters are its month. That is the same
    /// property `BETWEEN` already relies on, used again rather than a second
    /// date function that could disagree with it.
    ///
    /// **A month at the edge of the range is reported for the part of it that
    /// is inside the range**, and is not silently widened: `2026-01-20 ..
    /// 2026-02-10` answers about twelve days of January and ten of February,
    /// because reporting a whole January would be answering about days the
    /// caller did not ask for and could not see in the same call's
    /// [`ActivitySummary`].
    pub async fn activity_by_month(&self, range: &DayRange) -> Result<Vec<MonthActivity>> {
        let rows = sqlx::query(
            "SELECT substr(day, 1, 7)    AS month,
                    count(DISTINCT book_id) AS books,
                    count(DISTINCT day)     AS activity_days,
                    sum(minutes)            AS minutes,
                    sum(pages)              AS pages
               FROM reading_events
              WHERE day BETWEEN ? AND ?
              GROUP BY month
              ORDER BY month ASC",
        )
        .bind(range.from())
        .bind(range.to())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| MonthActivity {
                month: r.get("month"),
                books: r.get("books"),
                activity_days: r.get("activity_days"),
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
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    ..Default::default()
                },
                None,
            )
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

    // ---- item 42: the month is a period too --------------------------------

    /// Seed one row directly. The month aggregate is about grouping, not about
    /// which filler wrote a row, so the fillers are not in the way here.
    async fn event(s: &Storage, book: i64, day: &str, minutes: Option<i64>) {
        s.record_reading_events(&[NewReadingEvent {
            book_id: book,
            reading_id: None,
            day: day.into(),
            minutes,
            pages: None,
            source: SOURCE_KOREADER.into(),
            confidence: Confidence::Measured,
        }])
        .await
        .unwrap();
    }

    /// **The test the item is for.** A month holding measured days *beside*
    /// unmeasured ones must report only what was measured, and a month holding
    /// none at all must report `None` — never `Some(0)`, which is the lie a
    /// client's `reduce` produces on the first `null` it meets.
    #[tokio::test]
    async fn a_months_minutes_sum_only_what_was_measured() {
        let (s, book) = seeded().await;
        // March: two days the device timed, two it never did.
        event(&s, book, "2026-03-02", None).await;
        event(&s, book, "2026-03-05", Some(40)).await;
        event(&s, book, "2026-03-09", None).await;
        event(&s, book, "2026-03-21", Some(20)).await;
        // April: read about, never timed.
        event(&s, book, "2026-04-03", None).await;
        event(&s, book, "2026-04-04", None).await;

        let year = DayRange::new("2026-01-01", "2026-12-31").unwrap();
        let months = s.activity_by_month(&year).await.unwrap();

        let march = months.iter().find(|m| m.month == "2026-03").unwrap();
        assert_eq!(
            march.minutes,
            Some(60),
            "the unmeasured days must contribute nothing, not zero"
        );
        assert_eq!(march.activity_days, 4, "all four days still happened");

        let april = months.iter().find(|m| m.month == "2026-04").unwrap();
        assert_eq!(
            april.minutes, None,
            "a month with no device data has no minutes; it does not have zero"
        );
        assert_ne!(april.minutes, Some(0));
        assert_eq!(april.activity_days, 2, "and the days are still known");
        assert_eq!(april.pages, None);
    }

    /// The field that cannot be summed from days, which is the whole reason
    /// this grain lives below the seam rather than in a frontend's `reduce`.
    #[tokio::test]
    async fn a_months_books_are_distinct_over_the_month_and_do_not_sum_from_days() {
        let (s, a) = seeded().await;
        let b = s
            .upsert_book(
                &Book {
                    title: Some("Piranesi".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        for day in ["2026-05-01", "2026-05-02", "2026-05-03"] {
            event(&s, a, day, None).await;
        }
        event(&s, b, "2026-05-02", None).await;

        let may = DayRange::new("2026-05-01", "2026-05-31").unwrap();
        let by_day = s.activity_by_day(&may).await.unwrap();
        let summed: i64 = by_day.iter().map(|d| d.books).sum();
        assert_eq!(summed, 4, "three days of one book and one day of two");

        let months = s.activity_by_month(&may).await.unwrap();
        assert_eq!(months.len(), 1);
        assert_eq!(
            months[0].books, 2,
            "two books were open that month, however many days each was"
        );
        assert_ne!(
            months[0].books, summed,
            "summing the days is the arithmetic this method exists to refuse"
        );
        assert_eq!(months[0].activity_days, 3);
    }

    /// Only months carrying an event appear, for the reason only days do: an
    /// empty month is the client's to draw, and drawing it *as an absence* is
    /// what stops a gap looking like a zero.
    #[tokio::test]
    async fn only_months_carrying_an_event_appear() {
        let (s, book) = seeded().await;
        event(&s, book, "2026-03-11", Some(5)).await;
        event(&s, book, "2026-05-02", Some(7)).await;

        let year = DayRange::new("2026-01-01", "2026-12-31").unwrap();
        let months = s.activity_by_month(&year).await.unwrap();
        assert_eq!(
            months.iter().map(|m| m.month.as_str()).collect::<Vec<_>>(),
            vec!["2026-03", "2026-05"],
            "April is not a row of zeroes, and neither are the other nine"
        );

        // A year with nothing in it is an empty list, not twelve empty months.
        let before = DayRange::new("2025-01-01", "2025-12-31").unwrap();
        assert!(s.activity_by_month(&before).await.unwrap().is_empty());
    }

    /// A month at the edge of the range covers the part of it inside the range,
    /// and the range is never silently widened to whole months — that would
    /// report days the caller could not see in the same call's summary.
    #[tokio::test]
    async fn a_month_at_the_edge_covers_only_the_days_inside_the_range() {
        let (s, book) = seeded().await;
        for day in ["2026-01-05", "2026-01-25", "2026-02-05", "2026-02-20"] {
            event(&s, book, day, Some(10)).await;
        }

        let straddling = DayRange::new("2026-01-20", "2026-02-10").unwrap();
        let months = s.activity_by_month(&straddling).await.unwrap();
        assert_eq!(
            months.iter().map(|m| m.month.as_str()).collect::<Vec<_>>(),
            vec!["2026-01", "2026-02"]
        );
        assert_eq!(months[0].activity_days, 1, "only the 25th is in range");
        assert_eq!(months[0].minutes, Some(10));
        assert_eq!(months[1].activity_days, 1, "only the 5th is in range");
        assert_eq!(months[1].minutes, Some(10));

        // And the summary of the same range agrees, which is the coherence a
        // widened month would have quietly broken.
        let sum = s.activity_summary(&straddling).await.unwrap();
        assert_eq!(sum.activity_days, 2);
        assert_eq!(sum.minutes, Some(20));
    }

    /// An inverted span is refused before it can become a confident empty year,
    /// the same refusal the two older aggregates get.
    #[tokio::test]
    async fn an_inverted_range_never_reaches_the_month_aggregate() {
        assert!(DayRange::new("2026-12-31", "2026-01-01").is_err());
    }

    // ---- item 22: the typed page, the fifth filler -------------------------

    /// The `local` row of a book's log, as `(day, minutes, pages, confidence)`.
    async fn local_rows(s: &Storage, book: i64) -> Vec<(String, Option<i64>, Option<i64>, bool)> {
        s.reading_events(book)
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.source == SOURCE_LOCAL)
            .map(|e| {
                (
                    e.day,
                    e.minutes,
                    e.pages,
                    e.confidence == Confidence::Measured,
                )
            })
            .collect()
    }

    async fn open_read(s: &Storage, book: i64) -> i64 {
        s.open_reading(book, Some(JAN5), "manual").await.unwrap()
    }

    #[tokio::test]
    async fn the_first_typed_page_files_the_day_and_claims_no_pages() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, None, 42, JAN5).await.unwrap();

        assert_eq!(
            local_rows(&s, b).await,
            vec![("2026-01-05".to_string(), None, None, true)],
            "\"you are on page 42\" is not \"you read 42 pages today\""
        );
    }

    #[tokio::test]
    async fn a_second_page_claims_the_difference_and_nothing_more() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, None, 42, JAN5).await.unwrap();
        s.record_typed_page(b, r, Some(42), 90, JAN5).await.unwrap();

        assert_eq!(local_rows(&s, b).await[0].2, Some(48));
    }

    /// The trap [`EVENT_MERGE`] sets for this filler: it is a `COALESCE`, so a
    /// second write of the day's delta would *replace* the first and the
    /// morning's pages would vanish.
    #[tokio::test]
    async fn two_updates_in_one_day_accumulate_rather_than_replace() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, Some(0), 10, JAN5).await.unwrap();
        s.record_typed_page(b, r, Some(10), 30, JAN5).await.unwrap();

        assert_eq!(local_rows(&s, b).await[0].2, Some(30), "10 + 20, not 20");
    }

    #[tokio::test]
    async fn correcting_a_page_downwards_is_not_negative_pages() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, Some(0), 200, JAN5).await.unwrap();
        s.record_typed_page(b, r, Some(200), 190, JAN5)
            .await
            .unwrap();

        // The correction contributes nothing and erases nothing.
        assert_eq!(local_rows(&s, b).await[0].2, Some(200));
    }

    #[tokio::test]
    async fn retyping_the_same_page_touches_no_row() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, Some(0), 55, JAN5).await.unwrap();

        let again = s.record_typed_page(b, r, Some(55), 55, JAN5).await.unwrap();
        assert_eq!(
            again,
            FillStats::default(),
            "an unchanged day must report neither an insert nor an update"
        );
    }

    #[tokio::test]
    async fn two_days_of_typing_are_two_rows() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, Some(0), 20, JAN5).await.unwrap();
        s.record_typed_page(b, r, Some(20), 55, JAN5 + DAY)
            .await
            .unwrap();

        let rows = local_rows(&s, b).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].2, Some(20));
        assert_eq!(rows[1].2, Some(35));
    }

    /// Every `local` row is `measured` and carries no minutes. Nothing here
    /// times anything, and `Some(0)` minutes would claim somebody read for no
    /// time at all.
    #[tokio::test]
    async fn a_typed_page_measures_pages_and_never_minutes() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, Some(1), 2, JAN5).await.unwrap();

        let (_, minutes, _, measured) = local_rows(&s, b).await[0].clone();
        assert_eq!(minutes, None);
        assert!(measured);
    }

    /// The event names the read it was told about, rather than inferring one.
    #[tokio::test]
    async fn the_event_names_the_reading_it_was_written_to() {
        let (s, b) = seeded().await;
        let r = open_read(&s, b).await;
        s.record_typed_page(b, r, None, 7, JAN5).await.unwrap();

        let e = s.reading_events(b).await.unwrap();
        assert_eq!(e[0].reading_id, Some(r));
    }

    /// The whole point of putting the write in `update_progress`: no frontend
    /// has to remember, so both of them cannot disagree about whether today
    /// counted.
    #[tokio::test]
    async fn update_progress_files_the_day_by_itself() {
        let (s, b) = seeded().await;
        s.update_progress(b, Some(30), None).await.unwrap();

        let rows = local_rows(&s, b).await;
        assert_eq!(rows.len(), 1, "a typed page files exactly one local day");
        assert_eq!(rows[0].1, None);
        assert!(rows[0].3, "the user typed it, so it is measured");
    }

    #[tokio::test]
    async fn toggling_finished_alone_files_nothing_local() {
        let (s, b) = seeded().await;
        s.update_progress(b, None, Some(true)).await.unwrap();
        assert!(
            local_rows(&s, b).await.is_empty(),
            "closing a read is `fill_events_from_readings`' business, not this one"
        );
    }

    /// The reason `local` is its own word rather than the reading's `source`.
    /// Two claimants, one day, two rows — which is what the primary key
    /// `(book_id, day, source)` is arranged to allow.
    #[tokio::test]
    async fn a_typed_page_and_the_reads_own_endpoint_share_a_day_without_colliding() {
        let (s, b) = seeded().await;
        s.update_progress(b, Some(12), None).await.unwrap();
        s.refill_reading_events().await.unwrap();

        let mut sources: Vec<String> = s
            .reading_events(b)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.source)
            .collect();
        sources.sort();
        assert_eq!(sources, vec!["local".to_string(), "manual".to_string()]);
    }

    /// A refill must not disturb what the typed page recorded. `EVENT_MERGE` is
    /// no-clobber and the two fillers write different sources, so this is two
    /// invariants at once — and it is the one that would break silently.
    #[tokio::test]
    async fn refilling_does_not_erase_a_typed_days_pages() {
        let (s, b) = seeded().await;
        s.update_progress(b, Some(10), None).await.unwrap();
        s.update_progress(b, Some(40), None).await.unwrap();
        let before = local_rows(&s, b).await;
        assert_eq!(before[0].2, Some(30));

        s.refill_reading_events().await.unwrap();
        s.refill_reading_events().await.unwrap();
        assert_eq!(local_rows(&s, b).await, before);
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use crate::book::Book;
    use crate::storage::{NewHighlight, NewNoteMeta};
    use proptest::prelude::*;

    /// 2026-01-05 00:00:00 UTC.
    const JAN5_UTC: i64 = 1_767_571_200;

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
                s.upsert_book(
                    &Book {
                        title: Some(format!("book {n}")),
                        ..Default::default()
                    },
                    None,
                )
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
        /// A day's `local` pages are the sum of the forward moves made that
        /// day, and never anything else.
        ///
        /// A property rather than more examples because the rule is arithmetic
        /// and the interesting inputs are the ones a hand-written case does not
        /// think of: a correction backwards, a repeat of the same number, a
        /// first page with nothing before it. Each of those has its own way of
        /// producing a plausible-looking wrong total — a negative delta, a
        /// double count, a fabricated `Some(42)` on the first entry — and all
        /// three are invisible in an example where the reader only ever moves
        /// forward by a comfortable amount.
        #[test]
        fn a_days_pages_are_the_forward_moves_of_that_day(
            pages in proptest::collection::vec(0i64..500, 1..12),
        ) {
            rt().block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let b = s.upsert_book(&Book { title: Some("P".into()), ..Default::default() }, None)
                    .await.unwrap();
                let r = s.open_reading(b, Some(JAN5_UTC), "manual").await.unwrap();

                // The delta needs two points, so the first entry establishes
                // the position and contributes nothing — which is exactly what
                // the expected total below has to agree with.
                let mut expected = 0i64;
                let mut prev: Option<i64> = None;
                for p in &pages {
                    s.record_typed_page(b, r, prev, *p, JAN5_UTC).await.unwrap();
                    if let Some(q) = prev {
                        expected += (p - q).max(0);
                    }
                    prev = Some(*p);
                }

                let row = s.reading_events(b).await.unwrap();
                let local: Vec<_> = row.iter().filter(|e| e.source == SOURCE_LOCAL).collect();
                prop_assert_eq!(local.len(), 1, "one book, one day, one local row");
                if expected == 0 {
                    // Nothing moved forward. That is absence, not zero pages —
                    // the same rule the aggregates hold to.
                    prop_assert_eq!(local[0].pages, None);
                } else {
                    prop_assert_eq!(local[0].pages, Some(expected));
                }
                prop_assert_eq!(local[0].minutes, None);
                prop_assert_eq!(local[0].confidence, Confidence::Measured);
                Ok(())
            })?;
        }

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

        /// The months regroup the days — except for `books`, which is the one
        /// field that cannot be recovered from them (item 42).
        ///
        /// A property rather than more examples because the interesting input
        /// is the *shape of the overlap*: whether a book appears on one day of
        /// a month or on nine, and whether a month's measured days sit beside
        /// unmeasured ones, is what decides whether summing the days happens to
        /// give the right answer. Hand-picked libraries are hand-picked to be
        /// ones where it does.
        ///
        /// Three claims, and the third is deliberately an inequality rather
        /// than an equation: `books` is asserted **exactly** against the events
        /// and only *bounded* by the days, because there is no arithmetic over
        /// `activity_by_day` that yields it — which is the claim itself.
        #[test]
        fn months_regroup_the_days_and_books_is_the_field_that_does_not(
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

                let by_day = s.activity_by_day(&range).await.unwrap();
                let months = s.activity_by_month(&range).await.unwrap();

                // 1. The months are exactly the months of the days, in order,
                //    and no empty month is invented between two full ones.
                let mut want: Vec<String> =
                    by_day.iter().map(|d| d.day[..7].to_string()).collect();
                want.dedup();
                prop_assert_eq!(
                    months.iter().map(|m| m.month.clone()).collect::<Vec<_>>(),
                    want
                );

                for m in &months {
                    let days: Vec<&DayActivity> =
                        by_day.iter().filter(|d| d.day.starts_with(&m.month)).collect();

                    // 2. Days and minutes are sums of the days — and `None`
                    //    survives the sum rather than becoming zero.
                    prop_assert_eq!(m.activity_days, days.len() as i64);
                    let measured: Vec<i64> = days.iter().filter_map(|d| d.minutes).collect();
                    if measured.is_empty() {
                        prop_assert_eq!(
                            m.minutes, None,
                            "a month whose every day was unmeasured cannot claim a number"
                        );
                    } else {
                        prop_assert_eq!(m.minutes, Some(measured.iter().sum::<i64>()));
                    }

                    // 3. `books` is distinct over the month, which the days
                    //    bound from both sides and determine from neither.
                    let mut books: Vec<i64> = events.iter()
                        .filter(|e| e.day.starts_with(&m.month))
                        .map(|e| e.book_id)
                        .collect();
                    books.sort_unstable();
                    books.dedup();
                    prop_assert_eq!(m.books, books.len() as i64);

                    let most = days.iter().map(|d| d.books).max().unwrap_or(0);
                    let summed: i64 = days.iter().map(|d| d.books).sum();
                    prop_assert!(
                        m.books >= most && m.books <= summed,
                        "a month holds at least its busiest day's books and at most their sum"
                    );
                }
                Ok(())
            })?;
        }
    }
}
