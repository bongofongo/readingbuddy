//! One row per *reading* of a book.
//!
//! `books` used to carry `current_page`, `finished`, `date_started` and
//! `date_finished`, which modelled one reading of one book. Rereads are real, so
//! they moved here (migration `0005`). [`Book`](crate::book::Book) keeps all
//! four as **read-only projections** of the current reading, which is what left
//! every render call site untouched — see `BOOK_COLUMNS` in
//! [`super::books`].

use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use super::books::{BOOK_COLUMNS, BOOK_FROM, row_to_book};
use super::{Storage, now_unix};
use crate::book::Book;
use crate::error::{EngineError, Result};
use crate::koreader::KoStatus;

/// Our own status vocabulary. Distinct from `ko_status`, which mirrors what the
/// *device* said and is never written by us.
pub const STATUS_READING: &str = "reading";
pub const STATUS_FINISHED: &str = "finished";
pub const STATUS_ABANDONED: &str = "abandoned";

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub id: i64,
    pub book_id: i64,
    pub started_at: Option<i64>,
    /// `NULL` means open, and the partial unique index
    /// `idx_readings_one_open` makes "at most one open reading per book" an
    /// invariant rather than a convention.
    pub finished_at: Option<i64>,
    pub status: String,
    /// `manual` | `koreader` | `migrated`.
    pub source: String,
    pub current_page: Option<i64>,
    /// The device-owned mirror. Refreshed by **straight assignment**, never
    /// `COALESCE` — a sidecar is the complete state, so a cleared rating means
    /// the user cleared it. The user's own rating belongs to a Review.
    pub ko_status: Option<String>,
    pub ko_percent: Option<f64>,
    pub ko_rating: Option<i64>,
    pub created_at: i64,
    pub last_modified: i64,
}

pub(super) const READING_COLUMNS: &str = "id, book_id, started_at, finished_at, status, source, current_page, \
     ko_status, ko_percent, ko_rating, created_at, last_modified";

/// What [`Storage::list_open_readings`] renames the reading's columns to.
///
/// Six of the twelve — `id`, `book_id`, `current_page`, `status`, `created_at`,
/// `last_modified` — are names `BOOK_COLUMNS` also projects, so joining the two
/// into one row means renaming one side of the collision.
const JOINED_READING_PREFIX: &str = "r_";

/// `READING_COLUMNS`, qualified by a table alias and prefixed.
///
/// Derived from the one list rather than spelled out a second time: a
/// hand-written rename list is a second copy of the columns waiting to drift
/// from the first, and a column added to `Reading` but forgotten here would
/// surface as a decode error at runtime rather than a compile error.
fn reading_columns_as(alias: &str, prefix: &str) -> String {
    READING_COLUMNS
        .split(',')
        .map(|c| {
            let c = c.trim();
            format!("{alias}.{c} AS {prefix}{c}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn row_to_reading(row: &SqliteRow) -> Result<Reading> {
    row_to_reading_prefixed(row, "")
}

/// One mapping for both the bare `SELECT {READING_COLUMNS}` shape and the
/// joined, prefixed one.
fn row_to_reading_prefixed(row: &SqliteRow, prefix: &str) -> Result<Reading> {
    let col = |name: &str| format!("{prefix}{name}");
    Ok(Reading {
        id: row.try_get(col("id").as_str())?,
        book_id: row.try_get(col("book_id").as_str())?,
        started_at: row.try_get(col("started_at").as_str())?,
        finished_at: row.try_get(col("finished_at").as_str())?,
        status: row.try_get(col("status").as_str())?,
        source: row.try_get(col("source").as_str())?,
        current_page: row.try_get(col("current_page").as_str())?,
        ko_status: row.try_get(col("ko_status").as_str())?,
        ko_percent: row.try_get(col("ko_percent").as_str())?,
        ko_rating: row.try_get(col("ko_rating").as_str())?,
        created_at: row.try_get(col("created_at").as_str())?,
        last_modified: row.try_get(col("last_modified").as_str())?,
    })
}

/// The device-owned columns, and the null-safe test for "the sidecar disagrees
/// with what we stored".
///
/// One copy, shared by the conditional `UPDATE` in
/// [`Storage::set_device_state`] and the read-only `SELECT` in
/// [`Storage::device_state_differs`], for the same reason
/// `DEVICE_FIELDS_DIFFER` is shared in [`super::highlights`]: a preview that
/// reported different changes than the write it previews would be worse than no
/// preview at all.
///
/// `IS NOT` rather than `!=`: SQLite's `!=` yields NULL when either side is
/// NULL, so a rating the user *cleared* on the device would compare as "no
/// change" and never be removed.
const DEVICE_STATE_DIFFER: &str =
    "(ko_status IS NOT ?2 OR ko_percent IS NOT ?3 OR ko_rating IS NOT ?4)";

/// One book's readings as half-open-ish intervals of unix seconds, `?1` the
/// book id, columns `reading_id`/`win_start`/`win_end`.
///
/// **Extracted so there is one definition, not two.** The derivation of a
/// missing `started_at` is the subtle part — see [`Storage::attribute_highlights`]
/// for the full argument and for the ±infinity bug it replaced — and item 31
/// needs the same windows to place a *day* of measured reading time. Two copies
/// of this would be two chances to reintroduce that bug, and only one of them
/// would have a test aimed at it.
///
/// `?1` rather than `?` because both callers mention the book id twice.
pub(super) const READING_WINDOWS: &str = "
    SELECT r.id AS reading_id,
           COALESCE(
               r.started_at,
               (SELECT MAX(p.finished_at) + 1 FROM readings p
                 WHERE p.book_id = r.book_id
                   AND p.id <> r.id
                   AND p.finished_at IS NOT NULL
                   AND p.finished_at <= COALESCE(r.finished_at, 8640000000000)),
               -8640000000000
           ) AS win_start,
           COALESCE(r.finished_at, 8640000000000) AS win_end
      FROM readings r
     WHERE r.book_id = ?1";

/// KOReader's `datetime` as unix seconds.
///
/// The device writes `YYYY-MM-DD HH:MM:SS` with no zone, so it is read as UTC —
/// the same reading SQLite's `strftime('%s', …)` takes, which is what
/// [`Storage::attribute_highlights`] compares against. The two must agree; a
/// second convention here would put attribution and its test hours apart for
/// anyone not on UTC.
pub fn ko_datetime_to_unix(s: &str) -> Option<i64> {
    let fmt = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    time::PrimitiveDateTime::parse(s.trim(), fmt)
        .ok()
        .map(|dt| dt.assume_utc().unix_timestamp())
}

impl Storage {
    /// The open reading, if there is one.
    pub async fn active_reading(&self, book_id: i64) -> Result<Option<Reading>> {
        let sql = format!(
            "SELECT {READING_COLUMNS} FROM readings WHERE book_id = ? AND finished_at IS NULL"
        );
        let row = sqlx::query(&sql)
            .bind(book_id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_reading).transpose()
    }

    /// One reading by id.
    pub async fn get_reading(&self, id: i64) -> Result<Option<Reading>> {
        let sql = format!("SELECT {READING_COLUMNS} FROM readings WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_reading).transpose()
    }

    /// Every reading of a book, oldest first.
    pub async fn list_readings(&self, book_id: i64) -> Result<Vec<Reading>> {
        let sql = format!(
            "SELECT {READING_COLUMNS} FROM readings WHERE book_id = ?
             ORDER BY COALESCE(started_at, created_at) ASC, id ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(book_id)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_reading).collect()
    }

    /// Every book with an **open** reading, most-recently-touched first.
    ///
    /// This is the home screen's query. It is deliberately the *same* join
    /// `BOOK_COLUMNS`/`BOOK_FROM` already resolve `Book`'s four progress
    /// projections through, plus `WHERE cur.finished_at IS NULL` — a second join
    /// written to the same intent would be free to disagree with the first, and
    /// a home screen whose progress bar came from a different reading than
    /// `Book::current_page` would look right while being wrong.
    ///
    /// `BOOK_FROM`'s subquery orders `(finished_at IS NULL) DESC` first, so
    /// where a book has an open reading `cur` **is** that reading, and the
    /// `WHERE` is a filter on the joined row rather than a second search for it.
    ///
    /// **`cur.id IS NOT NULL` is load-bearing, not belt-and-braces.** The join
    /// is a `LEFT JOIN`, so a book with no reading at all still produces a row —
    /// with every `cur` column NULL, which satisfies `finished_at IS NULL` on
    /// its own. Filtering on that alone listed the whole library as currently
    /// being read, the empty state included.
    ///
    /// Sorted by the later of the book's and the reading's `last_modified`:
    /// reading the book bumps `books.last_modified` (`update_progress` does it
    /// explicitly), but a device sync writes `readings.last_modified` alone
    /// (`set_device_state`), so either stamp on its own would leave a book you
    /// just read on the reader sitting at the bottom of the list.
    ///
    /// **An abandoned reading is an open reading and appears here.** `status`
    /// is on the returned [`Reading`], so a frontend can say so; dropping the
    /// row instead would make a book you might pick up unreachable from the one
    /// screen that lists what you are reading, and there is no other place it
    /// would show up.
    pub async fn list_open_readings(&self, limit: i64) -> Result<Vec<(Book, Reading)>> {
        let reading = reading_columns_as("cur", JOINED_READING_PREFIX);
        let sql = format!(
            "SELECT {BOOK_COLUMNS}, {reading} {BOOK_FROM}
             WHERE cur.id IS NOT NULL AND cur.finished_at IS NULL
             ORDER BY MAX(books.last_modified, cur.last_modified) DESC, books.id DESC
             LIMIT ?"
        );
        let rows = sqlx::query(&sql).bind(limit).fetch_all(self.pool()).await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row_to_book(row)?,
                    row_to_reading_prefixed(row, JOINED_READING_PREFIX)?,
                ))
            })
            .collect()
    }

    /// Open a reading. Returns its id.
    ///
    /// A second open reading violates `idx_readings_one_open`, and that arrives
    /// here as a raw sqlx constraint error. It is translated to
    /// [`EngineError::InvalidInput`] because a caller branches on it — the CLI's
    /// `progress --reread` closes the open one first, and wants to say so rather
    /// than print a constraint message.
    pub async fn open_reading(
        &self,
        book_id: i64,
        started_at: Option<i64>,
        source: &str,
    ) -> Result<i64> {
        let now = now_unix();
        let row = sqlx::query(
            "INSERT INTO readings (book_id, started_at, status, source, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(book_id)
        .bind(started_at)
        .bind(STATUS_READING)
        .bind(source)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => EngineError::InvalidInput(
                format!("book {book_id} already has an open reading; finish it first"),
            ),
            _ => EngineError::from(e),
        })?;
        Ok(row.try_get("id")?)
    }

    /// Record a reading that already happened, dates and status as given.
    ///
    /// The general form of [`Storage::open_reading`], and it exists because an
    /// *import* knows things `open_reading` cannot express: a reading that is
    /// already finished, and finished on a day that is not today. Goodreads'
    /// `Read Count` is the first caller; Calibre will be the second.
    ///
    /// `finished_at = None` still means open, so the same
    /// `idx_readings_one_open` violation surfaces here as
    /// [`EngineError::InvalidInput`] — one translation, one message, whichever
    /// door the second open reading came through.
    pub async fn record_reading(
        &self,
        book_id: i64,
        started_at: Option<i64>,
        finished_at: Option<i64>,
        status: &str,
        source: &str,
    ) -> Result<i64> {
        let now = now_unix();
        let row = sqlx::query(
            "INSERT INTO readings (book_id, started_at, finished_at, status, source,
                                   created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(book_id)
        .bind(started_at)
        .bind(finished_at)
        .bind(status)
        .bind(source)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => EngineError::InvalidInput(
                format!("book {book_id} already has an open reading; finish it first"),
            ),
            _ => EngineError::from(e),
        })?;
        Ok(row.try_get("id")?)
    }

    /// Every reading of a book that came from one importer, oldest first.
    ///
    /// What makes a re-import idempotent: the question an importer has to ask
    /// is "how many of these did *I* record", not "how many are there" — a
    /// reading the user opened by hand is not one of ours to count against the
    /// far side's `Read Count`.
    pub async fn readings_from_source(&self, book_id: i64, source: &str) -> Result<Vec<Reading>> {
        let sql = format!(
            "SELECT {READING_COLUMNS} FROM readings WHERE book_id = ? AND source = ?
             ORDER BY COALESCE(started_at, created_at) ASC, id ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(book_id)
            .bind(source)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_reading).collect()
    }

    /// Close one reading at a date the caller knows. Returns whether anything
    /// changed.
    ///
    /// Beside [`Storage::finish_reading`], which closes *the open one* at
    /// `now()` — right for a person pressing a key, wrong for an import
    /// replaying a date from three years ago.
    pub async fn close_reading_at(
        &self,
        reading_id: i64,
        finished_at: i64,
        status: &str,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE readings SET finished_at = ?2, status = ?3, last_modified = ?4
             WHERE id = ?1 AND (finished_at IS NOT ?2 OR status IS NOT ?3)",
        )
        .bind(reading_id)
        .bind(finished_at)
        .bind(status)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Close the open reading. Returns false when there was none.
    pub async fn finish_reading(&self, book_id: i64) -> Result<bool> {
        let now = now_unix();
        let done = sqlx::query(
            "UPDATE readings SET finished_at = ?2, status = ?3, last_modified = ?2
             WHERE book_id = ?1 AND finished_at IS NULL",
        )
        .bind(book_id)
        .bind(now)
        .bind(STATUS_FINISHED)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Mark the open reading abandoned **without closing it**.
    ///
    /// `finished_at` stays NULL on purpose: an abandoned book is one you might
    /// still pick up, and closing it would make resuming a *reread* rather than
    /// a continuation. It is the status that changed, not the reading.
    pub async fn abandon_reading(&self, book_id: i64) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE readings SET status = ?2, last_modified = ?3
             WHERE book_id = ?1 AND finished_at IS NULL AND status IS NOT ?2",
        )
        .bind(book_id)
        .bind(STATUS_ABANDONED)
        .bind(now_unix())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The current reading's id, opening one only when the book has **none at
    /// all**.
    ///
    /// Deliberately not "open one when none is *open*". A `complete` sidecar
    /// closes the reading it just wrote, so that rule would open a fresh reading
    /// on every subsequent import and the KOReader import — whose whole contract
    /// is idempotency — would grow the reading history without bound.
    ///
    /// Writing to a closed reading is the right answer for the caller that needs
    /// this: KOReader's sidecar is per-file and a reread appends to it, so the
    /// device cannot tell us a second reading has begun. Saying so is
    /// [`Storage::reread`]'s job, and it is the user's decision.
    pub async fn ensure_reading(
        &self,
        book_id: i64,
        started_at: Option<i64>,
        source: &str,
    ) -> Result<i64> {
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM readings WHERE book_id = ?
             ORDER BY (finished_at IS NULL) DESC, COALESCE(started_at, created_at) DESC, id DESC
             LIMIT 1",
        )
        .bind(book_id)
        .fetch_optional(self.pool())
        .await?;
        match current {
            Some(id) => Ok(id),
            None => self.open_reading(book_id, started_at, source).await,
        }
    }

    /// Close the open reading (if any) and open a fresh one. Returns its id.
    ///
    /// One statement pair rather than two calls from the CLI: between them the
    /// partial unique index is satisfied only in one order, and getting that
    /// order wrong is an error the caller would meet as a constraint violation.
    pub async fn reread(&self, book_id: i64) -> Result<i64> {
        self.finish_reading(book_id).await?;
        self.open_reading(book_id, Some(now_unix()), "manual").await
    }

    /// Write reading progress and return the book with its projections
    /// refreshed.
    ///
    /// `id` is the **book** id — the signature `books.rs` had, kept so its two
    /// callers (`cli/src/commands/book.rs`, `tui/src/app.rs`) change nothing at
    /// all.
    ///
    /// Opens a reading when none is open, *except* when the caller is clearing
    /// `finished`: the TUI's finished-toggle would otherwise leave a finished
    /// reading behind and start an empty new one every time it was pressed.
    /// Un-finishing reopens the most recent reading instead, which is what the
    /// user meant.
    pub async fn update_progress(
        &self,
        id: i64,
        page: Option<i64>,
        finished: Option<bool>,
    ) -> Result<Book> {
        if self.get_book(id).await?.is_none() {
            return Err(EngineError::NotFound(format!("book id {id}")));
        }
        let now = now_unix();

        let reading_id = match self.active_reading(id).await? {
            Some(r) => r.id,
            None if finished == Some(false) => {
                // Reopen rather than create. `finished_at IS NULL` is what makes
                // a reading open, so clearing it is the whole operation.
                let latest: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM readings WHERE book_id = ?
                     ORDER BY COALESCE(finished_at, started_at, created_at) DESC, id DESC LIMIT 1",
                )
                .bind(id)
                .fetch_optional(self.pool())
                .await?;
                match latest {
                    Some(rid) => rid,
                    None => self.open_reading(id, Some(now), "manual").await?,
                }
            }
            None => self.open_reading(id, Some(now), "manual").await?,
        };

        sqlx::query(
            r#"UPDATE readings SET
                   current_page  = COALESCE(?2, current_page),
                   started_at    = COALESCE(started_at, ?3),
                   status        = CASE WHEN ?4 = 1 THEN 'finished'
                                        WHEN ?4 = 0 THEN 'reading'
                                        ELSE status END,
                   finished_at   = CASE WHEN ?4 = 1 THEN COALESCE(finished_at, ?3)
                                        WHEN ?4 = 0 THEN NULL
                                        ELSE finished_at END,
                   last_modified = ?3
               WHERE id = ?1"#,
        )
        .bind(reading_id)
        .bind(page)
        .bind(now)
        .bind(finished)
        .execute(self.pool())
        .await?;

        // The book's own `last_modified` is what the library list sorts on, and
        // reading it is why the user reaches for it.
        sqlx::query("UPDATE books SET last_modified = ?2 WHERE id = ?1")
            .bind(id)
            .bind(now)
            .execute(self.pool())
            .await?;

        self.get_book(id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {id}")))
    }

    /// Mirror the device's status/percent/rating onto the current reading.
    /// Returns whether anything actually changed.
    ///
    /// **Straight assignment, not `COALESCE`.** A sidecar is the complete state
    /// of that book on the device, so an absent rating means the user cleared
    /// it. The provider no-clobber pattern is right for partial records and
    /// wrong here — the same seam migration `0004` drew for `ko_note`.
    ///
    /// Writes nothing and reports false when the book has no reading at all;
    /// deciding to *open* one is the import's call, not this function's, because
    /// only the import knows what to start it at.
    pub async fn set_device_state(
        &self,
        book_id: i64,
        status: Option<&KoStatus>,
        percent: Option<f64>,
        rating: Option<i64>,
    ) -> Result<bool> {
        let sql = format!(
            "UPDATE readings SET ko_status = ?2, ko_percent = ?3, ko_rating = ?4,
                                 last_modified = ?5
             WHERE id = (SELECT id FROM readings WHERE book_id = ?1
                         ORDER BY (finished_at IS NULL) DESC,
                                  COALESCE(started_at, created_at) DESC, id DESC LIMIT 1)
               AND {DEVICE_STATE_DIFFER}"
        );
        let done = sqlx::query(&sql)
            .bind(book_id)
            .bind(status.map(|s| s.to_string()))
            .bind(percent)
            .bind(rating)
            .bind(now_unix())
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Would [`Storage::set_device_state`] change anything? Read-only, for a
    /// dry-run preview.
    pub async fn device_state_differs(
        &self,
        book_id: i64,
        status: Option<&KoStatus>,
        percent: Option<f64>,
        rating: Option<i64>,
    ) -> Result<bool> {
        let sql = format!(
            "SELECT count(*) FROM readings
             WHERE id = (SELECT id FROM readings WHERE book_id = ?1
                         ORDER BY (finished_at IS NULL) DESC,
                                  COALESCE(started_at, created_at) DESC, id DESC LIMIT 1)
               AND {DEVICE_STATE_DIFFER}"
        );
        let n: i64 = sqlx::query_scalar(&sql)
            .bind(book_id)
            .bind(status.map(|s| s.to_string()))
            .bind(percent)
            .bind(rating)
            .fetch_one(self.pool())
            .await?;
        Ok(n > 0)
    }

    /// Assign `highlights.reading_id` by matching each highlight's
    /// `ko_datetime` into a reading's window.
    ///
    /// Returns how many highlights of this book now carry a reading.
    ///
    /// **Leaving it `NULL` is correct** when no window contains it. KOReader's
    /// sidecar is per-file and a reread appends to the same file, so the device
    /// cannot supply this attribution and we must not invent it. Unattributed is
    /// reached from the book, which is why it needs no staging bucket.
    ///
    /// Recomputed from scratch rather than filled in: a highlight whose reading
    /// was deleted, or whose window moved, must lose its stale attribution too.
    ///
    /// ## A missing `started_at` is derived, not taken as −∞
    ///
    /// This is the whole substance of the query and it was wrong in the first
    /// cut, which `COALESCE`d an absent bound straight to ±8.64e12. That makes
    /// an unstarted reading's window *contain every earlier reading's window*,
    /// and since the match is "the latest window that holds it", the newest
    /// reading then takes the older readings' highlights and the older readings
    /// end up with none — permanently, and with nothing on screen looking
    /// wrong.
    ///
    /// Both ways in are ordinary rather than exotic. A **Goodreads** import
    /// writes `Read Count > 1` as several readings with NULL `started_at` by
    /// design (the CSV has no start date and `goodreads.rs` refuses to invent
    /// one), so *every* highlight landed on the most recent read. And any
    /// reading opened through [`Storage::record_reading`] or
    /// `open_reading(.., None, ..)` is unstarted, so a reread swallowed the
    /// first read's highlights too.
    ///
    /// So an absent `started_at` derives one: **an unstarted reading begins
    /// where the previous reading ended.** "Previous" is the latest
    /// `finished_at` of another reading of the same book that is not after this
    /// one's own end — which needs no reading-order convention, only the dates
    /// the rows already carry, and is exactly the bound the user would draw by
    /// hand. With no such reading it really is −∞, which is the one case the
    /// old `COALESCE` had right.
    ///
    /// `+ 1` makes that derived bound **exclusive**: reading 1 owns the instant
    /// it finished, and without it that one second lies in both windows and the
    /// tie is settled by the `ORDER BY` in the newer reading's favour. It is not
    /// an epsilon — these are integer unix seconds, so it is the next
    /// representable value. An *explicit* `started_at` stays inclusive: the user
    /// said the reading started then.
    ///
    /// The `ORDER BY` survives as a tie-break rather than as the deciding rule.
    /// Derived windows are disjoint by construction, but a user is free to give
    /// two readings explicitly overlapping dates, and the later window is the
    /// better guess for a highlight that falls in both.
    pub async fn attribute_highlights(&self, book_id: i64) -> Result<usize> {
        sqlx::query(&format!(
            "WITH windows AS ({READING_WINDOWS})
               UPDATE highlights SET reading_id = (
                   SELECT w.reading_id FROM windows w
                    WHERE CAST(strftime('%s', highlights.ko_datetime) AS INTEGER)
                          BETWEEN w.win_start AND w.win_end
                    ORDER BY w.win_start DESC, w.reading_id DESC
                    LIMIT 1)
               WHERE book_id = ?1"
        ))
        .bind(book_id)
        .execute(self.pool())
        .await?;

        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM highlights WHERE book_id = ? AND reading_id IS NOT NULL",
        )
        .bind(book_id)
        .fetch_one(self.pool())
        .await?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seeded() -> (Storage, i64) {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(
                &Book {
                    title: Some("Pachinko".into()),
                    page_count: Some(490),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (s, id)
    }

    async fn add(s: &Storage, title: &str) -> i64 {
        s.upsert_book(
            &Book {
                title: Some(title.into()),
                page_count: Some(300),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
    }

    /// Hand-set the two stamps `list_open_readings` sorts on.
    ///
    /// They are unix *seconds*, so two writes in one test tie and the ordering
    /// would be decided by the tie-break instead of by the thing under test.
    /// `None` leaves that side alone, which is how the device-sync case (the
    /// reading moved, the book did not) is expressed.
    async fn touched_at(s: &Storage, book_id: i64, book: Option<i64>, reading: Option<i64>) {
        if let Some(t) = book {
            sqlx::query("UPDATE books SET last_modified = ? WHERE id = ?")
                .bind(t)
                .bind(book_id)
                .execute(s.pool())
                .await
                .unwrap();
        }
        if let Some(t) = reading {
            sqlx::query("UPDATE readings SET last_modified = ? WHERE book_id = ?")
                .bind(t)
                .bind(book_id)
                .execute(s.pool())
                .await
                .unwrap();
        }
    }

    /// What the home screen lists: books with a reading still open. A finished
    /// one drops off, a reread brings it back, and a book nobody has started is
    /// never there at all.
    #[tokio::test]
    async fn open_readings_are_the_ones_still_open() {
        let (s, pachinko) = seeded().await;
        let unread = add(&s, "Kokoro").await;

        assert!(
            s.list_open_readings(10).await.unwrap().is_empty(),
            "a library nobody has opened is not a currently-reading list"
        );

        s.update_progress(pachinko, Some(120), None).await.unwrap();
        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].0.id, Some(pachinko));
        assert_eq!(open[0].1.current_page, Some(120));
        assert!(
            !open.iter().any(|(b, _)| b.id == Some(unread)),
            "a book with no reading has nothing open"
        );

        s.update_progress(pachinko, None, Some(true)).await.unwrap();
        assert!(
            s.list_open_readings(10).await.unwrap().is_empty(),
            "finishing it takes it off the list"
        );

        // A reread is a new reading, and it is the new one that must come back
        // — not the finished one the book's projections would still show if the
        // filter had been written against `books` instead of `cur`.
        let second = s.reread(pachinko).await.unwrap();
        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].1.id, second);
        assert_eq!(open[0].1.current_page, None);
    }

    /// The anti-drift assertion. `Book`'s four progress fields are projections
    /// of the current reading, and this query returns the reading beside them —
    /// so if it ever grows a join of its own that resolves "current" differently,
    /// the two sides of one row will disagree here first.
    #[tokio::test]
    async fn the_projections_agree_with_the_reading_beside_them() {
        let (s, id) = seeded().await;
        // A finished reading first, so "current" and "open" are genuinely
        // different rows for this book and picking the wrong one is visible.
        s.update_progress(id, Some(490), Some(true)).await.unwrap();
        s.reread(id).await.unwrap();
        s.update_progress(id, Some(40), None).await.unwrap();

        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        let (book, reading) = &open[0];

        assert_eq!(book.current_page, reading.current_page);
        assert_eq!(book.date_started, reading.started_at);
        assert_eq!(book.date_finished, reading.finished_at);
        assert_eq!(book.finished, reading.status == STATUS_FINISHED);
        // And the same four values `get_book` reports, which is what every
        // other surface in the app renders from.
        let elsewhere = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(book.id, elsewhere.id);
        assert_eq!(book.current_page, elsewhere.current_page);
        assert_eq!(book.finished, elsewhere.finished);
        assert_eq!(book.date_started, elsewhere.date_started);
        assert_eq!(book.date_finished, elsewhere.date_finished);
    }

    /// Most-recently-touched first, and "touched" has to mean either stamp: a
    /// device sync writes the reading's `last_modified` and never the book's, so
    /// sorting on `books.last_modified` alone would leave a book you read on the
    /// reader this morning at the bottom of the list.
    #[tokio::test]
    async fn ordering_is_most_recently_touched() {
        let (s, first) = seeded().await;
        let second = add(&s, "Kokoro").await;
        let third = add(&s, "Snow Country").await;
        for id in [first, second, third] {
            s.update_progress(id, Some(10), None).await.unwrap();
        }

        touched_at(&s, first, Some(300), Some(300)).await;
        touched_at(&s, second, Some(100), Some(100)).await;
        touched_at(&s, third, Some(200), Some(200)).await;
        let order: Vec<i64> = s
            .list_open_readings(10)
            .await
            .unwrap()
            .iter()
            .map(|(b, _)| b.id.unwrap())
            .collect();
        assert_eq!(order, vec![first, third, second]);

        // The reading moves, the book does not. This is exactly what
        // `set_device_state` does.
        touched_at(&s, second, None, Some(900)).await;
        let order: Vec<i64> = s
            .list_open_readings(10)
            .await
            .unwrap()
            .iter()
            .map(|(b, _)| b.id.unwrap())
            .collect();
        assert_eq!(order, vec![second, first, third]);
    }

    /// An abandoned reading is still open — `abandon_reading` deliberately does
    /// not stamp `finished_at` — so it is listed, carrying the status that says
    /// so. Filtering it out here would make a book you might pick up unreachable
    /// from the one screen that lists what you are reading.
    #[tokio::test]
    async fn an_abandoned_reading_is_still_listed_and_says_so() {
        let (s, id) = seeded().await;
        s.update_progress(id, Some(60), None).await.unwrap();
        assert!(s.abandon_reading(id).await.unwrap());

        let open = s.list_open_readings(10).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].1.status, STATUS_ABANDONED);
    }

    /// The three states `finished: bool` cannot tell apart, told apart.
    ///
    /// `abandon_reading` leaves the reading open and stamps no `finished_at`, so
    /// an abandoned book and an open one are both `finished: false` with a
    /// `current_page` — and a book nobody has opened is `finished: false` too.
    /// Three different things behind one boolean is the gap `reading_status`
    /// closes, and it is what lets a frontend honour "abandoning a book is not
    /// failure" without deriving the state from three other columns.
    #[tokio::test]
    async fn the_projection_tells_abandoned_from_reading_from_never_opened() {
        let (s, reading) = seeded().await;
        let abandoned = add(&s, "Kokoro").await;
        let untouched = add(&s, "Season of Migration to the North").await;

        s.update_progress(reading, Some(60), None).await.unwrap();
        s.update_progress(abandoned, Some(20), None).await.unwrap();
        assert!(s.abandon_reading(abandoned).await.unwrap());

        let mut got = Vec::new();
        for id in [reading, abandoned, untouched] {
            let b = s
                .get_book(id)
                .await
                .unwrap()
                .expect("seeded book is stored");
            // The boolean all three share is still what it was, so nothing that
            // reads `finished` had to change.
            assert!(!b.finished);
            got.push(b.reading_status);
        }
        // The third is `None`, not a status string of its own: the book has no
        // reading at all, and an invented `"unread"` would be this layer
        // claiming a row exists.
        assert_eq!(
            got,
            vec![
                Some(STATUS_READING.to_string()),
                Some(STATUS_ABANDONED.to_string()),
                None
            ]
        );
    }

    #[tokio::test]
    async fn the_limit_is_honoured() {
        let (s, first) = seeded().await;
        let second = add(&s, "Kokoro").await;
        for id in [first, second] {
            s.update_progress(id, Some(10), None).await.unwrap();
        }
        assert_eq!(s.list_open_readings(1).await.unwrap().len(), 1);
        assert_eq!(s.list_open_readings(0).await.unwrap().len(), 0);
    }

    /// The invariant is an index, not a convention — and it must reach the
    /// caller as something branchable, not a raw sqlx constraint error.
    #[tokio::test]
    async fn the_index_refuses_a_second_open_reading() {
        let (s, id) = seeded().await;
        s.open_reading(id, Some(1), "manual").await.unwrap();
        assert!(matches!(
            s.open_reading(id, Some(2), "manual").await,
            Err(EngineError::InvalidInput(_))
        ));
        // Closing the first one is what makes room.
        assert!(s.finish_reading(id).await.unwrap());
        assert!(s.open_reading(id, Some(2), "manual").await.is_ok());
        assert_eq!(s.list_readings(id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn finishing_nothing_reports_nothing() {
        let (s, id) = seeded().await;
        assert!(!s.finish_reading(id).await.unwrap());
    }

    /// The reread flow end to end, through the projections the render layer
    /// actually reads.
    #[tokio::test]
    async fn a_reread_gets_its_own_reading_and_the_book_follows_it() {
        let (s, id) = seeded().await;
        s.update_progress(id, Some(200), None).await.unwrap();
        let b = s.update_progress(id, None, Some(true)).await.unwrap();
        assert!(b.finished);
        assert_eq!(b.current_page, Some(200));
        let first_finished = b.date_finished.expect("stamped");

        s.reread(id).await.unwrap();
        let b = s.update_progress(id, Some(30), None).await.unwrap();
        assert_eq!(b.current_page, Some(30), "the book follows the new reading");
        assert!(!b.finished);

        let readings = s.list_readings(id).await.unwrap();
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].current_page, Some(200));
        assert_eq!(readings[0].finished_at, Some(first_finished));
        assert_eq!(readings[1].current_page, Some(30));
        assert_eq!(readings[1].finished_at, None);
    }

    /// The TUI's finished-toggle. Un-finishing must reopen the reading it just
    /// closed, not start a new one — otherwise pressing the key twice leaves two
    /// readings behind and the reading history fills with noise.
    #[tokio::test]
    async fn unfinishing_reopens_rather_than_starting_over() {
        let (s, id) = seeded().await;
        s.update_progress(id, Some(120), Some(true)).await.unwrap();
        let b = s.update_progress(id, None, Some(false)).await.unwrap();
        assert!(!b.finished);
        assert_eq!(b.current_page, Some(120), "progress survives the toggle");
        assert_eq!(b.date_finished, None);
        assert_eq!(s.list_readings(id).await.unwrap().len(), 1);
    }

    /// A book with no reading at all projects as untouched, which is what the
    /// library list renders for something merely added.
    #[tokio::test]
    async fn a_book_with_no_reading_projects_as_blank() {
        let (s, id) = seeded().await;
        let b = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(b.current_page, None);
        assert!(!b.finished);
        assert_eq!(b.date_started, None);
        assert_eq!(b.date_finished, None);
        assert!(s.list_readings(id).await.unwrap().is_empty());
    }

    /// The counter comes straight off this boolean, so it has to be exact in
    /// both directions — including the one `!=` would get wrong.
    #[tokio::test]
    async fn device_state_reports_only_a_real_change() {
        let (s, id) = seeded().await;
        s.open_reading(id, Some(1), "koreader").await.unwrap();

        let complete = KoStatus::Complete;
        assert!(
            s.device_state_differs(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap()
        );
        assert!(
            s.set_device_state(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap()
        );
        assert!(
            !s.set_device_state(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap(),
            "an identical sidecar is not an update"
        );
        assert!(
            !s.device_state_differs(id, Some(&complete), Some(0.5), Some(4))
                .await
                .unwrap(),
            "the preview must agree with the write"
        );

        let r = &s.list_readings(id).await.unwrap()[0];
        assert_eq!(r.ko_status.as_deref(), Some("complete"));
        assert_eq!(r.ko_percent, Some(0.5));
        assert_eq!(r.ko_rating, Some(4));

        // Cleared on the device means cleared here. `COALESCE` would make this
        // impossible to sync, permanently.
        assert!(s.set_device_state(id, None, None, None).await.unwrap());
        let r = &s.list_readings(id).await.unwrap()[0];
        assert_eq!(r.ko_rating, None);
        assert_eq!(r.ko_status, None);
    }

    #[tokio::test]
    async fn device_state_with_no_reading_writes_nothing() {
        let (s, id) = seeded().await;
        assert!(
            !s.set_device_state(id, Some(&KoStatus::Reading), Some(0.1), None)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn ko_datetime_parses_as_utc_and_rejects_junk() {
        assert_eq!(ko_datetime_to_unix("1970-01-01 00:00:01"), Some(1));
        assert_eq!(ko_datetime_to_unix("2026-01-05 21:14:08"), Some(1767647648));
        assert_eq!(ko_datetime_to_unix("not a date"), None);
        assert_eq!(ko_datetime_to_unix(""), None);
    }

    // ---- attribution ------------------------------------------------------

    use crate::storage::NewHighlight;

    fn hl(text: &str, when: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{text}")),
            pos1: None,
            ko_datetime: Some(when.into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    async fn reading_of(s: &Storage, book_id: i64, text: &str) -> Option<i64> {
        sqlx::query_scalar("SELECT reading_id FROM highlights WHERE book_id = ? AND text = ?")
            .bind(book_id)
            .bind(text)
            .fetch_one(s.pool())
            .await
            .unwrap()
    }

    /// Two readings, three highlights, and one of them outside both windows.
    /// The out-of-window one stays `NULL` on purpose: the device cannot tell us
    /// which reading it belonged to, and guessing would be worse than saying so.
    #[tokio::test]
    async fn attribution_splits_by_window_and_leaves_the_rest_null() {
        let (s, id) = seeded().await;
        let first = s
            .open_reading(id, ko_datetime_to_unix("2024-01-01 00:00:00"), "manual")
            .await
            .unwrap();
        s.finish_reading(id).await.unwrap();
        sqlx::query("UPDATE readings SET finished_at = ? WHERE id = ?")
            .bind(ko_datetime_to_unix("2024-02-01 00:00:00"))
            .bind(first)
            .execute(s.pool())
            .await
            .unwrap();
        let second = s
            .open_reading(id, ko_datetime_to_unix("2026-01-01 00:00:00"), "manual")
            .await
            .unwrap();

        for h in [
            hl("in the first", "2024-01-15 09:00:00"),
            hl("in the second", "2026-03-02 09:00:00"),
            // Between the two readings: no window holds it.
            hl("in neither", "2025-06-01 09:00:00"),
        ] {
            s.insert_highlight(id, &h).await.unwrap();
        }

        assert_eq!(s.attribute_highlights(id).await.unwrap(), 2);
        assert_eq!(reading_of(&s, id, "in the first").await, Some(first));
        assert_eq!(reading_of(&s, id, "in the second").await, Some(second));
        assert_eq!(reading_of(&s, id, "in neither").await, None);
    }

    /// A stale attribution has to be dropped, not kept. A recompute that only
    /// ever *added* would leave a highlight pointing at a reading whose window
    /// no longer contains it, and nothing on screen would look wrong.
    #[tokio::test]
    async fn attribution_is_recomputed_not_accumulated() {
        let (s, id) = seeded().await;
        let r = s
            .open_reading(id, ko_datetime_to_unix("2024-01-01 00:00:00"), "manual")
            .await
            .unwrap();
        s.insert_highlight(id, &hl("inside", "2024-01-15 09:00:00"))
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 1);

        // The window moves out from under it.
        sqlx::query("UPDATE readings SET started_at = ? WHERE id = ?")
            .bind(ko_datetime_to_unix("2025-01-01 00:00:00"))
            .bind(r)
            .execute(s.pool())
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 0);
        assert_eq!(reading_of(&s, id, "inside").await, None);
    }

    /// The Goodreads shape: several readings, none with a start date, each
    /// closed at the `Date Read` its row carried.
    ///
    /// This is the case the first cut got wrong, and it is not a corner — it is
    /// what *every* `Read Count > 1` row imports as, because the CSV has no
    /// start date and `goodreads.rs` refuses to invent one. Taking an absent
    /// `started_at` as −∞ makes the newest reading's window contain both older
    /// ones, so both highlights landed on the last read and the first two
    /// readings held none.
    #[tokio::test]
    async fn unstarted_readings_do_not_swallow_the_earlier_ones_highlights() {
        let (s, id) = seeded().await;
        let first = s
            .record_reading(
                id,
                None,
                ko_datetime_to_unix("2020-02-01 00:00:00"),
                STATUS_FINISHED,
                "goodreads",
            )
            .await
            .unwrap();
        let second = s
            .record_reading(
                id,
                None,
                ko_datetime_to_unix("2023-02-01 00:00:00"),
                STATUS_FINISHED,
                "goodreads",
            )
            .await
            .unwrap();
        let third = s
            .record_reading(
                id,
                None,
                ko_datetime_to_unix("2026-02-01 00:00:00"),
                STATUS_FINISHED,
                "goodreads",
            )
            .await
            .unwrap();

        for h in [
            hl("read once", "2020-01-15 09:00:00"),
            hl("read twice", "2023-01-15 09:00:00"),
            hl("read thrice", "2026-01-15 09:00:00"),
        ] {
            s.insert_highlight(id, &h).await.unwrap();
        }
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 3);

        assert_eq!(reading_of(&s, id, "read once").await, Some(first));
        assert_eq!(reading_of(&s, id, "read twice").await, Some(second));
        assert_eq!(reading_of(&s, id, "read thrice").await, Some(third));
    }

    /// The same defect through the other door: a reread whose reading was
    /// opened without a start date.
    ///
    /// An open reading has no `finished_at` either, so its window was −∞..+∞ and
    /// it took the finished reading's highlights along with its own. The derived
    /// bound is what stops it, and the assertion that matters is the *first*
    /// read keeping what it already had.
    #[tokio::test]
    async fn an_unstarted_reread_leaves_the_first_reads_highlights_alone() {
        let (s, id) = seeded().await;
        let first = s
            .record_reading(
                id,
                ko_datetime_to_unix("2020-01-01 00:00:00"),
                ko_datetime_to_unix("2020-02-01 00:00:00"),
                STATUS_FINISHED,
                "manual",
            )
            .await
            .unwrap();
        s.insert_highlight(id, &hl("first read", "2020-01-15 09:00:00"))
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 1);
        assert_eq!(reading_of(&s, id, "first read").await, Some(first));

        // Open, and with nothing to derive a start from — what
        // `open_reading(.., None, ..)` gives, and what a sidecar with no usable
        // datetimes reaches through `ensure_reading`.
        let second = s.open_reading(id, None, "koreader").await.unwrap();
        s.insert_highlight(id, &hl("second read", "2026-03-01 09:00:00"))
            .await
            .unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 2);

        assert_eq!(
            reading_of(&s, id, "first read").await,
            Some(first),
            "opening a reread must not move the first read's highlights onto it"
        );
        assert_eq!(reading_of(&s, id, "second read").await, Some(second));
    }

    /// The derived bound is exclusive; an explicit one is inclusive.
    ///
    /// A highlight captured at the exact second a reading closed belongs to that
    /// reading. Without the `+ 1` that instant lies in both windows and the
    /// `ORDER BY` hands it to the newer one — a one-second hole that only ever
    /// shows up on real device data, where a highlight and a "finished" tap land
    /// in the same second often enough to matter.
    #[tokio::test]
    async fn the_derived_start_is_exclusive_and_an_explicit_one_is_not() {
        let (s, id) = seeded().await;
        let closed_at = ko_datetime_to_unix("2020-02-01 00:00:00");
        let first = s
            .record_reading(
                id,
                ko_datetime_to_unix("2020-01-01 00:00:00"),
                closed_at,
                STATUS_FINISHED,
                "manual",
            )
            .await
            .unwrap();
        s.open_reading(id, None, "manual").await.unwrap();
        s.insert_highlight(id, &hl("on the boundary", "2020-02-01 00:00:00"))
            .await
            .unwrap();
        s.attribute_highlights(id).await.unwrap();
        assert_eq!(
            reading_of(&s, id, "on the boundary").await,
            Some(first),
            "a reading owns the instant it finished; the derived start is the second after"
        );

        // Said explicitly, the same instant is the *later* reading's start, and
        // the later window wins. The user's own dates are taken at face value.
        let (s, id) = seeded().await;
        s.record_reading(
            id,
            ko_datetime_to_unix("2020-01-01 00:00:00"),
            closed_at,
            STATUS_FINISHED,
            "manual",
        )
        .await
        .unwrap();
        let second = s.open_reading(id, closed_at, "manual").await.unwrap();
        s.insert_highlight(id, &hl("on the boundary", "2020-02-01 00:00:00"))
            .await
            .unwrap();
        s.attribute_highlights(id).await.unwrap();
        assert_eq!(reading_of(&s, id, "on the boundary").await, Some(second));
    }

    /// A highlight with no `ko_datetime` cannot be placed at all.
    #[tokio::test]
    async fn a_highlight_without_a_datetime_stays_unattributed() {
        let (s, id) = seeded().await;
        s.open_reading(id, Some(0), "manual").await.unwrap();
        let mut h = hl("undated", "");
        h.ko_datetime = None;
        s.insert_highlight(id, &h).await.unwrap();
        assert_eq!(s.attribute_highlights(id).await.unwrap(), 0);
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// However many times a book is finished and reopened, there is never
        /// more than one open reading and the book's projections come from the
        /// newest one.
        ///
        /// A property rather than more examples because the rule is general and
        /// the interesting input is the *sequence*: the ordering between
        /// `finish`, `reread` and a bare page update is exactly what decides
        /// which reading a write lands on, and three hand-picked sequences are
        /// three that happened to work.
        #[test]
        fn a_book_never_has_two_open_readings(
            steps in proptest::collection::vec(0u8..3, 1..12),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let id = s.upsert_book(&Book { title: Some("t".into()), ..Default::default() }, None)
                    .await.unwrap();

                let mut page = 0i64;
                for step in &steps {
                    match step {
                        0 => { page += 10; s.update_progress(id, Some(page), None).await.unwrap(); }
                        1 => { s.update_progress(id, None, Some(true)).await.unwrap(); }
                        _ => { s.reread(id).await.unwrap(); page = 0; }
                    }
                    let open: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM readings WHERE book_id = ? AND finished_at IS NULL")
                        .bind(id).fetch_one(s.pool()).await.unwrap();
                    prop_assert!(open <= 1, "two open readings after {steps:?}");
                }

                // The projection tracks the current reading: open if there is
                // one, else the latest closed one.
                let book = s.get_book(id).await.unwrap().unwrap();
                let readings = s.list_readings(id).await.unwrap();
                let current = readings.iter().find(|r| r.finished_at.is_none())
                    .or_else(|| readings.last())
                    .expect("every sequence writes at least one reading");
                prop_assert_eq!(book.current_page, current.current_page);
                prop_assert_eq!(book.finished, current.status == STATUS_FINISHED);
                prop_assert_eq!(book.date_finished, current.finished_at);
                Ok(())
            })?;
        }

        /// A book is on the currently-reading list **exactly when**
        /// [`Storage::active_reading`] finds it something open — and the row
        /// carries that same reading.
        ///
        /// A property because the rule is a biconditional over the whole
        /// library, and the two ways to get it wrong are opposite: the `LEFT
        /// JOIN` makes a book with *no* reading look open (every `cur` column
        /// NULL satisfies `finished_at IS NULL`, which listed the entire library
        /// once already), while a filter written against `books` rather than
        /// `cur` would drop a reread. Books are stepped independently so the
        /// generated sequences put the library in mixed states rather than
        /// marching it through one.
        #[test]
        fn the_open_list_is_exactly_the_books_with_an_open_reading(
            steps in proptest::collection::vec((0usize..3, 0u8..3), 1..16),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let s = Storage::connect("sqlite::memory:").await.unwrap();
                let mut ids = Vec::new();
                for n in 0..3 {
                    ids.push(s.upsert_book(&Book {
                        title: Some(format!("book {n}")), ..Default::default()
                    }, None).await.unwrap());
                }

                for (which, step) in &steps {
                    let id = ids[*which];
                    match step {
                        0 => { s.update_progress(id, Some(10), None).await.unwrap(); }
                        1 => { s.update_progress(id, None, Some(true)).await.unwrap(); }
                        _ => { s.reread(id).await.unwrap(); }
                    }
                }

                let listed = s.list_open_readings(10).await.unwrap();
                for id in &ids {
                    let active = s.active_reading(*id).await.unwrap();
                    let row = listed.iter().find(|(b, _)| b.id == Some(*id));
                    match (active, row) {
                        (Some(a), Some((_, r))) => prop_assert_eq!(a.id, r.id),
                        (None, None) => {}
                        (a, r) => prop_assert!(
                            false,
                            "book {} listed as {:?} but active is {:?} after {:?}",
                            id, r.map(|(_, r)| r.id), a.map(|a| a.id), steps
                        ),
                    }
                }
                Ok(())
            })?;
        }
    }
}
