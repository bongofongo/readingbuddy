//! The files readingbuddy owns (`book_files`, migration `0010`).
//!
//! One row per distinct *content*, keyed on its sha256 — see the migration for
//! why the key is global rather than `(book_id, sha256)`. The bytes themselves
//! are the filesystem's business and live in [`crate::files`]; this module is
//! only the record of what is owned and by whom.

use sqlx::Row;

use super::{Storage, now_unix};
use crate::error::Result;

/// One owned file.
///
/// `original_name` is what the file was called when it arrived and is
/// deliberately not part of any lookup: content addressing means the path is
/// derived from `sha256` and `format`, and a filename is a thing that collides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookFile {
    pub sha256: String,
    pub book_id: i64,
    /// Lowercased extension — `epub`, `azw3`, `pdf`. Also the stored file's
    /// extension, so the on-disk address is derivable from the row alone.
    pub format: String,
    pub original_name: Option<String>,
    pub size: i64,
    pub added_at: i64,
}

impl Storage {
    /// Record ownership of a file's content.
    ///
    /// **`DO NOTHING` on conflict, and the return value is "this was new".**
    /// Identical bytes are one file by construction, so a conflict is not an
    /// error — it is level-1 dedup happening, and the caller needs to know it
    /// happened rather than be told it succeeded. Repointing instead would let
    /// a second import quietly move a file to another book, which is the one
    /// thing a content address must never do on its own.
    pub async fn add_book_file(&self, file: &BookFile) -> Result<bool> {
        let done = sqlx::query(
            "INSERT INTO book_files (sha256, book_id, format, original_name, size, added_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (sha256) DO NOTHING",
        )
        .bind(&file.sha256)
        .bind(file.book_id)
        .bind(&file.format)
        .bind(file.original_name.as_ref())
        .bind(file.size)
        .bind(if file.added_at == 0 {
            now_unix()
        } else {
            file.added_at
        })
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The owner of these exact bytes, if we already have them. Dedup level 1.
    pub async fn book_file(&self, sha256: &str) -> Result<Option<BookFile>> {
        let row = sqlx::query(
            "SELECT sha256, book_id, format, original_name, size, added_at
               FROM book_files WHERE sha256 = ?",
        )
        .bind(sha256)
        .fetch_optional(self.pool())
        .await?;
        row.as_ref().map(row_to_file).transpose()
    }

    /// Every file of one book, oldest first. Dedup level 2 read from the other
    /// end: epub and azw3 of the same book are two rows here, not two books.
    pub async fn book_files(&self, book_id: i64) -> Result<Vec<BookFile>> {
        let rows = sqlx::query(
            "SELECT sha256, book_id, format, original_name, size, added_at
               FROM book_files WHERE book_id = ? ORDER BY added_at ASC, sha256 ASC",
        )
        .bind(book_id)
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(row_to_file).collect()
    }

    /// Forget a file. The caller owns the bytes on disk, the same contract
    /// [`Storage::delete_book`] has for a cover.
    pub async fn delete_book_file(&self, sha256: &str) -> Result<bool> {
        let done = sqlx::query("DELETE FROM book_files WHERE sha256 = ?")
            .bind(sha256)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

fn row_to_file(row: &sqlx::sqlite::SqliteRow) -> Result<BookFile> {
    Ok(BookFile {
        sha256: row.try_get("sha256")?,
        book_id: row.try_get("book_id")?,
        format: row.try_get("format")?,
        original_name: row.try_get("original_name")?,
        size: row.try_get("size")?,
        added_at: row.try_get("added_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;

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

    fn file(sha: &str, book_id: i64, format: &str) -> BookFile {
        BookFile {
            sha256: sha.into(),
            book_id,
            format: format.into(),
            original_name: Some(format!("Pachinko.{format}")),
            size: 4096,
            added_at: 1_700_000_000,
        }
    }

    #[tokio::test]
    async fn a_file_round_trips_and_the_same_bytes_insert_once() {
        let (s, book) = seeded().await;
        assert!(s.book_file("aa11").await.unwrap().is_none());

        assert!(s.add_book_file(&file("aa11", book, "epub")).await.unwrap());
        let got = s.book_file("aa11").await.unwrap().unwrap();
        assert_eq!(got, file("aa11", book, "epub"));

        assert!(
            !s.add_book_file(&file("aa11", book, "epub")).await.unwrap(),
            "identical bytes are one file — the second import must report that, not succeed"
        );
    }

    /// Level 2, as a test: two formats of one book are two rows and one
    /// `book_id`. The failure this rules out is the obvious one — a second
    /// format arriving as a second book.
    #[tokio::test]
    async fn two_formats_of_one_book_are_two_rows_and_one_book() {
        let (s, book) = seeded().await;
        s.add_book_file(&file("aa11", book, "epub")).await.unwrap();
        s.add_book_file(&file("bb22", book, "azw3")).await.unwrap();

        let files = s.book_files(book).await.unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.book_id == book));
        let mut formats: Vec<&str> = files.iter().map(|f| f.format.as_str()).collect();
        formats.sort_unstable();
        assert_eq!(formats, ["azw3", "epub"]);
    }

    /// The same bytes may not hang off two books, and the constraint says so
    /// rather than a convention. Two books holding one file means the *books*
    /// are duplicates — the move is `merge_books`, not a second row.
    #[tokio::test]
    async fn the_same_bytes_cannot_belong_to_two_books() {
        let (s, first) = seeded().await;
        let second = s
            .upsert_book(&Book {
                title: Some("Pachinko (the other copy)".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(s.add_book_file(&file("aa11", first, "epub")).await.unwrap());
        assert!(
            !s.add_book_file(&file("aa11", second, "epub"))
                .await
                .unwrap()
        );
        assert_eq!(s.book_file("aa11").await.unwrap().unwrap().book_id, first);
        assert!(s.book_files(second).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn deleting_the_book_cascades_its_files_away() {
        let (s, book) = seeded().await;
        s.add_book_file(&file("aa11", book, "epub")).await.unwrap();
        s.delete_book(book).await.unwrap();
        assert!(
            s.book_file("aa11").await.unwrap().is_none(),
            "foreign keys must be ON for this to hold"
        );
    }

    #[tokio::test]
    async fn a_file_can_be_forgotten_and_forgetting_twice_is_not_an_error() {
        let (s, book) = seeded().await;
        s.add_book_file(&file("aa11", book, "epub")).await.unwrap();
        assert!(s.delete_book_file("aa11").await.unwrap());
        assert!(!s.delete_book_file("aa11").await.unwrap());
    }
}
