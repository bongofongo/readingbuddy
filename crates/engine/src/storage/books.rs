use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use time::OffsetDateTime;

use super::highlights::identity_hash_of;
use super::{Storage, now_unix};
use crate::book::Book;
use crate::error::{EngineError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookSort {
    LastModified,
    Title,
    Progress,
}

/// What [`Storage::merge_books`] actually moved.
///
/// The dropped counts are the interesting ones: they are rows that existed on
/// both sides and are gone from the count of what survived, which is the only
/// way a caller can tell "merged cleanly" from "merged and lost duplicates".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// False when `src` was already gone — the merge had nothing to do. Every
    /// other field is then zero, which is what makes a repeat merge a no-op
    /// rather than an error.
    pub src_existed: bool,
    pub highlights_moved: usize,
    /// Highlights that collided on identity with one already on `dst`. Dropped,
    /// not duplicated; anything anchored to the dropped copy was repointed at
    /// the survivor first.
    pub highlights_dropped: usize,
    pub notes_moved: usize,
    /// Readings repointed at `dst`. Both books may have had an open reading;
    /// the older one is closed as `abandoned` rather than deleted, so it is
    /// counted here like any other.
    pub readings_moved: usize,
    pub flashcards_moved: usize,
    /// `flashcards` is `UNIQUE(book_id, word)`, so a word both books captured
    /// cannot move.
    pub flashcards_dropped: usize,
    pub device_links_moved: usize,
    /// Owned files repointed at `dst`. No `dropped` twin: `book_files` is keyed
    /// on the sha256 alone, so the same content cannot already be on both sides
    /// and a collision is not representable.
    pub files_moved: usize,
    /// `src`'s cover file, when `dst` already had one of its own and therefore
    /// kept it. The file is now unreferenced; the caller deletes it, the same
    /// contract [`Storage::delete_book`] has.
    pub orphaned_cover: Option<String>,
}

/// The `books` columns plus the four reading-state **projections**.
///
/// `current_page`, `finished`, `date_started` and `date_finished` left `books`
/// with migration `0005` and now come off the current reading. Keeping them on
/// [`Book`] as read-only projections is what left every consumer — the CLI's
/// `render.rs`, the TUI's `progress_tag` and `progress_text`, the note page
/// auto-anchor — untouched by that move, and [`row_to_book`] unchanged with it.
///
/// Every `books` column is qualified: `readings` carries `id`, `created_at` and
/// `last_modified` of its own, and an unqualified name would be ambiguous.
pub(super) const BOOK_COLUMNS: &str = "books.id, books.title, books.sort_title, books.authors, \
     books.translators, books.publisher, books.publish_year, books.language, books.isbn_10, \
     books.isbn_13, books.openlibrary_key, books.googlebooks_id, books.cover_url, \
     books.cover_path, books.page_count, books.description, books.first_sentence, \
     cur.current_page AS current_page, \
     CASE WHEN cur.status = 'finished' THEN 1 ELSE 0 END AS finished, \
     cur.started_at AS date_started, cur.finished_at AS date_finished, \
     books.created_at, books.last_modified";

/// The join that resolves **the current reading**: the open one if there is
/// one, else the most recent.
///
/// One join rather than four correlated subqueries, and one definition of
/// "current" rather than a different one per column — a book whose `finished`
/// came from its last reading while its `current_page` came from nowhere would
/// render as a contradiction.
pub(super) const BOOK_FROM: &str = "FROM books LEFT JOIN readings cur ON cur.id = (
         SELECT r.id FROM readings r WHERE r.book_id = books.id
          ORDER BY (r.finished_at IS NULL) DESC,
                   COALESCE(r.started_at, r.created_at) DESC, r.id DESC
          LIMIT 1)";

pub(super) fn row_to_book(row: &SqliteRow) -> Result<Book> {
    let authors: String = row.try_get("authors")?;
    let translators: String = row.try_get("translators")?;
    let created: i64 = row.try_get("created_at")?;
    let modified: i64 = row.try_get("last_modified")?;
    Ok(Book {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        sort_title: row.try_get("sort_title")?,
        authors: serde_json::from_str(&authors).unwrap_or_default(),
        translators: serde_json::from_str(&translators).unwrap_or_default(),
        publisher: row.try_get("publisher")?,
        publish_year: row.try_get("publish_year")?,
        language: row.try_get("language")?,
        isbn_10: row.try_get("isbn_10")?,
        isbn_13: row.try_get("isbn_13")?,
        openlibrary_key: row.try_get("openlibrary_key")?,
        googlebooks_id: row.try_get("googlebooks_id")?,
        cover_url: row.try_get("cover_url")?,
        cover_path: row.try_get("cover_path")?,
        page_count: row.try_get("page_count")?,
        description: row.try_get("description")?,
        first_sentence: row.try_get("first_sentence")?,
        current_page: row.try_get("current_page")?,
        finished: row.try_get::<i64, _>("finished")? != 0,
        date_started: row.try_get("date_started")?,
        date_finished: row.try_get("date_finished")?,
        created_at: OffsetDateTime::from_unix_timestamp(created).ok(),
        last_modified: OffsetDateTime::from_unix_timestamp(modified).ok(),
    })
}

/// How one column merges when a **partial** record arrives.
///
/// The three shapes are not stylistic. `title` and the two JSON list columns are
/// NOT NULL with a sentinel empty value (`''`, `'[]'`), so `COALESCE` would
/// happily overwrite a real title with an empty string; the rest are nullable
/// and `NULL` genuinely means "this record does not say".
#[derive(Clone, Copy)]
enum Merge {
    /// NOT NULL text: keep what is there unless the new value is non-empty.
    NonEmptyText,
    /// NOT NULL JSON array: keep what is there unless the new value is not `[]`.
    NonEmptyList,
    /// Nullable: `COALESCE`, so a missing field never clobbers a known one.
    Coalesce,
}

/// **The provider no-clobber merge, defined once.**
///
/// Two statements need it — `ON CONFLICT DO UPDATE` in [`Storage::upsert_book`],
/// and a plain `UPDATE … WHERE id = ?` in [`Storage::enrich_book`] — and they
/// must not be able to disagree about what "merge a partial record" means. That
/// is the same rule `DEVICE_FIELDS_DIFFER` and `identity_hash_of` follow: one
/// formula, both sides through it.
///
/// It is emphatically **not** the device merge. A sidecar is the device's
/// complete state, so a missing note there means the user deleted it and
/// straight assignment is correct. A provider record — and a `calibredb list`
/// row, which carries no page count at all — is partial, and missing means
/// "don't know". `docs/decisions.md`: do not copy one pattern to the other.
const MERGE_RULES: [(&str, Merge); 16] = [
    ("title", Merge::NonEmptyText),
    ("sort_title", Merge::Coalesce),
    ("authors", Merge::NonEmptyList),
    ("translators", Merge::NonEmptyList),
    ("publisher", Merge::Coalesce),
    ("publish_year", Merge::Coalesce),
    ("language", Merge::Coalesce),
    ("isbn_10", Merge::Coalesce),
    ("isbn_13", Merge::Coalesce),
    ("openlibrary_key", Merge::Coalesce),
    ("googlebooks_id", Merge::Coalesce),
    ("cover_url", Merge::Coalesce),
    ("cover_path", Merge::Coalesce),
    ("page_count", Merge::Coalesce),
    ("description", Merge::Coalesce),
    ("first_sentence", Merge::Coalesce),
];

/// The SET clause for [`MERGE_RULES`]. `src` names where the incoming value
/// comes from, per column: `excluded.title` inside an upsert, `?1` inside an
/// update. `last_modified` is appended by the caller, since only one of the two
/// statements binds it positionally.
fn merge_set(src: impl Fn(usize, &str) -> String) -> String {
    MERGE_RULES
        .iter()
        .enumerate()
        .map(|(i, (col, rule))| {
            let new = src(i, col);
            match rule {
                Merge::NonEmptyText => {
                    format!("{col} = CASE WHEN {new} != '' THEN {new} ELSE books.{col} END")
                }
                Merge::NonEmptyList => {
                    format!("{col} = CASE WHEN {new} != '[]' THEN {new} ELSE books.{col} END")
                }
                Merge::Coalesce => format!("{col} = COALESCE({new}, books.{col})"),
            }
        })
        .collect::<Vec<_>>()
        .join(",\n            ")
}

/// Bind the sixteen [`MERGE_RULES`] columns, in order, to a query.
///
/// Shared for the same reason the clause is: the SQL and the binds are one
/// thing, and a column added to the list without a bind beside it is a runtime
/// error in a statement nobody reads.
fn bind_merge_columns<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    book: &Book,
) -> Result<sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>> {
    Ok(q.bind(book.title.clone().unwrap_or_default())
        .bind(book.sort_title.clone())
        .bind(serde_json::to_string(&book.authors)?)
        .bind(serde_json::to_string(&book.translators)?)
        .bind(book.publisher.clone())
        .bind(book.publish_year)
        .bind(book.language.clone())
        .bind(book.isbn_10.clone())
        .bind(book.isbn_13.clone())
        .bind(book.openlibrary_key.clone())
        .bind(book.googlebooks_id.clone())
        .bind(book.cover_url.clone())
        .bind(book.cover_path.clone())
        .bind(book.page_count)
        .bind(book.description.clone())
        .bind(book.first_sentence.clone()))
}

impl Storage {
    /// Insert-or-merge keyed on isbn_10, else isbn_13, else plain insert.
    /// COALESCE(excluded.x, books.x) keeps NULL fields from clobbering
    /// existing data. Returns the row id.
    ///
    /// **Reading state is not written here, and `Book`'s four progress fields
    /// are ignored.** They are projections of the current reading
    /// (`BOOK_COLUMNS`); writing them belongs to
    /// [`Storage::update_progress`]. This is also why the old
    /// `finished = MAX(excluded.finished, books.finished)` clause is gone rather
    /// than moved: it only ever existed to stop a metadata refresh from
    /// un-finishing a book, and a provider upsert now has no reach into reading
    /// state at all. Everything else keeps its `COALESCE` no-clobber — that
    /// pattern is still right for providers, which return partial records.
    pub async fn upsert_book(&self, book: &Book) -> Result<i64> {
        let set_clause = format!(
            "{},\n            last_modified = excluded.last_modified",
            merge_set(|_, col| format!("excluded.{col}"))
        );

        let insert = r#"INSERT INTO books (
                title, sort_title, authors, translators, publisher, publish_year, language,
                isbn_10, isbn_13, openlibrary_key, googlebooks_id, cover_url, cover_path,
                page_count, description, first_sentence, created_at, last_modified
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#;

        let sql = if book.isbn_10.is_some() {
            format!("{insert} ON CONFLICT(isbn_10) DO UPDATE SET {set_clause} RETURNING id")
        } else if book.isbn_13.is_some() {
            format!("{insert} ON CONFLICT(isbn_13) DO UPDATE SET {set_clause} RETURNING id")
        } else {
            format!("{insert} RETURNING id")
        };

        let now = now_unix();
        let row = bind_merge_columns(sqlx::query(&sql), book)?
            .bind(now)
            .bind(now)
            .fetch_one(self.pool())
            .await?;
        Ok(row.try_get("id")?)
    }

    /// Merge a partial record into a book **we have already identified**.
    ///
    /// [`Storage::upsert_book`] cannot do this job, and the way it fails is
    /// silent: its third branch — no `isbn_10`, no `isbn_13` — is a *plain
    /// unconditional insert*, so handing it a `Book` whose `id` is set creates a
    /// second row rather than updating the first. That is the same trap
    /// `import_book_from_sidecar` had to key on `device_books` to avoid, and it
    /// is exactly the shape a calibre import meets: a book matched by uuid or by
    /// file hash, carrying no ISBN at all.
    ///
    /// Same rules as the upsert, from [`MERGE_RULES`] — the caller has a partial
    /// record either way, and which statement runs is about *how the book was
    /// found*, never about what a missing field means.
    pub async fn enrich_book(&self, book_id: i64, book: &Book) -> Result<()> {
        // ?1..?16 are the merge columns, ?17 is last_modified, ?18 the id.
        let sql = format!(
            "UPDATE books SET {}, last_modified = ?17 WHERE id = ?18",
            merge_set(|i, _| format!("?{}", i + 1))
        );
        bind_merge_columns(sqlx::query(&sql), book)?
            .bind(now_unix())
            .bind(book_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn get_book(&self, id: i64) -> Result<Option<Book>> {
        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    /// Lookup by a normalized ISBN (either column).
    pub async fn find_book_by_isbn(&self, isbn: &str) -> Result<Option<Book>> {
        let sql = format!(
            "SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.isbn_10 = ?1 OR books.isbn_13 = ?1"
        );
        let row = sqlx::query(&sql)
            .bind(isbn)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    pub async fn find_books_by_title(&self, fragment: &str) -> Result<Vec<Book>> {
        let sql = format!(
            "SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.title LIKE ?
             ORDER BY books.last_modified DESC"
        );
        let rows = sqlx::query(&sql)
            .bind(format!("%{fragment}%"))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_book).collect()
    }

    pub async fn list_books(&self, limit: i64, sort: BookSort) -> Result<Vec<Book>> {
        let order = match sort {
            BookSort::LastModified => "books.last_modified DESC",
            BookSort::Title => "books.title COLLATE NOCASE ASC",
            // The joined reading's page, not a `books` column any more.
            BookSort::Progress => {
                "CAST(cur.current_page AS REAL) / NULLIF(books.page_count, 0) DESC NULLS LAST"
            }
        };
        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} ORDER BY {order} LIMIT ?");
        let rows = sqlx::query(&sql).bind(limit).fetch_all(self.pool()).await?;
        rows.iter().map(row_to_book).collect()
    }

    /// Fold `src` into `dst`: move highlights, notes, flashcards, readings,
    /// device links and owned files, fill `dst`'s empty fields from `src`,
    /// delete `src`.
    ///
    /// This exists because the ISBN-less insert path guarantees duplicates
    /// regardless of how good matching gets: `upsert_book` branches
    /// isbn_10 → isbn_13 → *plain insert*, and a book created from a sidecar
    /// has neither ISBN. Matching can only shrink the rate, never reach zero,
    /// so the library needs a way to put two rows back together.
    ///
    /// **One transaction.** A half-merged library — highlights moved, notes
    /// not — is worse than the duplicate it was trying to fix, and nothing
    /// would tell the user it had happened.
    ///
    /// **The identity collision is handled, not avoided.** `book_id` is an
    /// input to a highlight's `identity_hash`, so every moved row's hash has to
    /// be recomputed against `dst`; one that then collides with a highlight
    /// already there is the *same annotation*, and is dropped rather than
    /// duplicated. Its note anchors and flashcards are repointed at the
    /// survivor first — dropping them would quietly undo exactly the guarantee
    /// migration `0004` went to the trouble of keeping.
    ///
    /// Field merge is `COALESCE`-style with **`dst` winning**: `dst` is the row
    /// the user chose to keep, so `src` only fills what `dst` does not have.
    ///
    /// Idempotent: a repeat merge finds `src` already gone and returns a report
    /// with `src_existed == false` and every count zero. A missing `dst` is a
    /// real error — there is nothing to merge *into*.
    pub async fn merge_books(&self, src: i64, dst: i64) -> Result<MergeReport> {
        let mut report = MergeReport::default();
        if src == dst {
            // Not an error: "make these two one" is already true. Proceeding
            // would delete the book.
            report.src_existed = self.get_book(src).await?.is_some();
            return Ok(report);
        }

        // One connection for the whole merge. Nothing below may call back
        // through `self` — an in-memory Storage caps the pool at one
        // connection, so a nested acquire would deadlock rather than fail.
        let mut tx = self.pool().begin().await?;

        let dst_cover: Option<Option<String>> =
            sqlx::query_scalar("SELECT cover_path FROM books WHERE id = ?")
                .bind(dst)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(dst_cover) = dst_cover else {
            return Err(EngineError::NotFound(format!("book id {dst}")));
        };

        let sql = format!("SELECT {BOOK_COLUMNS} {BOOK_FROM} WHERE books.id = ?");
        let Some(src_row) = sqlx::query(&sql).bind(src).fetch_optional(&mut *tx).await? else {
            // Already merged (or never existed). Returning an empty report
            // rather than an error is what makes merging twice equal merging
            // once, which the caller — a retry, a re-run, a device screen
            // firing the same action — has no other way to be sure of.
            return Ok(report);
        };
        let src_book = row_to_book(&src_row)?;
        report.src_existed = true;

        // ---- highlights: recompute identity against dst, drop collisions ----
        let rows =
            sqlx::query("SELECT id, text, pos0, ko_datetime FROM highlights WHERE book_id = ?")
                .bind(src)
                .fetch_all(&mut *tx)
                .await?;
        for r in &rows {
            let id: i64 = r.try_get("id")?;
            let text: String = r.try_get("text")?;
            let pos0: Option<String> = r.try_get("pos0")?;
            let ko_datetime: Option<String> = r.try_get("ko_datetime")?;
            let hash = identity_hash_of(dst, ko_datetime.as_deref(), pos0.as_deref(), &text);

            let survivor: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM highlights WHERE book_id = ? AND identity_hash = ?",
            )
            .bind(dst)
            .bind(&hash)
            .fetch_optional(&mut *tx)
            .await?;
            match survivor {
                Some(keep) => {
                    // The same annotation under two book ids. Move what hangs
                    // off the copy we are about to drop before dropping it:
                    // `flashcards.highlight_id` cascades on delete and
                    // `notes.highlight_id` nulls, so doing this in the other
                    // order loses note anchors silently.
                    sqlx::query("UPDATE notes SET highlight_id = ? WHERE highlight_id = ?")
                        .bind(keep)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("UPDATE flashcards SET highlight_id = ? WHERE highlight_id = ?")
                        .bind(keep)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM highlights WHERE id = ?")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    report.highlights_dropped += 1;
                }
                None => {
                    sqlx::query(
                        "UPDATE highlights SET book_id = ?, identity_hash = ? WHERE id = ?",
                    )
                    .bind(dst)
                    .bind(&hash)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    report.highlights_moved += 1;
                }
            }
        }

        // ---- notes -------------------------------------------------------
        report.notes_moved = sqlx::query("UPDATE notes SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- flashcards: UNIQUE(book_id, word) forbids a straight move ----
        report.flashcards_dropped = sqlx::query(
            "DELETE FROM flashcards WHERE book_id = ?1
             AND word IN (SELECT word FROM flashcards WHERE book_id = ?2)",
        )
        .bind(src)
        .bind(dst)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
        report.flashcards_moved = sqlx::query("UPDATE flashcards SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- readings ------------------------------------------------------
        // Two open readings would violate `idx_readings_one_open`, so the older
        // one is closed first — `abandoned`, at its own `last_modified`, because
        // deleting it would lose a reading that really happened. This runs
        // before the move (and before `src` is deleted, which would cascade its
        // readings away entirely).
        sqlx::query(
            r#"UPDATE readings SET finished_at = last_modified, status = 'abandoned'
               WHERE id = (
                   SELECT id FROM readings
                    WHERE book_id IN (?1, ?2) AND finished_at IS NULL
                    ORDER BY COALESCE(started_at, created_at) ASC, id ASC
                    LIMIT 1)
                 AND (SELECT count(*) FROM readings
                       WHERE book_id IN (?1, ?2) AND finished_at IS NULL) > 1"#,
        )
        .bind(src)
        .bind(dst)
        .execute(&mut *tx)
        .await?;
        report.readings_moved = sqlx::query("UPDATE readings SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- device links --------------------------------------------------
        report.device_links_moved =
            sqlx::query("UPDATE device_books SET book_id = ? WHERE book_id = ?")
                .bind(dst)
                .bind(src)
                .execute(&mut *tx)
                .await?
                .rows_affected() as usize;

        // ---- owned files ---------------------------------------------------
        // Same reason as the provenance below: `book_files` cascades on `books`,
        // so a merge that left it alone would *delete* the rows with `src` — and
        // the bytes they name would stay on disk, owned by nothing, findable by
        // nothing, with the book that was folded in showing no files at all.
        //
        // A plain `UPDATE`, unlike `book_tags`: the primary key is the sha256,
        // so `src` and `dst` cannot both hold one file and there is nothing to
        // ignore.
        report.files_moved = sqlx::query("UPDATE book_files SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // ---- provenance ----------------------------------------------------
        // `external_ids` and `book_tags` both cascade on `books`, so a merge
        // that left them alone would *delete* them with `src` — and losing
        // `external_ids` is the expensive one: the next Goodreads import would
        // no longer recognise the row and would recreate the duplicate this
        // merge just folded in.
        //
        // `UPDATE OR IGNORE` for the tags, because their primary key is
        // `(book_id, tag, source)` and both books being on the same shelf is
        // ordinary; the losers are dropped by the cascade a moment later.
        sqlx::query("UPDATE external_ids SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE OR IGNORE book_tags SET book_id = ? WHERE book_id = ?")
            .bind(dst)
            .bind(src)
            .execute(&mut *tx)
            .await?;

        // `src` goes before `dst` is updated: isbn_10 and isbn_13 are UNIQUE, so
        // handing `dst` an ISBN `src` still holds would fail the constraint.
        sqlx::query("DELETE FROM books WHERE id = ?")
            .bind(src)
            .execute(&mut *tx)
            .await?;

        if dst_cover.is_some() {
            report.orphaned_cover = src_book.cover_path.clone();
        }

        // dst wins: `src` fills only what `dst` does not already have. The
        // inverse of `upsert_book`'s clause, where the incoming record wins —
        // there the new data is the authority, here the kept row is.
        sqlx::query(
            r#"UPDATE books SET
                   title           = CASE WHEN title   != ''   THEN title   ELSE ?2  END,
                   sort_title      = COALESCE(sort_title,      ?3),
                   authors         = CASE WHEN authors != '[]' THEN authors ELSE ?4  END,
                   translators     = CASE WHEN translators != '[]' THEN translators ELSE ?5 END,
                   publisher       = COALESCE(publisher,       ?6),
                   publish_year    = COALESCE(publish_year,    ?7),
                   language        = COALESCE(language,        ?8),
                   isbn_10         = COALESCE(isbn_10,         ?9),
                   isbn_13         = COALESCE(isbn_13,         ?10),
                   openlibrary_key = COALESCE(openlibrary_key, ?11),
                   googlebooks_id  = COALESCE(googlebooks_id,  ?12),
                   cover_url       = COALESCE(cover_url,       ?13),
                   cover_path      = COALESCE(cover_path,      ?14),
                   page_count      = COALESCE(page_count,      ?15),
                   description     = COALESCE(description,     ?16),
                   first_sentence  = COALESCE(first_sentence,  ?17),
                   last_modified   = ?18
               WHERE id = ?1"#,
        )
        .bind(dst)
        .bind(src_book.title.as_deref().unwrap_or(""))
        .bind(src_book.sort_title.as_ref())
        .bind(serde_json::to_string(&src_book.authors)?)
        .bind(serde_json::to_string(&src_book.translators)?)
        .bind(src_book.publisher.as_ref())
        .bind(src_book.publish_year)
        .bind(src_book.language.as_ref())
        .bind(src_book.isbn_10.as_ref())
        .bind(src_book.isbn_13.as_ref())
        .bind(src_book.openlibrary_key.as_ref())
        .bind(src_book.googlebooks_id.as_ref())
        .bind(src_book.cover_url.as_ref())
        .bind(src_book.cover_path.as_ref())
        .bind(src_book.page_count)
        .bind(src_book.description.as_ref())
        .bind(src_book.first_sentence.as_ref())
        .bind(now_unix())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(report)
    }

    /// Backdate a book's `created_at`.
    ///
    /// Narrow on purpose: it is for an importer that has *just created* a book
    /// and knows when the user really added it — Goodreads' `Date Added`. For a
    /// book we already had, when it arrived here is ours to know and no CSV has
    /// standing to rewrite it, which is why the caller checks and this function
    /// does not.
    pub async fn set_book_created_at(&self, book_id: i64, created_at: i64) -> Result<()> {
        sqlx::query("UPDATE books SET created_at = ? WHERE id = ?")
            .bind(created_at)
            .bind(book_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Delete a book; returns its cover_path (if any) so the caller can
    /// clean up the image file.
    pub async fn delete_book(&self, id: i64) -> Result<Option<String>> {
        let cover: Option<Option<String>> =
            sqlx::query_scalar("DELETE FROM books WHERE id = ? RETURNING cover_path")
                .bind(id)
                .fetch_optional(self.pool())
                .await?;
        match cover {
            None => Err(EngineError::NotFound(format!("book id {id}"))),
            Some(path) => Ok(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Book {
        Book {
            title: Some("Pachinko".into()),
            authors: vec!["Min Jin Lee".into()],
            isbn_13: Some("9781455563937".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn upsert_merges_without_clobbering() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut b = sample();
        b.description = Some("A sweeping saga.".into());
        let id1 = s.upsert_book(&b).await.unwrap();

        // Re-upsert same ISBN with missing description but new page count:
        // description must survive, page count must land, id must be stable.
        let mut b2 = sample();
        b2.page_count = Some(490);
        let id2 = s.upsert_book(&b2).await.unwrap();
        assert_eq!(id1, id2);

        let got = s.get_book(id1).await.unwrap().unwrap();
        assert_eq!(got.description.as_deref(), Some("A sweeping saga."));
        assert_eq!(got.page_count, Some(490));
        assert_eq!(got.authors, vec!["Min Jin Lee".to_string()]);
    }

    /// The two statements built from [`MERGE_RULES`] must not be able to
    /// disagree about what merging a partial record means.
    ///
    /// The clause is generated once and rendered twice — `excluded.x` inside the
    /// upsert, `?n` inside the update — so this is the assertion that the second
    /// rendering is the same rule and not merely similar SQL. Run against the
    /// same starting row and the same partial update, both must land in exactly
    /// the same state.
    #[tokio::test]
    async fn enrich_merges_a_partial_record_exactly_as_the_upsert_does() {
        let mut full = sample();
        full.description = Some("A sweeping saga.".into());
        full.publisher = Some("Grand Central".into());
        full.page_count = Some(490);

        // Partial: a new publish year, an empty title, no authors, and nothing
        // to say about the three fields already set.
        let partial = Book {
            title: None,
            isbn_13: full.isbn_13.clone(),
            publish_year: Some(2017),
            language: Some("en".into()),
            ..Default::default()
        };

        let by_upsert = {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s.upsert_book(&full).await.unwrap();
            // Same ISBN, so this takes the ON CONFLICT branch.
            assert_eq!(s.upsert_book(&partial).await.unwrap(), id);
            s.get_book(id).await.unwrap().unwrap()
        };
        let by_enrich = {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let id = s.upsert_book(&full).await.unwrap();
            s.enrich_book(id, &partial).await.unwrap();
            s.get_book(id).await.unwrap().unwrap()
        };

        for (name, a, b) in [
            ("title", by_upsert.title.clone(), by_enrich.title.clone()),
            (
                "publisher",
                by_upsert.publisher.clone(),
                by_enrich.publisher.clone(),
            ),
            (
                "description",
                by_upsert.description.clone(),
                by_enrich.description.clone(),
            ),
            (
                "language",
                by_upsert.language.clone(),
                by_enrich.language.clone(),
            ),
        ] {
            assert_eq!(a, b, "{name} merged differently");
        }
        assert_eq!(by_upsert.authors, by_enrich.authors);
        assert_eq!(by_upsert.page_count, by_enrich.page_count);
        assert_eq!(by_upsert.publish_year, by_enrich.publish_year);
        // …and both actually merged rather than both blanking everything, which
        // is the way a same-vs-same assertion passes for the wrong reason.
        assert_eq!(by_enrich.title.as_deref(), Some("Pachinko"));
        assert_eq!(by_enrich.authors, vec!["Min Jin Lee".to_string()]);
        assert_eq!(by_enrich.page_count, Some(490));
        assert_eq!(by_enrich.publish_year, Some(2017));
    }

    /// The reason `enrich_book` exists at all: `upsert_book`'s third branch is a
    /// plain unconditional insert, so a `Book` with no ISBN cannot be updated
    /// through it however its `id` is set.
    #[tokio::test]
    async fn enrich_updates_an_isbn_less_book_where_upsert_would_duplicate_it() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s
            .upsert_book(&Book {
                title: Some("Untitled Draft".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        s.enrich_book(
            id,
            &Book {
                publisher: Some("Self".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(s.list_books(10, BookSort::Title).await.unwrap().len(), 1);
        let got = s.get_book(id).await.unwrap().unwrap();
        assert_eq!(got.publisher.as_deref(), Some("Self"));
        assert_eq!(got.title.as_deref(), Some("Untitled Draft"));

        // The contrast, so the claim above is observed rather than asserted:
        // the same partial record through `upsert_book` makes a second row.
        s.upsert_book(&Book {
            id: Some(id),
            publisher: Some("Self".into()),
            ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(s.list_books(10, BookSort::Title).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn upsert_branches_on_isbn10() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let b = Book {
            title: Some("X".into()),
            isbn_10: Some("0306406152".into()),
            ..Default::default()
        };
        let id1 = s.upsert_book(&b).await.unwrap();
        let id2 = s.upsert_book(&b).await.unwrap();
        assert_eq!(id1, id2);
        let found = s.find_book_by_isbn("0306406152").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn progress_and_finish_flow() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample()).await.unwrap();
        let b = s.update_progress(id, Some(100), None).await.unwrap();
        assert_eq!(b.current_page, Some(100));
        assert!(!b.finished);
        assert!(b.date_started.is_some());

        let b = s.update_progress(id, None, Some(true)).await.unwrap();
        assert!(b.finished);
        assert!(b.date_finished.is_some());
        assert_eq!(b.current_page, Some(100));
    }

    /// The *absence* of the retired `finished = MAX(excluded.finished,
    /// books.finished)` clause, pinned.
    ///
    /// That clause existed only because reading state lived on `books`, and
    /// without a test naming it the behaviour comes straight back the next time
    /// someone "fixes" the upsert to carry a `Book`'s progress fields. A
    /// provider record cannot know what page you are on: the fields are ignored,
    /// in both directions — an incoming `finished: true` must not finish a book,
    /// and an incoming `finished: false` must not un-finish one.
    #[tokio::test]
    async fn a_provider_upsert_never_touches_reading_state() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = s.upsert_book(&sample()).await.unwrap();
        s.update_progress(id, Some(100), Some(true)).await.unwrap();
        let before = s.list_readings(id).await.unwrap();

        // A metadata refresh arriving with the opposite of everything.
        let refreshed = Book {
            page_count: Some(490),
            current_page: Some(1),
            finished: false,
            date_started: Some(1),
            date_finished: Some(2),
            ..sample()
        };
        assert_eq!(s.upsert_book(&refreshed).await.unwrap(), id);

        let b = s.get_book(id).await.unwrap().unwrap();
        assert!(b.finished, "a provider upsert must not un-finish a book");
        assert_eq!(b.current_page, Some(100));
        assert_eq!(b.page_count, Some(490), "metadata still lands");
        assert_eq!(s.list_readings(id).await.unwrap(), before);

        // And the other direction: it must not *start* a reading either.
        let fresh = s
            .upsert_book(&Book {
                title: Some("Never opened".into()),
                current_page: Some(42),
                finished: true,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(s.list_readings(fresh).await.unwrap().is_empty());
        let b = s.get_book(fresh).await.unwrap().unwrap();
        assert_eq!(b.current_page, None);
        assert!(!b.finished);
    }

    // ---- merge ------------------------------------------------------------

    use crate::storage::{LinkedBy, NewHighlight, Reading};

    fn hl(text: &str, datetime: &str) -> NewHighlight {
        NewHighlight {
            text: text.into(),
            chapter: None,
            page: Some(1),
            pos0: Some(format!("/body/p[1]/text().{}", text.len())),
            pos1: None,
            ko_datetime: Some(datetime.into()),
            ko_datetime_updated: None,
            color: None,
            note: None,
            source: "koreader".into(),
        }
    }

    async fn book(s: &Storage, title: &str) -> i64 {
        s.upsert_book(&Book {
            title: Some(title.into()),
            ..Default::default()
        })
        .await
        .unwrap()
    }

    async fn count(s: &Storage, sql: &str, id: i64) -> i64 {
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(s.pool())
            .await
            .unwrap()
    }

    /// The whole contract in one pass: everything moves, the duplicate is
    /// dropped rather than doubled, and `src` is gone.
    #[tokio::test]
    async fn merge_moves_everything_and_drops_the_identity_collision() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;

        // Same annotation on both sides. Its `identity_hash` differs today only
        // because `book_id` is one of the hash's inputs — recompute against
        // `dst` and it is plainly the same row.
        let shared = hl(
            "History has failed us, but no matter.",
            "2026-01-05 21:14:08",
        );
        let only_on_src = hl("pachinko", "2026-01-06 08:02:11");
        s.insert_highlight(dst, &shared).await.unwrap();
        let src_shared = s.insert_highlight(src, &shared).await.unwrap().unwrap();
        s.insert_highlight(src, &only_on_src).await.unwrap();

        // Dependents of the copy that is about to be dropped.
        s.insert_flashcard(src, Some(src_shared), "pachinko", None)
            .await
            .unwrap();
        s.link_device_book("abc", src, LinkedBy::Auto)
            .await
            .unwrap();

        let r = s.merge_books(src, dst).await.unwrap();
        assert!(r.src_existed);
        assert_eq!(r.highlights_moved, 1);
        assert_eq!(r.highlights_dropped, 1, "the collision must not duplicate");
        assert_eq!(r.flashcards_moved, 1);
        assert_eq!(r.device_links_moved, 1);

        assert!(s.get_book(src).await.unwrap().is_none(), "src is gone");
        let texts: Vec<String> = s
            .list_highlights(dst)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(texts.len(), 2, "two distinct annotations, got {texts:?}");
        assert_eq!(
            s.find_book_by_partial_md5("abc").await.unwrap().unwrap().id,
            Some(dst)
        );
        assert_eq!(
            count(&s, "SELECT count(*) FROM flashcards WHERE book_id = ?", dst).await,
            1
        );
    }

    /// The dropped copy's note anchors and flashcards have to survive it.
    /// `flashcards.highlight_id` cascades on delete and `notes.highlight_id`
    /// nulls, so deleting first and repointing after would lose both — silently,
    /// and only for books that happened to overlap.
    #[tokio::test]
    async fn a_dropped_duplicate_hands_its_dependents_to_the_survivor() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;

        let shared = hl("a shared passage", "2026-01-05 21:14:08");
        let keep = s.insert_highlight(dst, &shared).await.unwrap().unwrap();
        let doomed = s.insert_highlight(src, &shared).await.unwrap().unwrap();
        s.insert_flashcard(src, Some(doomed), "shared", None)
            .await
            .unwrap();

        s.merge_books(src, dst).await.unwrap();

        let anchored: Option<i64> =
            sqlx::query_scalar("SELECT highlight_id FROM flashcards WHERE word = ?")
                .bind("shared")
                .fetch_one(s.pool())
                .await
                .unwrap();
        assert_eq!(
            anchored,
            Some(keep),
            "the flashcard must follow the survivor"
        );
    }

    /// `flashcards` is `UNIQUE(book_id, word)`, so a word both books captured
    /// cannot simply move. It is dropped, and the merge does not fail.
    #[tokio::test]
    async fn a_word_both_books_captured_is_dropped_not_a_constraint_error() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;
        s.insert_flashcard(src, None, "pachinko", None)
            .await
            .unwrap();
        s.insert_flashcard(dst, None, "pachinko", None)
            .await
            .unwrap();
        s.insert_flashcard(src, None, "hanko", None).await.unwrap();

        let r = s.merge_books(src, dst).await.unwrap();
        assert_eq!(r.flashcards_dropped, 1);
        assert_eq!(r.flashcards_moved, 1);
        assert_eq!(
            count(&s, "SELECT count(*) FROM flashcards WHERE book_id = ?", dst).await,
            2
        );
    }

    /// `dst` is the row the user chose to keep, so `src` fills only its gaps —
    /// the inverse of `upsert_book`, where the incoming record wins. The ISBN is
    /// the one that would blow up if the order were wrong: both columns are
    /// UNIQUE, so `dst` cannot take `src`'s until `src` is gone.
    #[tokio::test]
    async fn dst_wins_and_src_fills_the_gaps() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = s
            .upsert_book(&Book {
                title: Some("Pachinko".into()),
                isbn_13: Some("9781455563937".into()),
                description: Some("from the duplicate".into()),
                page_count: Some(490),
                ..Default::default()
            })
            .await
            .unwrap();
        s.update_progress(src, None, Some(true)).await.unwrap();
        let dst = s
            .upsert_book(&Book {
                title: Some("Pachinko (the keeper)".into()),
                description: Some("the one to keep".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        s.merge_books(src, dst).await.unwrap();

        let got = s.get_book(dst).await.unwrap().unwrap();
        assert_eq!(got.title.as_deref(), Some("Pachinko (the keeper)"));
        assert_eq!(got.description.as_deref(), Some("the one to keep"));
        assert_eq!(got.isbn_13.as_deref(), Some("9781455563937"), "gap filled");
        assert_eq!(got.page_count, Some(490));
        // Not a field merge any more: `src`'s reading came with it, and the
        // projection reads off that.
        assert!(got.finished, "src's reading survives the fold");
    }

    /// Both books were being read. That is two open readings on one book the
    /// moment they fold together, which `idx_readings_one_open` forbids — so the
    /// older is closed as `abandoned` rather than deleted, because it is a
    /// reading that really happened.
    #[tokio::test]
    async fn merging_moves_readings_and_leaves_exactly_one_open() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;

        // `src` started first, and finished a reading before this one.
        s.open_reading(src, Some(1_000), "manual").await.unwrap();
        s.finish_reading(src).await.unwrap();
        s.open_reading(src, Some(2_000), "manual").await.unwrap();
        s.open_reading(dst, Some(3_000), "manual").await.unwrap();

        let r = s.merge_books(src, dst).await.unwrap();
        assert_eq!(r.readings_moved, 2);

        let readings = s.list_readings(dst).await.unwrap();
        assert_eq!(readings.len(), 3, "nothing is deleted");
        let open: Vec<&Reading> = readings
            .iter()
            .filter(|r| r.finished_at.is_none())
            .collect();
        assert_eq!(open.len(), 1, "the index invariant survives the merge");
        assert_eq!(open[0].started_at, Some(3_000), "the newer one stays open");

        let closed = readings
            .iter()
            .find(|r| r.started_at == Some(2_000))
            .expect("src's open reading moved");
        assert_eq!(closed.status, crate::storage::STATUS_ABANDONED);
        assert!(closed.finished_at.is_some());
    }

    /// Only one side was being read: nothing has to be closed, and the open
    /// reading must survive as open. The close is conditional, and a
    /// close-unconditionally version passes the test above.
    #[tokio::test]
    async fn merging_one_open_reading_leaves_it_open() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let src = book(&s, "Pachinko (dupe)").await;
        let dst = book(&s, "Pachinko").await;
        s.open_reading(src, Some(2_000), "manual").await.unwrap();

        s.merge_books(src, dst).await.unwrap();
        let readings = s.list_readings(dst).await.unwrap();
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].finished_at, None);
        assert_eq!(readings[0].status, crate::storage::STATUS_READING);
    }

    /// `src` keeps its cover only when `dst` had none. Otherwise the file is
    /// unreferenced and the caller is told, the same contract `delete_book` has.
    #[tokio::test]
    async fn a_cover_is_adopted_or_reported_as_orphaned() {
        for (dst_cover, expect_orphan) in [(None, None), (Some("dst.jpg"), Some("src.jpg"))] {
            let s = Storage::connect("sqlite::memory:").await.unwrap();
            let src = s
                .upsert_book(&Book {
                    title: Some("a".into()),
                    cover_path: Some("src.jpg".into()),
                    ..Default::default()
                })
                .await
                .unwrap();
            let dst = s
                .upsert_book(&Book {
                    title: Some("b".into()),
                    cover_path: dst_cover.map(str::to_string),
                    ..Default::default()
                })
                .await
                .unwrap();

            let r = s.merge_books(src, dst).await.unwrap();
            assert_eq!(r.orphaned_cover.as_deref(), expect_orphan);
            assert_eq!(
                s.get_book(dst)
                    .await
                    .unwrap()
                    .unwrap()
                    .cover_path
                    .as_deref(),
                Some(dst_cover.unwrap_or("src.jpg"))
            );
        }
    }

    /// A missing `dst` is a real error — there is nothing to merge into — while
    /// a missing `src` is a satisfied postcondition. Merging a book into itself
    /// must not delete it.
    #[tokio::test]
    async fn the_degenerate_cases_are_distinguished() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let id = book(&s, "Pachinko").await;

        assert!(matches!(
            s.merge_books(id, 9999).await,
            Err(EngineError::NotFound(_))
        ));

        let gone = s.merge_books(9999, id).await.unwrap();
        assert!(!gone.src_existed);

        let itself = s.merge_books(id, id).await.unwrap();
        assert!(itself.src_existed);
        assert!(
            s.get_book(id).await.unwrap().is_some(),
            "must not self-delete"
        );
    }

    #[tokio::test]
    async fn delete_returns_cover_path() {
        let s = Storage::connect("sqlite::memory:").await.unwrap();
        let mut b = sample();
        b.cover_path = Some("database/images/x.jpg".into());
        let id = s.upsert_book(&b).await.unwrap();
        let cover = s.delete_book(id).await.unwrap();
        assert_eq!(cover.as_deref(), Some("database/images/x.jpg"));
        assert!(s.get_book(id).await.unwrap().is_none());
        assert!(matches!(
            s.delete_book(id).await,
            Err(EngineError::NotFound(_))
        ));
    }
}

#[cfg(test)]
mod props {
    use super::tests_support::*;
    use proptest::prelude::*;

    proptest! {
        /// Merging `src` into `dst` twice equals merging it once.
        ///
        /// Stated as a property rather than an example because the interesting
        /// input is the *overlap*: which annotations and which flashcard words
        /// the two books happen to share is exactly what decides whether a row
        /// moves, is dropped, or collides — and picking three overlaps by hand
        /// is picking the three that work.
        ///
        /// It matters because the caller retries. A device screen firing the
        /// same merge twice, a re-run after a crash, a user pressing the key
        /// again: none of those should be able to leave the library different
        /// from one clean run.
        #[test]
        fn merging_twice_equals_merging_once(
            src_texts in proptest::collection::vec("[a-c]{1,3}", 0..6),
            dst_texts in proptest::collection::vec("[a-c]{1,3}", 0..6),
            src_words in proptest::collection::vec("[x-z]{1,2}", 0..4),
            dst_words in proptest::collection::vec("[x-z]{1,2}", 0..4),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let s = fixture(&src_texts, &dst_texts, &src_words, &dst_words).await;
                let (src, dst) = (s.src, s.dst);

                let first = s.storage.merge_books(src, dst).await.unwrap();
                prop_assert!(first.src_existed);
                let after_one = snapshot(&s.storage, dst).await;

                let second = s.storage.merge_books(src, dst).await.unwrap();
                prop_assert!(!second.src_existed, "src is gone; there is nothing left to do");
                prop_assert_eq!(second, super::MergeReport::default());
                prop_assert_eq!(after_one, snapshot(&s.storage, dst).await);
                Ok(())
            })?;
        }
    }
}

/// Shared setup for the merge property. Lives outside `mod tests` because
/// `mod props` needs it too, and duplicating the fixture is how the two would
/// drift into testing different things.
#[cfg(test)]
mod tests_support {
    use super::*;
    use crate::storage::NewHighlight;

    pub struct Fixture {
        pub storage: Storage,
        pub src: i64,
        pub dst: i64,
    }

    /// Highlights are keyed on their text, so a text appearing in both lists is
    /// an identity collision and one appearing once is a plain move. Flashcard
    /// words work the same way against `UNIQUE(book_id, word)`.
    pub async fn fixture(
        src_texts: &[String],
        dst_texts: &[String],
        src_words: &[String],
        dst_words: &[String],
    ) -> Fixture {
        let storage = Storage::connect("sqlite::memory:").await.unwrap();
        let mk = |title: &str| Book {
            title: Some(title.to_string()),
            ..Default::default()
        };
        let src = storage.upsert_book(&mk("src")).await.unwrap();
        let dst = storage.upsert_book(&mk("dst")).await.unwrap();

        for (book_id, texts) in [(src, src_texts), (dst, dst_texts)] {
            for t in texts {
                let h = NewHighlight {
                    text: t.clone(),
                    chapter: None,
                    page: Some(1),
                    pos0: Some(format!("/body/p[1]/text().{t}")),
                    pos1: None,
                    ko_datetime: Some("2026-01-01 00:00:00".into()),
                    ko_datetime_updated: None,
                    color: None,
                    note: None,
                    source: "koreader".into(),
                };
                storage.insert_highlight(book_id, &h).await.unwrap();
            }
        }
        for (book_id, words) in [(src, src_words), (dst, dst_words)] {
            for w in words {
                storage.insert_flashcard(book_id, None, w, None).await.ok();
            }
        }
        Fixture { storage, src, dst }
    }

    /// Everything the merge could plausibly disturb, in a comparable shape.
    pub async fn snapshot(storage: &Storage, dst: i64) -> (Vec<String>, Vec<String>, i64) {
        let mut texts: Vec<String> = storage
            .list_highlights(dst)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.text)
            .collect();
        texts.sort();
        let mut words: Vec<String> = storage
            .list_flashcards_for_book(dst)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.word)
            .collect();
        words.sort();
        let books: i64 = sqlx::query_scalar("SELECT count(*) FROM books")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        (texts, words, books)
    }
}
