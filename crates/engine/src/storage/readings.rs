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

fn row_to_reading(row: &SqliteRow) -> Result<Reading> {
    Ok(Reading {
        id: row.try_get("id")?,
        book_id: row.try_get("book_id")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        status: row.try_get("status")?,
        source: row.try_get("source")?,
        current_page: row.try_get("current_page")?,
        ko_status: row.try_get("ko_status")?,
        ko_percent: row.try_get("ko_percent")?,
        ko_rating: row.try_get("ko_rating")?,
        created_at: row.try_get("created_at")?,
        last_modified: row.try_get("last_modified")?,
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
    /// `ko_datetime` into a reading's `[started_at, finished_at]` window.
    /// Returns how many highlights of this book now carry a reading.
    ///
    /// **Leaving it `NULL` is correct** when no window contains it. KOReader's
    /// sidecar is per-file and a reread appends to the same file, so the device
    /// cannot supply this attribution and we must not invent it. Unattributed is
    /// reached from the book, which is why it needs no staging bucket.
    ///
    /// Recomputed from scratch rather than filled in: a highlight whose reading
    /// was deleted, or whose window moved, must lose its stale attribution too.
    pub async fn attribute_highlights(&self, book_id: i64) -> Result<usize> {
        sqlx::query(
            r#"UPDATE highlights SET reading_id = (
                   SELECT r.id FROM readings r
                    WHERE r.book_id = highlights.book_id
                      AND CAST(strftime('%s', highlights.ko_datetime) AS INTEGER)
                          >= COALESCE(r.started_at, -8640000000000)
                      AND CAST(strftime('%s', highlights.ko_datetime) AS INTEGER)
                          <= COALESCE(r.finished_at, 8640000000000)
                    ORDER BY COALESCE(r.started_at, r.created_at) DESC, r.id DESC
                    LIMIT 1)
               WHERE book_id = ?1"#,
        )
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
            .upsert_book(&Book {
                title: Some("Pachinko".into()),
                page_count: Some(490),
                ..Default::default()
            })
            .await
            .unwrap();
        (s, id)
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
                let id = s.upsert_book(&Book { title: Some("t".into()), ..Default::default() })
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
    }
}
