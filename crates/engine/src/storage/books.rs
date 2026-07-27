use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use time::OffsetDateTime;

use super::{Storage, now_unix};
use crate::book::Book;
use crate::error::{EngineError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookSort {
    LastModified,
    Title,
    Progress,
}

pub(super) const BOOK_COLUMNS: &str = "id, title, sort_title, authors, translators, publisher, publish_year, \
     language, isbn_10, isbn_13, openlibrary_key, googlebooks_id, cover_url, cover_path, \
     page_count, description, first_sentence, current_page, finished, date_started, \
     date_finished, created_at, last_modified";

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

impl Storage {
    /// Insert-or-merge keyed on isbn_10, else isbn_13, else plain insert.
    /// COALESCE(excluded.x, books.x) keeps NULL fields from clobbering
    /// existing data; `finished` merges with MAX so a re-import never
    /// un-finishes a book. Returns the row id.
    pub async fn upsert_book(&self, book: &Book) -> Result<i64> {
        let set_clause = r#"
            title           = CASE WHEN excluded.title != '' THEN excluded.title ELSE books.title END,
            sort_title      = COALESCE(excluded.sort_title,      books.sort_title),
            authors         = CASE WHEN excluded.authors != '[]' THEN excluded.authors ELSE books.authors END,
            translators     = CASE WHEN excluded.translators != '[]' THEN excluded.translators ELSE books.translators END,
            publisher       = COALESCE(excluded.publisher,       books.publisher),
            publish_year    = COALESCE(excluded.publish_year,    books.publish_year),
            language        = COALESCE(excluded.language,        books.language),
            isbn_10         = COALESCE(excluded.isbn_10,         books.isbn_10),
            isbn_13         = COALESCE(excluded.isbn_13,         books.isbn_13),
            openlibrary_key = COALESCE(excluded.openlibrary_key, books.openlibrary_key),
            googlebooks_id  = COALESCE(excluded.googlebooks_id,  books.googlebooks_id),
            cover_url       = COALESCE(excluded.cover_url,       books.cover_url),
            cover_path      = COALESCE(excluded.cover_path,      books.cover_path),
            page_count      = COALESCE(excluded.page_count,      books.page_count),
            description     = COALESCE(excluded.description,     books.description),
            first_sentence  = COALESCE(excluded.first_sentence,  books.first_sentence),
            current_page    = COALESCE(excluded.current_page,    books.current_page),
            finished        = MAX(excluded.finished, books.finished),
            date_started    = COALESCE(excluded.date_started,    books.date_started),
            date_finished   = COALESCE(excluded.date_finished,   books.date_finished),
            last_modified   = excluded.last_modified
        "#;

        let insert = r#"INSERT INTO books (
                title, sort_title, authors, translators, publisher, publish_year, language,
                isbn_10, isbn_13, openlibrary_key, googlebooks_id, cover_url, cover_path,
                page_count, description, first_sentence, current_page, finished,
                date_started, date_finished, created_at, last_modified
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#;

        let sql = if book.isbn_10.is_some() {
            format!("{insert} ON CONFLICT(isbn_10) DO UPDATE SET {set_clause} RETURNING id")
        } else if book.isbn_13.is_some() {
            format!("{insert} ON CONFLICT(isbn_13) DO UPDATE SET {set_clause} RETURNING id")
        } else {
            format!("{insert} RETURNING id")
        };

        let now = now_unix();
        let row = sqlx::query(&sql)
            .bind(book.title.as_deref().unwrap_or(""))
            .bind(book.sort_title.as_ref())
            .bind(serde_json::to_string(&book.authors)?)
            .bind(serde_json::to_string(&book.translators)?)
            .bind(book.publisher.as_ref())
            .bind(book.publish_year)
            .bind(book.language.as_ref())
            .bind(book.isbn_10.as_ref())
            .bind(book.isbn_13.as_ref())
            .bind(book.openlibrary_key.as_ref())
            .bind(book.googlebooks_id.as_ref())
            .bind(book.cover_url.as_ref())
            .bind(book.cover_path.as_ref())
            .bind(book.page_count)
            .bind(book.description.as_ref())
            .bind(book.first_sentence.as_ref())
            .bind(book.current_page)
            .bind(book.finished)
            .bind(book.date_started)
            .bind(book.date_finished)
            .bind(now)
            .bind(now)
            .fetch_one(self.pool())
            .await?;
        Ok(row.try_get("id")?)
    }

    pub async fn get_book(&self, id: i64) -> Result<Option<Book>> {
        let sql = format!("SELECT {BOOK_COLUMNS} FROM books WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    /// Lookup by a normalized ISBN (either column).
    pub async fn find_book_by_isbn(&self, isbn: &str) -> Result<Option<Book>> {
        let sql = format!("SELECT {BOOK_COLUMNS} FROM books WHERE isbn_10 = ?1 OR isbn_13 = ?1");
        let row = sqlx::query(&sql)
            .bind(isbn)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_book).transpose()
    }

    pub async fn find_books_by_title(&self, fragment: &str) -> Result<Vec<Book>> {
        let sql = format!(
            "SELECT {BOOK_COLUMNS} FROM books WHERE title LIKE ? ORDER BY last_modified DESC"
        );
        let rows = sqlx::query(&sql)
            .bind(format!("%{fragment}%"))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_book).collect()
    }

    pub async fn list_books(&self, limit: i64, sort: BookSort) -> Result<Vec<Book>> {
        let order = match sort {
            BookSort::LastModified => "last_modified DESC",
            BookSort::Title => "title COLLATE NOCASE ASC",
            BookSort::Progress => {
                "CAST(current_page AS REAL) / NULLIF(page_count, 0) DESC NULLS LAST"
            }
        };
        let sql = format!("SELECT {BOOK_COLUMNS} FROM books ORDER BY {order} LIMIT ?");
        let rows = sqlx::query(&sql).bind(limit).fetch_all(self.pool()).await?;
        rows.iter().map(row_to_book).collect()
    }

    pub async fn update_progress(
        &self,
        id: i64,
        page: Option<i64>,
        finished: Option<bool>,
    ) -> Result<Book> {
        let now = now_unix();
        sqlx::query(
            r#"UPDATE books SET
                current_page  = COALESCE(?2, current_page),
                date_started  = COALESCE(date_started, ?3),
                finished      = COALESCE(?4, finished),
                date_finished = CASE WHEN ?4 = 1 THEN COALESCE(date_finished, ?3) ELSE date_finished END,
                last_modified = ?3
            WHERE id = ?1"#,
        )
        .bind(id)
        .bind(page)
        .bind(now)
        .bind(finished)
        .execute(self.pool())
        .await?;
        self.get_book(id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("book id {id}")))
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

        // Re-upsert must not un-finish.
        s.upsert_book(&sample()).await.unwrap();
        let b = s.get_book(id).await.unwrap().unwrap();
        assert!(b.finished);
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
